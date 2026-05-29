#!/usr/bin/env bash
#
# M5 VM verification: mackeyd grabs keyboards (at startup and on hotplug) and
# forwards their events, unmodified, through its virtual output keyboard. Run:
#
#   scripts/vm-verify-m5.sh ubuntu
#   scripts/vm-verify-m5.sh fedora
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
vmtest="$repo_root/vmtest/vmtest"
distro="${1:?usage: vm-verify-m5.sh <ubuntu|fedora>}"

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
journal_grabbed_count() { r 'sudo journalctl -u mackey.service --no-pager 2>/dev/null | grep -c "grabbed " || true'; }

echo "== [$distro] clean snapshot + install + start =="
"$vmtest" "$distro" snapshot reset
"$vmtest" "$distro" install "$pkg"
r 'sudo systemctl enable --now mackey.service' >/dev/null
sleep 1

echo "== [$distro] daemon created virtual keyboard and grabbed existing kbd =="
check "virtual keyboard created" \
    "$(r 'sudo journalctl -u mackey.service --no-pager | grep -q "created virtual keyboard" && echo yes || echo no')" "yes"
check "grabbed >=1 keyboard at startup" \
    "$([ "$(journal_grabbed_count)" -ge 1 ] && echo yes || echo no)" "yes"

echo "== [$distro] install python3-evdev =="
r "$install_evdev" >/dev/null 2>&1
check "python3-evdev present" "$(r 'python3 -c "import evdev" 2>/dev/null && echo yes || echo no')" "yes"

echo "== [$distro] inject on a source keyboard, read off the virtual output =="
b64="$(base64 -w0 scripts/m5_passthrough_test.py)"
out="$(r "echo $b64 | base64 -d >/tmp/m5.py && sudo python3 /tmp/m5.py" 2>&1 || true)"
echo "$out" | sed 's/^/    /'
check "passthrough identity" "$(echo "$out" | grep -q 'RESULT: PASS' && echo PASS || echo FAIL)" "PASS"

echo "== [$distro] hotplugged keyboard is grabbed too =="
before="$(journal_grabbed_count)"
"$vmtest" "$distro" qmp '{"execute":"device_add","arguments":{"driver":"usb-kbd","id":"hotkbd","bus":"xhci.0"}}' >/dev/null
after="$before"
for _ in 1 2 3 4 5 6; do
    after="$(journal_grabbed_count)"
    [ "$after" -gt "$before" ] && break
    sleep 1
done
check "hotplug keyboard grabbed" "$([ "$after" -gt "$before" ] && echo yes || echo no)" "yes"

echo "== [$distro] reset =="
"$vmtest" "$distro" snapshot reset

[ "$fail" -eq 0 ] && echo "PASS: [$distro] M5 verified" || { echo "M5 verify FAILED for $distro" >&2; exit 1; }
