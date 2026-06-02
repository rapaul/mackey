#!/usr/bin/env bash
#
# M2 deliverable: clean-install the M1 hello-world package on a VM, run mackeyd
# once, and assert it prints its startup line and exits 0. Resets the VM to the
# golden snapshot afterward. Run per distro:
#
#   scripts/vm-verify-m1.sh ubuntu
#   scripts/vm-verify-m1.sh fedora
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
vmtest="$repo_root/vmtest/vmtest"
distro="${1:?usage: vm-verify-m1.sh <ubuntu|fedora>}"

case "$distro" in
    ubuntu|ubuntu24) pkg="$(ls -1 target/debian/mackeyd_*_amd64.deb | tail -1)" ;;
    fedora|fedora43|fedora40) pkg="$(ls -1 target/generate-rpm/mackeyd-*.x86_64.rpm | tail -1)" ;;
    *) echo "unknown distro: $distro" >&2; exit 2 ;;
esac
[ -f "$pkg" ] || { echo "package not built: $pkg (run scripts/build-packages.sh)" >&2; exit 1; }

echo "== [$distro] install $pkg =="
"$vmtest" "$distro" install "$pkg"

echo "== [$distro] run mackeyd once =="
set +e
out="$("$vmtest" "$distro" run mackeyd 2>&1)"; rc=$?
set -e
echo "--- mackeyd output ---"; echo "$out"; echo "----------------------"

fail=0
if [ "$rc" -ne 0 ]; then echo "FAIL: mackeyd exit code $rc (want 0)" >&2; fail=1; fi
if ! grep -q "mackeyd v0.6.0 starting" <<<"$out"; then
    echo "FAIL: startup line not found in output" >&2; fail=1
fi

echo "== [$distro] snapshot reset =="
"$vmtest" "$distro" snapshot reset

[ "$fail" -eq 0 ] && echo "PASS: [$distro] M1 clean-install verified" || { echo "M1 verify FAILED for $distro" >&2; exit 1; }
