#!/usr/bin/env bash
# Round-trips every corpus file and compares the text an *independent* reader gets out.
#
# Three defects have now been found only this way (ADR-0006, ADR-0009, ADR-0010), and
# none of them could have been found internally: the engine's own before-and-after
# comparison was identical in each case, because whatever it got wrong reading it also
# got wrong writing. ADR-0010 is the sharpest — the engine extracts no text at all from
# the pages it was destroying, so it could not see the destruction from either side.
#
# macOS only: the independent reader is PDFKit. Skips cleanly elsewhere.
#
#   ./scripts/test/crosscheck_roundtrip.sh
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

if [ "$(uname)" != "Darwin" ] || ! command -v swiftc >/dev/null 2>&1; then
    echo "skipped: needs macOS and swiftc for the independent reader"
    exit 0
fi

work="${TMPDIR:-/tmp}/fepdf-crosscheck"
mkdir -p "$work"
reader="$work/pdftext"

if [ ! -x "$reader" ] || [ "$0" -nt "$reader" ]; then
    cat > "$work/pdftext.swift" <<'SWIFT'
import Foundation
import PDFKit
// Prints one line per page: the character count. The caller compares them pairwise,
// because a document total dilutes the failure that matters — ADR-0010's defect
// emptied five pages and moved the total by 0.02%.
let path = CommandLine.arguments[1]
guard let doc = PDFDocument(url: URL(fileURLWithPath: path)) else { exit(1) }
if doc.isLocked, CommandLine.arguments.count > 2 {
    _ = doc.unlock(withPassword: CommandLine.arguments[2])
}
for i in 0..<doc.pageCount { print(doc.page(at: i)?.string?.count ?? 0) }
SWIFT
    swiftc -O "$work/pdftext.swift" -o "$reader" 2>/dev/null || {
        echo "skipped: could not build the PDFKit reader"
        exit 0
    }
fi

cargo build --release -q -p fepdf-cli || exit 1
cli=target/release/fepdf

status=0
printf '%-18s %10s %10s %8s\n' file source output delta
for input in samples/*.pdf target/encrypted/*.pdf; do
    [ -e "$input" ] || continue
    name=$(basename "$input" .pdf)
    # The fixtures whose user password is not empty. A file whose password is missing
    # here reads as zero characters both sides and passes vacuously, which is how
    # aes256_saslprep sat in this list testing nothing.
    password=""
    [ "$name" = "aes256_owner" ] && password="userpw"
    # The fi ligature, which SASLprep folds to "fi" — written as raw UTF-8 because
    # bash 3.2's printf has no \u escape.
    [ "$name" = "aes256_saslprep" ] && password='ﬁre'

    output="$work/$name.pdf"
    if ! "$cli" publish upgrade "$input" "$output" --password "$password" >/dev/null 2>&1; then
        printf '%-18s %10s\n' "$name" "FAILED TO WRITE"
        status=1
        continue
    fi

    # CoreGraphics logs to stderr about files it dislikes but still reads.
    "$reader" "$input" "$password" 2>/dev/null > "$work/$name.before"
    "$reader" "$output" 2>/dev/null > "$work/$name.after"

    # A page that had text and now has none is the failure. Totals hide it: with the
    # defect ADR-0010 records, fy05 emptied five pages and the document total moved by
    # 0.02% — a threshold loose enough to tolerate word-spacing noise cannot see that,
    # which the first version of this script demonstrated by not firing.
    read -r emptied lost < <(paste "$work/$name.before" "$work/$name.after" |
        awk '{ if ($1 > 0 && $2 == 0) e++; if ($2 < $1) l += $1 - $2 } END { print e+0, l+0 }')
    before=$(awk '{ s += $1 } END { print s+0 }' "$work/$name.before")
    after=$(awk '{ s += $1 } END { print s+0 }' "$work/$name.after")

    # A file reading as empty on both sides passes every comparison without testing
    # anything. Almost always a password this script does not know — which is how
    # aes256_saslprep sat in the list contributing nothing.
    if [ "${before:-0}" -eq 0 ]; then
        printf '%-18s %10s %10s %8s  <- READ AS EMPTY, nothing compared\n' "$name" 0 0 0
        status=1
    elif [ "$emptied" -gt 0 ]; then
        printf '%-18s %10s %10s %8s  <- %s PAGES EMPTIED\n' \
            "$name" "$before" "$after" "$((after - before))" "$emptied"
        status=1
    else
        printf '%-18s %10s %10s %8s  (%s chars lost across pages that kept text)\n' \
            "$name" "$before" "$after" "$((after - before))" "$lost"
    fi
done

exit $status
