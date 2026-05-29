#!/usr/bin/env bash
#
# Local release pipeline (M12).
#
# There is no hosted CI in this project (see CLAUDE.md), so the "tagged-release
# pipeline" runs here: it gates a release on the full cargo gate, the package
# build, and the PRD verification scenario passing in BOTH local VMs, then
# collects the artifacts under dist/. Publishing (e.g. `gh release create`) is
# deliberately left as a manual step — this script never touches a remote.
#
#   scripts/release.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "== cargo gate =="
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

echo "== build packages =="
scripts/build-packages.sh

echo "== PRD verification (both distros) =="
for d in fedora ubuntu; do
    scripts/vm-verify-prd.sh "$d"
done

echo "== collect artifacts =="
mkdir -p dist
deb="$(ls -1 target/debian/mackeyd_*_amd64.deb | tail -1)"
rpm="$(ls -1 target/generate-rpm/mackeyd-*.x86_64.rpm | tail -1)"
cp -v "$deb" "$rpm" dist/

echo
echo "Release artifacts ready in dist/:"
ls -1 dist/
echo
echo "All gates passed. Publish manually when ready, e.g.:"
echo "  gh release create vX.Y.Z dist/* --notes-file <notes>"
