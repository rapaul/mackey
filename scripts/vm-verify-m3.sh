#!/usr/bin/env bash
#
# M3 VM verification: install the package on a clean golden VM and assert the
# systemd service + mackey system user behave per the milestone and the security
# contract. Run per distro:
#
#   scripts/vm-verify-m3.sh ubuntu
#   scripts/vm-verify-m3.sh fedora
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
vmtest="$repo_root/vmtest/vmtest"
distro="${1:?usage: vm-verify-m3.sh <ubuntu|fedora>}"

case "$distro" in
    ubuntu|ubuntu24) pkg="$(ls -1 target/debian/mackeyd_*_amd64.deb | tail -1)" ;;
    fedora|fedora43|fedora40) pkg="$(ls -1 target/generate-rpm/mackeyd-*.x86_64.rpm | tail -1)" ;;
    *) echo "unknown distro: $distro" >&2; exit 2 ;;
esac
[ -f "$pkg" ] || { echo "package not built: $pkg" >&2; exit 1; }

fail=0
note() { printf '  %s\n' "$*"; }
check() { # check <description> <actual> <expected>
    if [ "$2" = "$3" ]; then note "PASS: $1 ($2)"; else
        note "FAIL: $1 — got [$2], want [$3]"; fail=1
    fi
}
r() { "$vmtest" "$distro" run "$1"; }

echo "== [$distro] start from a clean golden snapshot =="
"$vmtest" "$distro" snapshot reset

echo "== [$distro] record tester groups, then install =="
groups_before="$(r 'id -nG tester')"
"$vmtest" "$distro" install "$pkg"

echo "== [$distro] post-install state =="
check "mackey user exists"   "$(r 'id -u mackey >/dev/null 2>&1 && echo yes || echo no')" "yes"
check "mackey group exists"  "$(r 'getent group mackey >/dev/null 2>&1 && echo yes || echo no')" "yes"
check "service NOT enabled"  "$(r 'systemctl is-enabled mackey.service 2>/dev/null || true')" "disabled"
check "service NOT running"  "$(r 'systemctl is-active mackey.service 2>/dev/null || true')" "inactive"
check "tester groups unchanged" "$(r 'id -nG tester')" "$groups_before"

echo "== [$distro] post-install scriptlet is idempotent (run it twice) =="
b64="$(base64 -w0 packaging/maintainer/postinst)"
check "postinst idempotent" \
    "$(r "echo $b64 | base64 -d >/tmp/pi.sh && sudo sh /tmp/pi.sh && sudo sh /tmp/pi.sh && echo OK")" \
    "OK"

echo "== [$distro] enable --now: active, running as mackey =="
r 'sudo systemctl enable --now mackey.service' >/dev/null
check "service active"        "$(r 'systemctl is-active mackey.service 2>/dev/null || true')" "active"
check "runs as mackey user"   "$(r 'ps -o user= -p "$(pidof mackeyd)" | tr -d " "')" "mackey"

echo "== [$distro] stop completes within 2s =="
check "clean stop <2s"        "$(r 'sudo timeout 2 systemctl stop mackey.service && echo OK || echo SLOW')" "OK"
check "stopped"               "$(r 'systemctl is-active mackey.service 2>/dev/null || true')" "inactive"

echo "== [$distro] reset =="
"$vmtest" "$distro" snapshot reset

[ "$fail" -eq 0 ] && echo "PASS: [$distro] M3 verified" || { echo "M3 verify FAILED for $distro" >&2; exit 1; }
