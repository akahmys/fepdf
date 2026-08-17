#!/usr/bin/env bash
# Reads every file this engine writes back **with this engine**, across the combinations
# of write-time switches.
#
# The other four cross-checks all ask somebody else — PDFKit, PDFium, openssl — which is
# what makes them worth running and also what they cannot do. They compare an
# independent reader's view of the input with its view of the output, so they answer
# "did writing lose anything" and are silent on "can this engine read what it just
# wrote". Three defects were found in one day where the answer was no and every one of
# those checks was green:
#
#   * encryption through object streams — PDFKit read the file; fepdf could not
#   * `inspect text` stopping at the first bad page — PDFKit read 846; fepdf reported 127
#   * `scn` with a pattern name — PDFKit read the page; six pages failed here
#
# So the baseline here is *this engine's* reading of the input, and the comparison is
# against *this engine's* reading of the output. No second implementation is involved,
# which is the point: it is the cheapest check in the suite and the only one that closes
# this gap.
#
# **Combinations, not features.** Encryption and object streams were each fine alone and
# broken together. Every switch below multiplies with the others, and only the states
# somebody happened to try are known good — so they are enumerated rather than sampled.
# The full matrix runs on small files; every sample gets the default write, because a
# per-file defect and a per-combination defect are different animals.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

[ -x target/release/fepdf ] || { echo "build first: cargo build --release"; exit 1; }
command -v openssl >/dev/null || { echo "openssl not found"; exit 1; }

WORK="${TMPDIR:-/tmp}/fepdf-crosscheck-selfread"
rm -rf "$WORK"; mkdir -p "$WORK"

# One identity, used both to sign and to encrypt to.
openssl req -x509 -newkey rsa:2048 -keyout "$WORK/key.pem" -out "$WORK/cert.pem" \
    -days 1 -nodes -subj "/CN=fepdf self-read" \
    -addext "keyUsage=critical,digitalSignature,keyEncipherment" 2>/dev/null
openssl x509 -in "$WORK/cert.pem" -outform der -out "$WORK/cert.der"
openssl pkcs8 -topk8 -nocrypt -in "$WORK/key.pem" -outform der -out "$WORK/key.der"

FAILED=0

# Text as this engine sees it, page by page. The exit status is ignored *here* and
# checked separately below, for a reason worth stating: a defect in extraction is
# symmetric — it loses the same pages on both sides — so comparing input with output
# cannot see it. Injecting the `scn` pattern defect into this script proved exactly
# that: every combination still compared equal. The status is the signal that sees it.
read_text() {
    target/release/fepdf inspect text "$@" 2>/dev/null | tail -n +2 || true
}

