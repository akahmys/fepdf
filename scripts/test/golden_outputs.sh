#!/usr/bin/env bash
# What the engine produces, recorded once and compared after each change (ROADMAP Y-0).
#
#   golden_outputs.sh record [--external]   write the baseline, from a clean tree
#   golden_outputs.sh check  [--external]   write the current outputs and compare
#
# Phase Y moves code without meaning to change what it does. The tests say whether a
# behaviour someone thought of still holds; this says whether *anything* the engine
# produces moved: per input, the bytes of a plain, a linearised and an encrypted save,
# its fonts, its text, its coverage and the decisions taken opening it
# (`crates/fepdf/examples/golden.rs` says what is kept and what is masked).
#
# A difference is not a failure by itself — a fix is meant to make one. It is a
# question: the commit that makes it says why.
#
# The inputs are `samples/*.pdf` and `target/malformed/*.pdf`, and with `--external`
# the 534 files under `target/external/`. One process per input, in parallel.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

MODE="${1:-}"
case "$MODE" in record|check) ;; *) echo "usage: $0 record|check [--external]"; exit 2 ;; esac

INPUTS=(samples/*.pdf target/malformed/*.pdf)
[ "${2:-}" = "--external" ] && INPUTS+=(target/external/*/*.pdf)

if [ "$MODE" = record ] && [ -n "$(git status --porcelain -- crates)" ]; then
    echo "record from a clean tree: the baseline is HEAD's, and crates/ has changes"
    exit 1
fi

cargo build -q -p fepdf --example golden || exit 1
BIN=target/debug/examples/golden

ROOT=out/golden
DEST="$ROOT/$([ "$MODE" = record ] && echo baseline || echo current)"
rm -rf "$DEST"; mkdir -p "$DEST"
[ "$MODE" = record ] && git rev-parse HEAD > "$ROOT/baseline.commit"

start=$(date +%s)
printf '%s\0' "${INPUTS[@]}" | xargs -0 -n 1 -P "$(sysctl -n hw.ncpu 2>/dev/null || nproc)" \
    "$BIN" "$DEST"
status=$?
echo "${#INPUTS[@]} inputs in $(( $(date +%s) - start ))s"
[ $status -eq 0 ] || { echo "the golden run itself failed ($status)"; exit 1; }

[ "$MODE" = record ] && exit 0

[ -d "$ROOT/baseline" ] || { echo "no baseline: run '$0 record' first"; exit 1; }
echo "baseline: $(cat "$ROOT/baseline.commit")"
if diff -rq "$ROOT/baseline" "$DEST"; then
    echo "=== GOLDEN OUTPUTS AGREE ==="
else
    echo "=== GOLDEN OUTPUTS DIFFER ==="
    exit 1
fi
