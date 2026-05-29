#!/usr/bin/env bash
#
# M11 VM verification: the hardened mackey.service still runs (as the mackey
# user), still remaps keys (Super+C -> Ctrl+C) under all the sandboxing, the
# hardening directives are actually applied, the journal stays clean of warnings
# during a synthetic typing burst, and the login user's groups are unchanged.
# Run:
#
#   scripts/vm-verify-m11.sh ubuntu
#   scripts/vm-verify-m11.sh fedora
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
vmtest="$repo_root/vmtest/vmtest"
distro="${1:?usage: vm-verify-m11.sh <ubuntu|fedora>}"

case "$distro" in
    ubuntu|ubuntu24)
        pkg="$(ls -1 target/debian/mackeyd_*_amd64.deb | tail -1)"
        install_evdev='sudo apt-get update -qq && sudo apt-get install -y -qq python3-evdev' ;;
    fedora|fedora43|fedora40)
        pkg="$(ls -1 target/generate-rpm/mackeyd-*.x86_64.rpm | tail -1)"
        install_evdev='sudo dnf install -y -q python3-evdev' ;;
    *) echo "unknown distro: $distro" >&2; exit 2 ;;
esac
[ -f "$pkg" ] || { echo "package not built: $pkg (run scripts/build-packages.sh)" >&2; exit 1; }

fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$2" = "$3" ]; then note "PASS: $1 ($2)"; else note "FAIL: $1 — got [$2], want [$3]"; fail=1; fi; }
r() { "$vmtest" "$distro" run "$1"; }
showp() { r "systemctl show mackey.service -p $1 --value"; }
run_keymap_test() {
    local b64; b64="$(base64 -w0 scripts/m6_keymap_test.py)"
    r "echo $b64 | base64 -d >/tmp/m6.py && sudo python3 /tmp/m6.py" 2>&1 || true
}

echo "== [$distro] clean snapshot + install + start =="
"$vmtest" "$distro" snapshot reset
groups_before="$(r 'id -nG tester')"
"$vmtest" "$distro" install "$pkg"
groups_after="$(r 'id -nG tester')"
r 'sudo systemctl enable --now mackey.service' >/dev/null
r "$install_evdev" >/dev/null 2>&1
sleep 2

echo "== [$distro] hardened unit is active under the mackey user =="
check "service active" "$(r 'systemctl is-active mackey.service')" "active"
check "runs as mackey" "$(r 'ps -o user= -p "$(pidof mackeyd)" | tr -d " "')" "mackey"
check "groups unchanged by install" "$groups_before" "$groups_after"

echo "== [$distro] hardening directives are applied =="
check "NoNewPrivileges" "$(showp NoNewPrivileges)" "yes"
check "ProtectSystem" "$(showp ProtectSystem)" "strict"
check "PrivateNetwork" "$(showp PrivateNetwork)" "yes"
check "MemoryDenyWriteExecute" "$(showp MemoryDenyWriteExecute)" "yes"
check "CapabilityBoundingSet empty" "$(showp CapabilityBoundingSet)" ""
check "RestrictAddressFamilies has AF_UNIX" \
    "$(showp RestrictAddressFamilies | grep -qw AF_UNIX && echo yes || echo no)" "yes"
check "SystemCallFilter set" \
    "$(test -n \"$(showp SystemCallFilter)\" && echo yes || echo no)" "yes"

echo "== [$distro] remapping still works under hardening =="
out="$(run_keymap_test)"
echo "$out" | sed 's/^/    /'
check "Super+C -> Ctrl+C" "$(echo "$out" | grep -q 'RESULT: PASS' && echo PASS || echo FAIL)" "PASS"

echo "== [$distro] journal is clean of warnings (no sandbox denials) =="
# A clean run logs only info; any syscall/namespace denial surfaces at warn+.
check "no warn+ journal lines" \
    "$(r 'sudo journalctl -u mackey.service -p warning --no-pager -q | wc -l')" "0"

echo "== [$distro] exposure score (informational; systemd version dependent) =="
note "INFO: $(r 'systemd-analyze security mackey.service --no-pager 2>/dev/null | tail -1' || echo n/a)"

echo "== [$distro] reset =="
"$vmtest" "$distro" snapshot reset

[ "$fail" -eq 0 ] && echo "PASS: [$distro] M11 verified" || { echo "M11 verify FAILED for $distro" >&2; exit 1; }
