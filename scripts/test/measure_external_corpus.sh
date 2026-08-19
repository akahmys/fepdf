#!/usr/bin/env bash
# Runs the engine over a corpus this project did not choose, and counts what fails.
#
# `scripts/test/fetch_external_corpus.sh` puts it in `target/external`. The point is
# stated there and is worth repeating here: every "zero occurrences in the corpus, so
# defer" judgement in `ROADMAP.md` is bounded by nine files this project picked. This
# measures against 242 it did not.
#
# What the first run found, with nine files having found none of it:
#
#   * a **panic** — an unchecked CFF INDEX offset in `fepdf-font`, one byte past the end
#   * `Object Handle<Object>(8) is not a dictionary`, the error message `ROADMAP.md`
#     cites as the bad one it replaced
#   * **clause 7.4**: three of the ten filters the standard defines are implemented, and
#     the roadmap's status table has no row for the clause at all
#
# A debug build is run as well as a release one. Arithmetic overflow is a panic in debug
# and a wrap in release, so a release-only pass can walk past exactly the kind of defect
# this corpus is here to find — the same reason `cli_smoke.sh` is a debug build.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

CORPUS="target/external"
[ -d "$CORPUS" ] || { echo "run scripts/test/fetch_external_corpus.sh first"; exit 1; }
[ -x target/release/fepdf ] || { echo "build first: cargo build --release"; exit 1; }

WORK="${TMPDIR:-/tmp}/fepdf-external"
rm -rf "$WORK"; mkdir -p "$WORK"
BIN="${1:-target/release/fepdf}"
echo "--- $(basename "$(dirname "$BIN")") build: $BIN ---"

total=0; opened=0; panicked=0; refused=0; texted=0; text_failed=0; wrote=0; write_failed=0
: > "$WORK/panics"; : > "$WORK/refused"; : > "$WORK/text"; : > "$WORK/writes"

for f in "$CORPUS"/*/*.pdf; do
    total=$((total + 1))
    name="${f#"$CORPUS"/}"

    # A panic and a refusal are different results and must not be one count. A refusal is
    # the engine declining a file and saying why; a panic is the engine losing control of
    # it, and on deliberately malformed input only the second is a defect on its face.
    if out=$("$BIN" inspect info "$f" 2>&1); then
        opened=$((opened + 1))
    elif printf '%s' "$out" | grep -q "panicked at"; then
        panicked=$((panicked + 1))
        printf '%s\n    %s\n' "$name" \
            "$(printf '%s' "$out" | grep -m1 'panicked at' | cut -c1-120)" >> "$WORK/panics"
        continue
    else
        refused=$((refused + 1))
        printf '%s\n    %s\n' "$name" \
            "$(printf '%s' "$out" | grep -m1 -iE 'error|Error' | cut -c1-120)" >> "$WORK/refused"
        continue
    fi

    if out=$("$BIN" inspect text "$f" 2>&1); then
        texted=$((texted + 1))
    else
        text_failed=$((text_failed + 1))
        printf '%s\n    %s\n' "$name" \
            "$(printf '%s' "$out" | grep -m1 'no text extracted' | cut -c1-120)" >> "$WORK/text"
    fi

    if "$BIN" publish upgrade "$f" "$WORK/out.pdf" >/dev/null 2>&1; then
        wrote=$((wrote + 1))
    else
        write_failed=$((write_failed + 1))
        echo "$name" >> "$WORK/writes"
    fi
done

printf '  %-28s %s\n' "files" "$total"
printf '  %-28s %s\n' "opened" "$opened"
printf '  %-28s %s%s\n' "PANICKED" "$panicked" \
    "$([ "$panicked" -gt 0 ] && echo '  <- the engine lost control of the file')"
printf '  %-28s %s\n' "refused with a message" "$refused"
printf '  %-28s %s of %s opened\n' "every page extracted" "$texted" "$opened"
printf '  %-28s %s of %s opened\n' "written back" "$wrote" "$opened"

for section in panics refused text writes; do
    [ -s "$WORK/$section" ] || continue
    echo
    echo "  --- $section ---"
    head -24 "$WORK/$section" | sed 's/^/    /'
done

# A panic is the only unconditional failure. Refusing a deliberately malformed file and
# saying why is a correct outcome, and this corpus is largely made of such files — so a
# non-zero refusal count is information, not a verdict.
if [ "$panicked" -gt 0 ]; then
    echo
    echo "  FAILED: $panicked file(s) panicked"
    exit 1
fi
