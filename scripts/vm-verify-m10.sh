#!/usr/bin/env bash
#
# M10 VM verification: the package ships the polkit action and helper, polkit
# grants the active seat user start/stop of mackey.service (and nothing else)
# without a prompt, the helper only ever touches mackey.service, a remote/other
# user is denied, and the login user's groups are unchanged. The status view's
# Pause/Resume widget behaviour is covered by the cargo tests; clicking it in the
# live session is the manual scenario in MILESTONES.md. Run:
#
#   scripts/vm-verify-m10.sh ubuntu
#   scripts/vm-verify-m10.sh fedora
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
vmtest="$repo_root/vmtest/vmtest"
distro="${1:?usage: vm-verify-m10.sh <ubuntu|fedora>}"

case "$distro" in
    ubuntu|ubuntu24) pkg="$(ls -1 target/debian/mackeyd_*_amd64.deb | tail -1)" ;;
    fedora|fedora43|fedora40) pkg="$(ls -1 target/generate-rpm/mackeyd-*.x86_64.rpm | tail -1)" ;;
    *) echo "unknown distro: $distro" >&2; exit 2 ;;
esac
[ -f "$pkg" ] || { echo "package not built: $pkg (run scripts/build-packages.sh)" >&2; exit 1; }

helper=/usr/libexec/mackey-service-control
policy=/usr/share/polkit-1/actions/app.mackey.policy
action=app.mackey.service-control
fail=0
note() { printf '  %s\n' "$*"; }
check() { if [ "$2" = "$3" ]; then note "PASS: $1 ($2)"; else note "FAIL: $1 — got [$2], want [$3]"; fail=1; fi; }
r() { "$vmtest" "$distro" run "$1"; }

echo "== [$distro] clean snapshot =="
"$vmtest" "$distro" snapshot reset
groups_before="$(r 'id -nG tester')"

echo "== [$distro] install + start =="
"$vmtest" "$distro" install "$pkg"
groups_after="$(r 'id -nG tester')"
r 'sudo systemctl enable --now mackey.service' >/dev/null
sleep 1

echo "== [$distro] login user's groups unchanged by install =="
check "id -nG tester identical" "$groups_before" "$groups_after"

echo "== [$distro] polkit action and helper are installed =="
check "helper executable" "$(r "test -x $helper && echo yes || echo no")" "yes"
check "policy present" "$(r "test -f $policy && echo yes || echo no")" "yes"
check "polkit knows the action" \
    "$(r "pkaction --action-id $action >/dev/null 2>&1 && echo yes || echo no")" "yes"
check "action allows active seat without auth" \
    "$(r "pkaction --verbose --action-id $action 2>/dev/null | grep -i 'implicit active' | grep -qi yes && echo yes || echo no")" "yes"

echo "== [$distro] the helper controls exactly mackey.service =="
r "sudo $helper stop" >/dev/null 2>&1 || true
sleep 1
check "helper stop pauses the daemon" "$(r 'systemctl is-active mackey.service || true')" "inactive"
r "sudo $helper start" >/dev/null 2>&1 || true
sleep 1
check "helper start resumes the daemon" "$(r 'systemctl is-active mackey.service || true')" "active"
check "helper rejects other verbs" \
    "$(r "sudo $helper restart >/dev/null 2>&1; echo \$?")" "2"

echo "== [$distro] a different (non-seat) user is denied =="
r 'id -u mallory >/dev/null 2>&1 || sudo useradd --no-create-home mallory' >/dev/null
check "mallory denied by polkit" \
    "$(r "sudo -u mallory PKEXEC_DISABLE_AGENT=1 pkexec $helper stop >/dev/null 2>&1 && echo allowed || echo denied")" \
    "denied"

echo "== [$distro] active-seat no-prompt check (informational) =="
note "INFO: $(r "PKEXEC_DISABLE_AGENT=1 pkexec $helper stop >/dev/null 2>&1 && echo 'allowed without prompt' || echo 'denied (expected unless run in the active graphical seat session)'")"
r "sudo $helper start" >/dev/null 2>&1 || true

echo "== [$distro] reset =="
"$vmtest" "$distro" snapshot reset

[ "$fail" -eq 0 ] && echo "PASS: [$distro] M10 verified" || { echo "M10 verify FAILED for $distro" >&2; exit 1; }
