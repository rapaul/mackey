#!/usr/bin/env bash
#
# M4 VM verification: the udev rule grants the mackey group access to /dev/uinput
# and keyboard event nodes (including hotplugged ones), and the daemon opens
# /dev/uinput at startup. Run per distro:
#
#   scripts/vm-verify-m4.sh ubuntu
#   scripts/vm-verify-m4.sh fedora
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
vmtest="$repo_root/vmtest/vmtest"
distro="${1:?usage: vm-verify-m4.sh <ubuntu|fedora>}"

case "$distro" in
    ubuntu|ubuntu24) pkg="$(ls -1 target/debian/mackeyd_*_amd64.deb | tail -1)" ;;
    fedora|fedora43|fedora40) pkg="$(ls -1 target/generate-rpm/mackeyd-*.x86_64.rpm | tail -1)" ;;
    *) echo "unknown distro: $distro" >&2; exit 2 ;;
esac
[ -f "$pkg" ] || { echo "package not built: $pkg" >&2; exit 1; }

fail=0
note() { printf '  %s\n' "$*"; }
check() {
    if [ "$2" = "$3" ]; then note "PASS: $1 ($2)"; else
        note "FAIL: $1 — got [$2], want [$3]"; fail=1
    fi
}
r() { "$vmtest" "$distro" run "$1"; }

# A keyboard event node, found via udev's ID_INPUT_KEYBOARD tag.
find_kbd='for e in /dev/input/event*; do udevadm info -q property -n "$e" 2>/dev/null | grep -q "^ID_INPUT_KEYBOARD=1" && { echo "$e"; break; }; done'

echo "== [$distro] clean snapshot + install =="
"$vmtest" "$distro" snapshot reset
groups_before="$(r 'id -nG tester')"
"$vmtest" "$distro" install "$pkg"

echo "== [$distro] /dev/uinput access =="
check "uinput exists"            "$(r '[ -e /dev/uinput ] && echo yes || echo no')" "yes"
check "uinput group=mackey"      "$(r 'stat -c %G /dev/uinput')" "mackey"
check "uinput group-rw (660)"    "$(r 'stat -c %a /dev/uinput')" "660"

echo "== [$distro] keyboard event node access =="
kbd="$(r "$find_kbd")"
check "keyboard node found"      "$([ -n "$kbd" ] && echo yes || echo no)" "yes"
[ -n "$kbd" ] && check "keyboard node group=mackey" "$(r "stat -c %G $kbd")" "mackey"

check "tester groups unchanged" "$(r 'id -nG tester')" "$groups_before"

echo "== [$distro] daemon opens /dev/uinput =="
r 'sudo systemctl enable --now mackey.service' >/dev/null
opened=no
for _ in 1 2 3 4 5; do
    if r 'sudo journalctl -u mackey.service --no-pager 2>/dev/null | grep -q "opened /dev/uinput"'; then
        opened=yes; break
    fi
    sleep 1
done
check "journal: opened /dev/uinput" "$opened" "yes"

echo "== [$distro] hotplugged keyboard gets the ACL =="
before="$(r 'ls /dev/input/event* 2>/dev/null | sort')"
"$vmtest" "$distro" qmp '{"execute":"device_add","arguments":{"driver":"usb-kbd","id":"hotkbd","bus":"xhci.0"}}' >/dev/null
new=""
for _ in 1 2 3 4 5; do
    after="$(r 'ls /dev/input/event* 2>/dev/null | sort')"
    new="$(comm -13 <(printf '%s\n' "$before") <(printf '%s\n' "$after") | head -1)"
    [ -n "$new" ] && break
    sleep 1
done
check "hotplug node appeared" "$([ -n "$new" ] && echo yes || echo no)" "yes"
if [ -n "$new" ]; then
    grp=""
    for _ in 1 2; do grp="$(r "stat -c %G $new")"; [ "$grp" = mackey ] && break; sleep 1; done
    check "hotplug node group=mackey (<2s)" "$grp" "mackey"
fi

echo "== [$distro] reset =="
"$vmtest" "$distro" snapshot reset

[ "$fail" -eq 0 ] && echo "PASS: [$distro] M4 verified" || { echo "M4 verify FAILED for $distro" >&2; exit 1; }
