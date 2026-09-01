#!/usr/bin/env bash
# Do this engine and PDFKit put the same characters in the same order?
#
# **This exists because a `Done when` was written without a check behind it.** ROADMAP
# Phase T sized the reading-order defect at 7,093 of 7,727 pages and said the work was
# done when "the order-only column falls, no file's identical column falls, and a check
# fails when it does". The first clause was met and the third was never built, so the
# second went unnoticed: `volvo_xc90.pdf` went from 61 agreeing pages to 0 and
# `bokutokitan.pdf` from 93 to 4, while the corpus total rose. A net figure hides a file.
#
# Whitespace is stripped before comparing. The two disagree about spacing by design — this
# engine inserts a space at a quarter-em gap (§9) — and that is a different question from
# order. A page is then one of three things:
#
#   identical   the same characters in the same sequence
#   order-only  the same characters in a different sequence  <- what this measures
#   content     different characters, which is extraction loss and §9's subject
#
# macOS only: the second reader is PDFKit. Skips cleanly elsewhere.
#
#   ./scripts/test/crosscheck_reading_order.sh
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

if [ "$(uname)" != "Darwin" ] || ! command -v swiftc >/dev/null 2>&1; then
    echo "skipped: needs macOS and swiftc for the independent reader"
    exit 0
fi

work="${TMPDIR:-/tmp}/fepdf-reading-order"
mkdir -p "$work"
reader="$work/alltext"

if [ ! -x "$reader" ] || [ "$0" -nt "$reader" ]; then
    cat > "$work/alltext.swift" <<'SWIFT'
import Foundation
import PDFKit
// One record per page, separated by a marker no document can contain. Page at a time
// rather than the whole document, because the comparison is per page: a document total
// would let one page's gain cover another's loss, which is the failure this file exists
// to stop happening at the file level.
let path = CommandLine.arguments[1]
guard let doc = PDFDocument(url: URL(fileURLWithPath: path)) else { exit(1) }
for i in 0..<doc.pageCount {
    print("\u{1}PAGE \(i + 1)\u{1}")
    print(doc.page(at: i)?.string ?? "")
}
SWIFT
    swiftc -O "$work/alltext.swift" -o "$reader" 2>/dev/null || {
        echo "skipped: could not build the PDFKit reader"
        exit 0
    }
fi

cargo build --release -q -p fepdf-cli || exit 1

# The floor each file must not fall below, the best it has ever read, and a floor on how
# far the two readers agree before they first part.
#
# `floor` is what fails this script. `best` is the high-water mark, recorded so that a
# file sitting below its own best is visible on every run without turning the suite red —
# four of them are, and ROADMAP Phase U owns them. A floor raised to meet a best is how
# this table records progress; a floor lowered needs a reason in the commit that lowers it.
#
# Derived 2026-09-01. Two files pass their pre-sort best because `TextExtractionBackend`
# now composes the CTM, which it never did: `volvo_xc90` reads 182 where it read 61 before
# the sort and 0 after it, and `unicode_16` 707 where it read 28 and then 7. The two still
# marked `best` above their floor are the vertical-Japanese ruby case.
#   file:floor:best:prefix-floor
FLOORS="
print_sample:19:19:78
constitution:12:12:96
sample:12:12:96
bokutokitan:4:93:8
fy05:11:45:3
unicode_16:707:707:74
volvo_xc90:182:182:56
fugaku:1:1:0
intel_sdm:1909:1909:46
"

status=0
printf '%-16s %7s %10s %11s %8s %8s %7s  %s\n' \
    file pages identical order-only content prefix% floor note
total_pages=0 total_ident=0 total_order=0 total_content=0

for entry in $FLOORS; do
    name=${entry%%:*}
    rest=${entry#*:}
    floor=${rest%%:*}
    rest=${rest#*:}
    best=${rest%%:*}
    prefix_floor=${rest#*:}
    pdf="samples/$name.pdf"
    [ -e "$pdf" ] || { printf '%-16s  absent, skipped\n' "$name"; continue; }

    ./target/release/fepdf inspect text "$pdf" 2>/dev/null > "$work/$name.fepdf"
    "$reader" "$pdf" > "$work/$name.pdfkit"

    read -r pages ident order content prefix < <(
        python3 scripts/test/compare_reading_order.py "$work/$name.fepdf" "$work/$name.pdfkit")

    note=""
    # A file both readers see as empty passes every comparison without testing anything,
    # which is how a sibling script sat in its list contributing nothing for weeks.
    if [ "${pages:-0}" -eq 0 ]; then
        note="<- NO PAGES COMPARED, nothing tested"
        status=1
    elif [ "$ident" -lt "$floor" ]; then
        note="<- FELL BELOW ITS FLOOR OF $floor"
        status=1
    elif [ "${prefix%%.*}" -lt "$prefix_floor" ]; then
        note="<- PREFIX AGREEMENT FELL BELOW ${prefix_floor}%"
        status=1
    elif [ "$ident" -lt "$best" ]; then
        note="below $best, its best before ADR-0047's sort (Phase U)"
    elif [ "$ident" -gt "$floor" ]; then
        note="above its floor of $floor — raise it"
    fi

    printf '%-16s %7s %10s %11s %8s %8s %7s  %s\n' \
        "$name" "$pages" "$ident" "$order" "$content" "$prefix" "$floor" "$note"
    total_pages=$((total_pages + pages))
    total_ident=$((total_ident + ident))
    total_order=$((total_order + order))
    total_content=$((total_content + content))
done

printf '%-16s %7s %10s %11s %8s\n' \
    TOTAL "$total_pages" "$total_ident" "$total_order" "$total_content"
echo
echo "  identical  the same characters in the same sequence"
echo "  order-only the same characters in a different sequence — this script's subject"
echo "  content    different characters, which is extraction loss (ROADMAP §9)"
echo "  prefix%    how far the two agree before they first part, over all pages of the"
echo "             file — a page is a coarse unit, and one misplaced running head keeps a"
echo "             page off the identical column however much of it is right"

exit $status
