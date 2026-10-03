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

bad=0
for f in "$OUT"/*.pdf; do
    result=$("$QPDF" --check-linearization "$f" 2>&1)
    warnings=$(printf '%s\n' "$result" | grep -c '^WARNING' || true)
    if [ "$warnings" -gt 0 ] || ! printf '%s' "$result" | grep -q 'no linearization errors'; then
        bad=$((bad + 1))
        echo "$(basename "$f"): $warnings warnings"
        printf '%s\n' "$result" | grep '^WARNING' | sed 's/^WARNING: [^:]*: /  /' \
            | sed -E 's/[0-9]+/N/g' | sort | uniq -c | sort -rn | head -5
    fi
done
echo "$(ls "$OUT"/*.pdf | wc -l | tr -d ' ') linearised, $bad with linearization errors"
[ "$bad" -eq 0 ]
