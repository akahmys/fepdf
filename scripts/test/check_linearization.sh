#!/bin/bash
# Holds every sample's linearised save against `qpdf --check-linearization` (ROADMAP Y-F31).
#
#   scripts/test/check_linearization.sh [--external]
#
# qpdf is not a dependency of the engine and is not in the gate: it is the reference this
# runs when a change touches the linearised writer, which nothing else here checks — the
# engine reads a linearised file back without reading its hint tables, and the golden
# comparison says only that bytes moved. It says so and stops when qpdf is not there,
# rather than passing.

set -u
QPDF=$(command -v qpdf || true)
if [ -z "$QPDF" ]; then
    echo "qpdf is not installed: brew install qpdf" >&2
    exit 2
fi
INPUTS=(samples/*.pdf)
[ "${1:-}" = "--external" ] && INPUTS+=(target/external/*/*.pdf)
OUT=out/linearization
rm -rf "$OUT"; mkdir -p "$OUT"
cargo run -q --release -p fepdf --example linearize -- "$OUT" "${INPUTS[@]}" || exit 1

KNOWN=scripts/test/linearization_known.tsv
bad=0
for f in "$OUT"/*.pdf; do
    name=$(basename "$f")
    result=$("$QPDF" --check-linearization "$f" 2>&1)
    # A warning listed for this file in linearization_known.tsv is one this engine keeps
    # on purpose (ADR-0108); it is reported, not counted.
    while IFS=$'\t' read -r file warning reason; do
        [ "$file" = "$name" ] || continue
        kept=$(printf '%s\n' "$result" | grep -c "^WARNING.*$warning" || true)
        [ "$kept" -gt 0 ] && echo "$name: $kept known ($reason)"
        result=$(printf '%s\n' "$result" | grep -v "^WARNING.*$warning")
    done < <(grep -v '^#' "$KNOWN")
    warnings=$(printf '%s\n' "$result" | grep -cE '^(WARNING|ERROR)|^qpdf: .*error' || true)
    if [ "$warnings" -gt 0 ] || ! printf '%s' "$result" | grep -qE 'no linearization errors|operation succeeded with warnings'; then
        bad=$((bad + 1))
        echo "$(basename "$f"): $warnings warnings"
        printf '%s\n' "$result" | grep '^WARNING' | sed 's/^WARNING: [^:]*: /  /' \
            | sed -E 's/[0-9]+/N/g' | sort | uniq -c | sort -rn | head -5
    fi
done
echo "$(ls "$OUT"/*.pdf | wc -l | tr -d ' ') linearised, $bad with linearization errors"
[ "$bad" -eq 0 ]
