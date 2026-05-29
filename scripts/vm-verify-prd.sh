#!/usr/bin/env bash
#
# Full PRD §Verification, encoded as one end-to-end scenario per distro (M12).
# Walks a single clean snapshot through the six acceptance points using the
# scriptable equivalents the per-milestone scripts established: D-Bus UpdateFocus
# stands in for live window focus, and evdev injection stands in for typing in
# real apps. The genuinely interactive bits (GUI wizard clicks, a real Pause
# button press) are exercised at the command layer they drive. Run:
#
#   scripts/vm-verify-prd.sh ubuntu
#   scripts/vm-verify-prd.sh fedora
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
vmtest="$repo_root/vmtest/vmtest"
distro="${1:?usage: vm-verify-prd.sh <ubuntu|fedora>}"

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
zip=/usr/share/mackey/gnome-extension/mackey@mackey.app.shell-extension.zip
helper=/usr/libexec/mackey-service-control
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$2" = "$3" ]; then note "PASS: $1 ($2)"; else note "FAIL: $1 — got [$2], want [$3]"; fail=1; fi; }
r() { "$vmtest" "$distro" run "$1"; }
journal_has() { r "sudo journalctl -u mackey.service --no-pager | grep -qF '$1' && echo yes || echo no"; }
set_focus() { r "dbus-send --system --dest=$dest $path ${dest}.UpdateFocus string:$1 string:Test" >/dev/null 2>&1 || true; }
inject() { # $1 = python injector path in repo, $2... = args
    local script="$1"; shift
    local b64; b64="$(base64 -w0 "$script")"
    r "echo $b64 | base64 -d >/tmp/inj.py && sudo python3 /tmp/inj.py $*" 2>&1 || true
}

echo "############ [$distro] PRD verification ############"
"$vmtest" "$distro" snapshot reset
groups_before="$(r 'id -nG tester')"

echo "== 1. clean install does not modify the running system =="
"$vmtest" "$distro" install "$pkg"
groups_after="$(r 'id -nG tester')"
r "$install_evdev" >/dev/null 2>&1
check "mackey user created" "$(r 'getent passwd mackey >/dev/null && echo yes || echo no')" "yes"
check "service NOT auto-enabled" "$(r 'systemctl is-enabled mackey.service 2>/dev/null || echo disabled')" "disabled"
check "groups unchanged by install" "$groups_before" "$groups_after"

echo "== 2. wizard items flip to done within 5s of running their command =="
check "service item pending pre-enable" "$(r 'systemctl is-active mackey.service || true')" "inactive"
r 'sudo systemctl enable --now mackey.service' >/dev/null
sleep 5
check "service item -> done" "$(r 'systemctl is-active mackey.service')" "active"
r "gnome-extensions install --force $zip" >/dev/null 2>&1 || true
r 'gnome-extensions enable mackey@mackey.app' >/dev/null 2>&1 || true
sleep 5
check "extension item -> done" \
    "$(r 'gnome-extensions list --enabled 2>/dev/null | grep -qx mackey@mackey.app && echo yes || echo no')" "yes"

echo "== 3. Super+C / Super+T remap per focused app; groups still unchanged =="
set_focus firefox.desktop
out_c="$(inject scripts/m6_keymap_test.py)"
check "Super+C -> Ctrl+C (copy)" "$(echo "$out_c" | grep -q 'RESULT: PASS' && echo PASS || echo FAIL)" "PASS"
set_focus firefox.desktop
out_t="$(inject scripts/m8_keymap_test.py firefox)"
check "Firefox Super+T -> Ctrl+T" "$(echo "$out_t" | grep -q 'RESULT: PASS' && echo PASS || echo FAIL)" "PASS"
check "id -nG tester unchanged" "$(r 'id -nG tester')" "$groups_before"

echo "== 4. losing focus updates falls back to global within 10s; resuming restores =="
set_focus com.mitchellh.ghostty.desktop
sleep 0.5
check "per-app keymap selected" "$(journal_has 'keymap → com.mitchellh.ghostty.desktop')" "yes"
sleep 7
check "stale fallback warned (<10s)" "$(journal_has 'falling back to global keymap')" "yes"
set_focus firefox.desktop
sleep 0.5
check "re-focus restores per-app keymap" "$(journal_has 'keymap → firefox.desktop')" "yes"

echo "== 5. Pause/Resume via polkit, no auth for the active seat =="
check "polkit grants active seat" \
    "$(r "pkaction --verbose --action-id app.mackey.service-control 2>/dev/null | grep -i 'implicit active' | grep -qi yes && echo yes || echo no")" "yes"
r "sudo $helper stop" >/dev/null 2>&1 || true; sleep 1
check "Pause stops the daemon" "$(r 'systemctl is-active mackey.service || true')" "inactive"
r "sudo $helper start" >/dev/null 2>&1 || true; sleep 1
check "Resume restarts the daemon" "$(r 'systemctl is-active mackey.service')" "active"
r 'id -u mallory >/dev/null 2>&1 || sudo useradd --no-create-home mallory' >/dev/null
check "non-seat user denied" \
    "$(r "sudo -u mallory PKEXEC_DISABLE_AGENT=1 pkexec $helper stop >/dev/null 2>&1 && echo allowed || echo denied")" "denied"

echo "== 6. active under mackey, mackey-only supplementary group, journal clean =="
check "active under mackey" "$(r 'ps -o user= -p "$(pidof mackeyd)" | tr -d " "')" "mackey"
check "mackey is the only supplementary group" \
    "$(r 'ps -o supgrp= -p "$(pidof mackeyd)" | tr -s " " | sed "s/^ *//;s/ *$//"')" "mackey"
# A synthetic typing burst stands in for the 10-minute session.
set_focus firefox.desktop
for _ in 1 2 3; do inject scripts/m6_keymap_test.py >/dev/null; done
check "journal clean of warn+ after typing" \
    "$(r 'sudo journalctl -u mackey.service -p warning --no-pager -q | wc -l')" "0"

echo "== [$distro] reset =="
"$vmtest" "$distro" snapshot reset

[ "$fail" -eq 0 ] && echo "PASS: [$distro] PRD verified" || { echo "PRD verify FAILED for $distro" >&2; exit 1; }
