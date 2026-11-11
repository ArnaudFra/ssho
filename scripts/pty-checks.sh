#!/bin/sh
# Runs every pseudo-terminal scenario against one binary, each in a fresh and
# short fixture directory (the scripts compare paths displayed on 120 columns).
# Fake SSH tools only: no real host, key or agent is used.
#
# Usage: scripts/pty-checks.sh [path/to/ssho]
# SSHO_PTY_TMP overrides the parent of the fixture directory (default /tmp).
set -eu

binary=${1:-target/release/ssho}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
if [ ! -x "$binary" ]; then
    echo "pty-checks: $binary is not an executable; build it first (cargo build --release)" >&2
    exit 2
fi
work=$(mktemp -d "${SSHO_PTY_TMP:-/tmp}/ssho-pty.XXXXXX")
trap 'rm -rf "$work"' EXIT INT TERM

python3 tests/test_pty_screen.py
python3 tests/pty_smoke.py "$binary"
for mode in source-filter file-tags launcher aliases groups; do
    python3 tests/pty_smoke.py "$binary" "--$mode-only"
done
for name in batch keys availability columns initial_query ux completion; do
    python3 "tests/pty_$name.py" "$binary" "$work/$name"
done
echo "PTY checks passed: $binary"
