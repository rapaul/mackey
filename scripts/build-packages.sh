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

echo "== gnome extension =="
ext_out="target/gnome-extension"
mkdir -p "$ext_out"
# `gnome-extensions pack` is the structural lint: it validates metadata.json and
# the bundle layout, and fails the build on a malformed extension.
gnome-extensions pack gnome-extension --out-dir "$ext_out" --force
echo "  -> $ext_out/mackey@mackey.app.shell-extension.zip"
# eslint is best-effort (not installed on the dev box by default); skip if absent
# rather than pulling it from the network, mirroring the rpmlint handling below.
if npx --no-install eslint --version >/dev/null 2>&1; then
    echo "== eslint =="
    (cd gnome-extension && npx --no-install eslint extension.js)
else
    echo "!! eslint not found; skipping (run 'npm i -g eslint' to enable)"
fi

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
