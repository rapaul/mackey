#!/usr/bin/env bash
#
# M6 VM verification: the global keymap rewrites Super+C to Ctrl+C at the event
# level, and stopping the daemon disables the remap. Run:
#
#   scripts/vm-verify-m6.sh ubuntu
#   scripts/vm-verify-m6.sh fedora
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
vmtest="$repo_root/vmtest/vmtest"
distro="${1:?usage: vm-verify-m6.sh <ubuntu|fedora>}"

case "$distro" in
    ubuntu|ubuntu24)
        pkg="$(ls -1 target/debian/mackeyd_*_amd64.deb | tail -1)"
        install_evdev='sudo apt-get update -qq && sudo apt-get install -y -qq python3-evdev' ;;
    fedora|fedora43|fedora40)
        pkg="$(ls -1 target/generate-rpm/mackeyd-*.x86_64.rpm | tail -1)"
        install_evdev='sudo dnf install -y -q python3-evdev' ;;
    *) echo "unknown distro: $distro" >&2; exit 2 ;;
esac
[ -f "$pkg" ] || { echo "package not built: $pkg" >&2; exit 1; }

fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$2" = "$3" ]; then note "PASS: $1 ($2)"; else note "FAIL: $1 — got [$2], want [$3]"; fail=1; fi; }
r() { "$vmtest" "$distro" run "$1"; }
run_keymap_test() {
    local b64; b64="$(base64 -w0 scripts/m6_keymap_test.py)"
    r "echo $b64 | base64 -d >/tmp/m6.py && sudo python3 /tmp/m6.py" 2>&1 || true
}

echo "== [$distro] clean snapshot + install + start + python3-evdev =="
"$vmtest" "$distro" snapshot reset
"$vmtest" "$distro" install "$pkg"
r 'sudo systemctl enable --now mackey.service' >/dev/null
r "$install_evdev" >/dev/null 2>&1
sleep 1

echo "== [$distro] Super+C is rewritten to Ctrl+C =="
out="$(run_keymap_test)"
echo "$out" | sed 's/^/    /'
check "Super+C -> Ctrl+C" "$(echo "$out" | grep -q 'RESULT: PASS' && echo PASS || echo FAIL)" "PASS"

echo "== [$distro] stopping the daemon disables the remap =="
r 'sudo systemctl stop mackey.service' >/dev/null
sleep 1
# With no daemon, there is no virtual output device, so the test can't find it.
out_stopped="$(run_keymap_test)"
check "remap gone when stopped" "$(echo "$out_stopped" | grep -q 'no output device' && echo yes || echo no)" "yes"

echo "== [$distro] reset =="
"$vmtest" "$distro" snapshot reset

[ "$fail" -eq 0 ] && echo "PASS: [$distro] M6 verified" || { echo "M6 verify FAILED for $distro" >&2; exit 1; }
