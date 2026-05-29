#!/usr/bin/env bash
# Build mackey's .deb and .rpm locally and lint them.
#
# This is the local stand-in for the package-build CI job: there is no hosted
# CI in this project, everything runs on the dev box (and, from M2, in local
# VMs). lintian is Debian-only and runs inside the Ubuntu VM via `vmtest`.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "== building release binaries (daemon + GUI) =="
cargo build --release -p mackeyd -p mackey-gui

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

echo "== systemd unit hardening =="
unit=packaging/mackey.service
baseline_file=packaging/mackey.service.exposure-baseline
if command -v systemd-analyze >/dev/null; then
    systemd-analyze verify "$unit" || true   # warns the binary isn't installed; not fatal
    score="$(systemd-analyze security --offline=true "$unit" 2>/dev/null \
        | sed -n 's/.*Overall exposure level[^:]*:[[:space:]]*\([0-9.]*\).*/\1/p')"
    baseline="$(grep -vE '^[[:space:]]*#' "$baseline_file" | tr -d '[:space:]')"
    echo "  -> exposure $score (baseline $baseline)"
    if awk "BEGIN { exit !($score > $baseline) }"; then
        echo "!! security regression: exposure $score is worse than baseline $baseline" >&2
        echo "   review the change or, if intentional tightening, lower $baseline_file" >&2
        exit 1
    fi
else
    echo "!! systemd-analyze not found; skipping hardening check"
fi

echo "== polkit policy =="
policy=packaging/polkit/app.mackey.policy
dtd=/usr/share/polkit-1/policyconfig-1.dtd
if command -v xmllint >/dev/null; then
    if [ -f "$dtd" ]; then
        # --nonet so the DOCTYPE's external DTD URL is not fetched; validation
        # uses the locally-installed polkit DTD instead.
        xmllint --noout --nonet --dtdvalid "$dtd" "$policy"
        echo "  -> valid against $dtd"
    else
        xmllint --noout "$policy"
        echo "  -> well-formed (polkit DTD not installed; DTD validation skipped)"
    fi
else
    echo "!! xmllint not found; skipping polkit validation"
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
    # M11: warnings are errors now. Any non-filtered warning/error fails the build.
    rpmlint_out="$(rpmlint -c packaging/rpmlint.toml "$rpm")"
    echo "$rpmlint_out"
    if ! echo "$rpmlint_out" | grep -qE '\b0 errors, 0 warnings\b'; then
        echo "!! rpmlint reported warnings or errors (M11: these fail the build)" >&2
        exit 1
    fi
else
    echo "!! rpmlint not found; skipping (runs in the Fedora VM)"
fi

echo "OK"