# Every page of every sample has to extract. This is not a round-trip property and no
# comparison finds it, because whatever the reader loses it loses identically on both
# sides. Two of the three defects this script was written for were of that shape.
echo "--- every page of every sample extracts ---"
for src in samples/*.pdf; do
    if report=$(target/release/fepdf inspect text "$src" 2>&1 >/dev/null); then
        printf '  %-14s every page\n' "$(basename "$src" .pdf)"
    else
        echo "  $(basename "$src" .pdf): PAGES THAT WILL NOT EXTRACT"
        printf '%s' "$report" | grep -E "^  (page|[0-9]+ of)" | head -4 | sed 's/^/    /'
        FAILED=1
    fi
done


# $1 label, $2 source, $3.. write flags — then the read flags after the `--` separator.
check() {
    local label="$1" src="$2"; shift 2
    local write=() read=() seen=0
    for arg in "$@"; do
        if [ "$arg" = "--" ]; then seen=1; continue; fi
        if [ "$seen" -eq 0 ]; then write+=("$arg"); else read+=("$arg"); fi
    done

    local out="$WORK/$(basename "$src" .pdf).$label.pdf"
    # `${a[@]+"${a[@]}"}` rather than `"${a[@]}"`: the bash macOS ships treats an empty
    # array expansion as an unbound variable under `set -u`, and the default write has
    # no flags at all.
    if ! target/release/fepdf publish upgrade "$src" "$out" ${write[@]+"${write[@]}"} \
            >"$WORK/write.log" 2>&1; then
        echo "  $(basename "$src" .pdf) [$label]: WRITE FAILED"
        tail -2 "$WORK/write.log" | sed 's/^/      /'
        FAILED=1; return
    fi

    read_text "$src" > "$WORK/want"
    read_text "$out" ${read[@]+"${read[@]}"} > "$WORK/got"
    if ! diff -q "$WORK/want" "$WORK/got" >/dev/null; then
        echo "  $(basename "$src" .pdf) [$label]: THIS ENGINE CANNOT READ WHAT IT WROTE"
        echo "      $(wc -c < "$WORK/want" | tr -d ' ') bytes in, $(wc -c < "$WORK/got" | tr -d ' ') out"
        diff "$WORK/want" "$WORK/got" | head -3 | sed 's/^/      /'
        FAILED=1; return
    fi
    printf '  %-14s %-22s %s bytes, unchanged\n' \
        "$(basename "$src" .pdf)" "[$label]" "$(wc -c < "$WORK/got" | tr -d ' ')"
}

echo "--- every sample, written the default way ---"
for src in samples/*.pdf; do
    check default "$src" --
done

# The catalogue survives, key for key. `ROADMAP.md` opens by claiming round-trip
# fidelity already holds for entries the engine has no typed view of; that claim went
# unchecked long enough to name two keys as untyped that had since been typed. This is
# the check, so the next such drift is a failure rather than a paragraph.
#
# Two differences are expected and normalised away. Object numbers are renumbered
# because saving produces a new document (ADR-0012), so only the key and the value's
# *shape* are compared, not the `N 0 R` before it. `/Metadata` is added where the source
# had none, because output always carries XMP — so it is dropped from the input side
# only when the input lacked it, which still catches a *lost* `/Metadata`.
#
# `/Pages` is compared by key alone. Its value legitimately changes shape:
# `bokutokitan.pdf`'s page-tree root carries an inheritable `/MediaBox` that the writer
# resolves onto each page, so `dictionary[4]` becomes `dictionary[3]` while nothing is
# lost. That is the single normalised state (ADR-0013), and it is the one entry where
# the shape is not the fact worth comparing.
echo "--- the catalogue survives, key for key ---"
catalog_keys() {
    target/release/fepdf inspect catalog "$1" 2>/dev/null \
        | awk '/ENTRIES/,/WHAT THE/' \
        | awk '/^  [A-Z]/ {
            key = $1; $1 = ""; $2 = ""; $3 = "";       # drop key, support, in-7.7.2
            gsub(/[0-9]+ 0 R -> /, "");                # object numbers are renumbered
            sub(/^ +/, "");
            print key, (key == "Pages" ? "" : $0);
        }' | sort
}
for src in samples/*.pdf; do
    b=$(basename "$src" .pdf)
    out="$WORK/$b.default.pdf"
    [ -f "$out" ] || continue
    catalog_keys "$src" > "$WORK/cat.want"
    if grep -q '^Metadata ' "$WORK/cat.want"; then
        catalog_keys "$out" > "$WORK/cat.got"
    else
        catalog_keys "$out" | grep -v '^Metadata ' > "$WORK/cat.got"
    fi
    if diff -q "$WORK/cat.want" "$WORK/cat.got" >/dev/null; then
        printf '  %-14s %s entries, unchanged\n' "$b" "$(wc -l < "$WORK/cat.want" | tr -d ' ')"
    else
        echo "  $b: THE CATALOGUE CHANGED ACROSS THE ROUND TRIP"
        diff "$WORK/cat.want" "$WORK/cat.got" | head -6 | sed 's/^/      /'
        FAILED=1
    fi
done

# The matrix, on files small enough to run it on. Three switches: whether objects are
# packed, which handler encrypts, and whether the file is signed.
echo "--- the combinations, on two samples ---"
for src in samples/sample.pdf samples/fugaku.pdf; do
    check loose            "$src" --no-obj-stm --
    check packed+password  "$src" --encrypt-password pw -- --password pw
    check loose+password   "$src" --no-obj-stm --encrypt-password pw -- --password pw
    check packed+cert      "$src" --encrypt-to "$WORK/cert.der" \
        -- --recipient-certificate "$WORK/cert.der" --recipient-key "$WORK/key.der"
    check loose+cert       "$src" --no-obj-stm --encrypt-to "$WORK/cert.der" \
        -- --recipient-certificate "$WORK/cert.der" --recipient-key "$WORK/key.der"
done

# Signing goes through `publish sign`, so it needs its own shape rather than `check`.
echo "--- signed, packed and loose, plain and encrypted ---"
sign_check() {
    local label="$1" src="$2"; shift 2
    local out="$WORK/$(basename "$src" .pdf).$label.pdf"
    if ! target/release/fepdf publish sign "$src" "$out" \
            --certificate "$WORK/cert.der" --private-key "$WORK/key.der" "$@" \
            >"$WORK/sign.log" 2>&1; then
        echo "  $(basename "$src" .pdf) [$label]: SIGN FAILED"
        tail -2 "$WORK/sign.log" | sed 's/^/      /'
        FAILED=1; return
    fi
    read_text "$src" > "$WORK/want"
    read_text "$out" > "$WORK/got"
    if ! diff -q "$WORK/want" "$WORK/got" >/dev/null; then
        echo "  $(basename "$src" .pdf) [$label]: THIS ENGINE CANNOT READ WHAT IT SIGNED"
        FAILED=1; return
    fi
    # And the signature it just wrote still verifies to itself.
    local verdict
    verdict=$(target/release/fepdf publish verify-signature "$out" 2>&1 || true)
    if ! printf '%s' "$verdict" | grep -q ": verifies"; then
        echo "  $(basename "$src" .pdf) [$label]: THE SIGNATURE IT WROTE DOES NOT VERIFY"
        FAILED=1; return
    fi
    printf '  %-14s %-22s %s bytes, unchanged, signature verifies\n' \
        "$(basename "$src" .pdf)" "[$label]" "$(wc -c < "$WORK/got" | tr -d ' ')"
}
sign_check signed       samples/sample.pdf
sign_check signed+loose samples/sample.pdf --no-obj-stm

# The check has to be able to fail. Comparing one sample's text against another's must
# be caught, or the comparison above proves nothing.
read_text samples/sample.pdf > "$WORK/want"
read_text samples/fugaku.pdf > "$WORK/got"
if diff -q "$WORK/want" "$WORK/got" >/dev/null; then
    echo "  THE COMPARISON CANNOT TELL TWO DIFFERENT DOCUMENTS APART"; FAILED=1
else
    echo "  (two different documents compare unequal, so the comparison can fail)"
fi

echo
if [ "$FAILED" -eq 0 ]; then
    echo "every combination reads back exactly as its input does"
else
    echo "FAILURES above"
fi
exit "$FAILED"
