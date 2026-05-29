#!/usr/bin/env bash
#
# M7 VM verification: the daemon owns app.mackey.FocusTracker on the system bus,
# accepts UpdateFocus from the active-seat user, rejects it from anyone else, and
# the D-Bus policy permits only the UpdateFocus member. Run:
#
#   scripts/vm-verify-m7.sh ubuntu
#   scripts/vm-verify-m7.sh fedora
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
vmtest="$repo_root/vmtest/vmtest"
distro="${1:?usage: vm-verify-m7.sh <ubuntu|fedora>}"

case "$distro" in
    ubuntu|ubuntu24) pkg="$(ls -1 target/debian/mackeyd_*_amd64.deb | tail -1)" ;;
    fedora|fedora43|fedora40) pkg="$(ls -1 target/generate-rpm/mackeyd-*.x86_64.rpm | tail -1)" ;;
    *) echo "unknown distro: $distro" >&2; exit 2 ;;
esac
[ -f "$pkg" ] || { echo "package not built: $pkg" >&2; exit 1; }

dest=app.mackey.FocusTracker
path=/app/mackey/FocusTracker
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$2" = "$3" ]; then note "PASS: $1 ($2)"; else note "FAIL: $1 — got [$2], want [$3]"; fail=1; fi; }
r() { "$vmtest" "$distro" run "$1"; }
journal_has() { r "sudo journalctl -u mackey.service --no-pager | grep -qF '$1' && echo yes || echo no"; }

echo "== [$distro] clean snapshot + install + start =="
"$vmtest" "$distro" snapshot reset
"$vmtest" "$distro" install "$pkg"
r 'sudo systemctl enable --now mackey.service' >/dev/null
sleep 2

echo "== [$distro] daemon owns the well-known name =="
check "name on system bus" \
    "$(r "busctl --system list --no-pager | grep -q $dest && echo yes || echo no")" "yes"

echo "== [$distro] UpdateFocus from the active-seat user (tester) is accepted =="
r "dbus-send --system --print-reply --dest=$dest $path ${dest}.UpdateFocus string:firefox.desktop string:Test" >/dev/null 2>&1 || true
sleep 0.5
check "journal: accepted" "$(journal_has 'accepted UpdateFocus app_id=firefox.desktop')" "yes"

echo "== [$distro] UpdateFocus from a different user is rejected =="
r 'id -u mallory >/dev/null 2>&1 || sudo useradd --no-create-home mallory' >/dev/null
mallory_uid="$(r 'id -u mallory')"
r "sudo -u mallory dbus-send --system --dest=$dest $path ${dest}.UpdateFocus string:evil.desktop string:x" >/dev/null 2>&1 || true
sleep 0.5
check "journal: rejected uid=$mallory_uid" "$(journal_has "rejected UpdateFocus from uid=$mallory_uid")" "yes"
# And the spoofed app id must NOT have been recorded as accepted.
check "spoof not accepted" "$(journal_has 'accepted UpdateFocus app_id=evil.desktop')" "no"

echo "== [$distro] D-Bus policy permits only UpdateFocus =="
out="$(r "dbus-send --system --print-reply --dest=$dest $path org.freedesktop.DBus.Introspectable.Introspect 2>&1" || true)"
check "Introspect denied by policy" \
    "$(echo "$out" | grep -qiE 'AccessDenied|Rejected send' && echo denied || echo allowed)" "denied"

echo "== [$distro] reset =="
"$vmtest" "$distro" snapshot reset

[ "$fail" -eq 0 ] && echo "PASS: [$distro] M7 verified" || { echo "M7 verify FAILED for $distro" >&2; exit 1; }
