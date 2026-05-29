#!/usr/bin/env bash
# Build mackey's .deb and .rpm locally and lint them.
#
# This is the local stand-in for the package-build CI job: there is no hosted
# CI in this project, everything runs on the dev box (and, from M2, in local
# VMs). lintian is Debian-only and runs inside the Ubuntu VM via `vmtest`.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "== building release binary =="
cargo build --release -p mackeyd

echo "== cargo deb =="
deb="$(cargo deb -p mackeyd | tail -1)"
echo "  -> $deb"

echo "== cargo generate-rpm =="
cargo generate-rpm -p mackeyd >/dev/null
rpm="$(ls -1 target/generate-rpm/mackeyd-*.rpm | tail -1)"
echo "  -> $rpm"

echo "== rpm metadata =="
rpm -qpi "$rpm"
echo "-- files --"
rpm -qpl "$rpm"

if command -v rpmlint >/dev/null; then
    echo "== rpmlint =="
    rpmlint -c packaging/rpmlint.toml "$rpm"
else
    echo "!! rpmlint not found; skipping (runs in the Fedora VM)"
fi

echo "OK"
