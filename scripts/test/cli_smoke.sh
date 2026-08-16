#!/usr/bin/env bash
# Starts every CLI subcommand, in a **debug** build, and checks it does not panic.
#
# Written because a regression got through everything else. Adding `--password` to the
# shared ingestion arguments collided with an identically named argument on `SaveArgs`,
# so `publish upgrade` and `publish sign` panicked before parsing anything. Clap's
# duplicate-argument check is a `debug_assert`, and every verification this project runs
# — the test suite, the compliance audit, the PDFKit cross-check — uses `--release`.
# The defect was therefore invisible to all of them while being fatal to anyone running
# a debug build.
#
# `--help` is enough: clap builds the whole command tree before it prints anything, so
# a malformed definition fails there. Hidden subcommands are included deliberately —
# they still parse, and a user who knows the name can still reach them.
#
#   ./scripts/test/cli_smoke.sh
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

cargo build -q -p fepdf-cli || exit 1
cli=target/debug/fepdf

# Discovered, not listed. A hand-written list rots: the first version of this script
# named `debug structure`, which does not exist, and omitted `font-extract` and
# `trace-glyph`, which do. Asking clap what it has means a new subcommand is covered
# the day it is added.
#
# Hidden subcommands do not appear in `--help` and so are not discovered here. They are
# named explicitly below, because they still parse and a user who knows the name can
# still reach them — and because they are the ones most likely to be forgotten.
discover() {
    local prefix="$1"
    # shellcheck disable=SC2086
    "$cli" $prefix --help 2>/dev/null |
        sed -n '/^Commands:/,/^$/p' |
        sed -n 's/^  \([a-z][a-z-]*\) .*/\1/p' |
        grep -v '^help$' |
        while read -r name; do
            printf '%s %s\n' "$prefix" "$name"
            discover "$prefix $name"
        done
}

# `mapfile` is bash 4; macOS ships 3.2, and this project targets macOS.
list=$(mktemp)
trap 'rm -f "$list"' EXIT
{
    echo ""
    discover "" | sed 's/^ *//' | tr -s ' '
    # Hidden subcommands do not appear in `--help`, so they are named here.
    echo "publish sign"
    echo "publish verify-signature"
} > "$list"

status=0
checked=0
while IFS= read -r command; do
    # Word splitting is the point: these are argument lists, not filenames.
    # shellcheck disable=SC2086
    output=$("$cli" $command --help 2>&1)
    checked=$((checked + 1))
    if printf '%s' "$output" | grep -q "panicked at"; then
        printf 'PANIC   fepdf %s\n' "${command:-<root>}"
        printf '%s\n' "$output" | grep -m1 -A1 "panicked at" | sed 's/^/        /'
        status=1
    elif printf '%s' "$output" | grep -qi "^error:"; then
        printf 'ERROR   fepdf %s\n' "${command:-<root>}"
        status=1
    fi
done < "$list"

if [ "$status" -eq 0 ]; then
    printf '%s subcommands start cleanly in a debug build\n' "$checked"
fi
exit $status
