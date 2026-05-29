#!/usr/bin/env bash
#
# M9 VM verification: the package ships the setup GUI (/usr/bin/mackey) and its
# .desktop launcher, the launcher validates, the binary's GTK/libadwaita deps
# resolve on a clean install, and the login user's groups are unchanged. The
# wizard's state-driven widget behaviour is covered by the cargo UI test
# (mackey-gui: flipping_state_swaps_the_visible_page); the interactive
# click-through is the manual scenario in MILESTONES.md. Run:
#
#   scripts/vm-verify-m9.sh ubuntu
#   scripts/vm-verify-m9.sh fedora
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
vmtest="$repo_root/vmtest/vmtest"
distro="${1:?usage: vm-verify-m9.sh <ubuntu|fedora>}"

case "$distro" in
    ubuntu|ubuntu24) pkg="$(ls -1 target/debian/mackeyd_*_amd64.deb | tail -1)" ;;
    fedora|fedora43|fedora40) pkg="$(ls -1 target/generate-rpm/mackeyd-*.x86_64.rpm | tail -1)" ;;
    *) echo "unknown distro: $distro" >&2; exit 2 ;;
esac
[ -f "$pkg" ] || { echo "package not built: $pkg (run scripts/build-packages.sh)" >&2; exit 1; }

mackey=/usr/bin/mackey
desktop=/usr/share/applications/mackey.desktop
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$2" = "$3" ]; then note "PASS: $1 ($2)"; else note "FAIL: $1 — got [$2], want [$3]"; fail=1; fi; }
r() { "$vmtest" "$distro" run "$1"; }

echo "== [$distro] clean snapshot =="
"$vmtest" "$distro" snapshot reset
groups_before="$(r 'id -nG tester')"

echo "== [$distro] install =="
"$vmtest" "$distro" install "$pkg"
groups_after="$(r 'id -nG tester')"

echo "== [$distro] login user's groups unchanged by install =="
check "id -nG tester identical" "$groups_before" "$groups_after"

echo "== [$distro] the setup GUI and launcher are installed =="
check "mackey binary executable" "$(r "test -x $mackey && echo yes || echo no")" "yes"
check "desktop launcher present" "$(r "test -f $desktop && echo yes || echo no")" "yes"
check "desktop launcher Exec=mackey" \
    "$(r "grep -q '^Exec=mackey$' $desktop && echo yes || echo no")" "yes"

echo "== [$distro] desktop file validates =="
check "desktop-file-validate clean" \
    "$(r "command -v desktop-file-validate >/dev/null && (desktop-file-validate $desktop && echo ok || echo bad) || echo skipped")" \
    "$(r "command -v desktop-file-validate >/dev/null && echo ok || echo skipped")"

echo "== [$distro] GTK/libadwaita runtime deps resolve =="
check "no missing shared libs" \
    "$(r "ldd $mackey 2>/dev/null | grep -c 'not found'")" "0"

echo "== [$distro] reset =="
"$vmtest" "$distro" snapshot reset

[ "$fail" -eq 0 ] && echo "PASS: [$distro] M9 verified" || { echo "M9 verify FAILED for $distro" >&2; exit 1; }
