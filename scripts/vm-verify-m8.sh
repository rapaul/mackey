#!/usr/bin/env bash
#
# M8 VM verification: the package installs the GNOME extension system-wide,
# UpdateFocus switches the active keymap (logged), Super+T is rewritten per the
# focused app's keymap (Ctrl+Shift+T in Ghostty vs plain Ctrl+T in Firefox), and
# the keymap reverts to global with a warning after >5s of focus silence. The
# login user's groups are unchanged by the install. Run:
#
#   scripts/vm-verify-m8.sh ubuntu
#   scripts/vm-verify-m8.sh fedora
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
vmtest="$repo_root/vmtest/vmtest"
distro="${1:?usage: vm-verify-m8.sh <ubuntu|fedora>}"

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

dest=app.mackey.FocusTracker
path=/app/mackey/FocusTracker
ext_dir=/usr/share/gnome-shell/extensions/mackey@mackey.app
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$2" = "$3" ]; then note "PASS: $1 ($2)"; else note "FAIL: $1 — got [$2], want [$3]"; fail=1; fi; }
r() { "$vmtest" "$distro" run "$1"; }
journal_has() { r "sudo journalctl -u mackey.service --no-pager | grep -qF '$1' && echo yes || echo no"; }
set_focus() { r "gdbus call --system --dest $dest --object-path $path --method ${dest}.UpdateFocus $1 Test" >/dev/null 2>&1 || true; }
run_keymap_test() {
    local b64; b64="$(base64 -w0 scripts/m8_keymap_test.py)"
    r "echo $b64 | base64 -d >/tmp/m8.py && sudo python3 /tmp/m8.py $1" 2>&1 || true
}

echo "== [$distro] clean snapshot =="
"$vmtest" "$distro" snapshot reset
groups_before="$(r 'id -nG tester')"

echo "== [$distro] install + start + python3-evdev =="
"$vmtest" "$distro" install "$pkg"
groups_after="$(r 'id -nG tester')"
r 'sudo systemctl enable --now mackey.service' >/dev/null
r "$install_evdev" >/dev/null 2>&1
sleep 1

echo "== [$distro] login user's groups unchanged by install =="
check "id -nG tester identical" "$groups_before" "$groups_after"

echo "== [$distro] the GNOME extension is installed system-wide by the package =="
check "extension metadata present" "$(r "test -f '$ext_dir/metadata.json' && echo yes || echo no")" "yes"
check "extension listed by gnome-extensions" \
    "$(r "gnome-extensions list 2>/dev/null | grep -q mackey@mackey.app && echo yes || echo no")" "yes"

echo "== [$distro] UpdateFocus switches the active keymap (logged) =="
set_focus firefox.desktop
sleep 0.5
check "journal: keymap → firefox" "$(journal_has 'keymap → firefox.desktop')" "yes"
set_focus com.mitchellh.ghostty.desktop
sleep 0.5
check "journal: keymap → ghostty" "$(journal_has 'keymap → com.mitchellh.ghostty.desktop')" "yes"

echo "== [$distro] Super+T is rewritten per the focused app =="
set_focus com.mitchellh.ghostty.desktop
out_g="$(run_keymap_test ghostty)"
echo "$out_g" | sed 's/^/    /'
check "Ghostty: Super+T -> Ctrl+Shift+T" \
    "$(echo "$out_g" | grep -q 'RESULT: PASS' && echo PASS || echo FAIL)" "PASS"
set_focus firefox.desktop
out_f="$(run_keymap_test firefox)"
echo "$out_f" | sed 's/^/    /'
check "Firefox: Super+T -> Ctrl+T" \
    "$(echo "$out_f" | grep -q 'RESULT: PASS' && echo PASS || echo FAIL)" "PASS"

echo "== [$distro] keymap reverts to global after >5s of focus silence =="
set_focus com.mitchellh.ghostty.desktop
sleep 7
check "journal: stale fallback warning" \
    "$(journal_has 'falling back to global keymap')" "yes"

echo "== [$distro] reset =="
"$vmtest" "$distro" snapshot reset

[ "$fail" -eq 0 ] && echo "PASS: [$distro] M8 verified" || { echo "M8 verify FAILED for $distro" >&2; exit 1; }
