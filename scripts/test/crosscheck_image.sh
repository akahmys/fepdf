#!/usr/bin/env bash
# Asks PDFKit what it sees in an image this engine decoded, and compares.
#
# The five cross-checks beside this one compare *text* and *structure*. None of them
# looks at a picture, so the codecs Phase M builds — `/CCITTFaxDecode`, `/JBIG2Decode`
# and eventually `/JPXDecode` — had nothing independent to be checked against. Neither
# corpus holds a scan: `JBIG2Decode` occurs in none of the 251 files and `CCITTFaxDecode`
# in two, so "it decoded" meant "it did not return an error".
#
# `scripts/dev/../../crates/fepdf-model/examples/make_scan_fixtures.rs` writes the files,
# with the images encoded by implementations that are **not** the decoders under test.
# This asks a second *renderer* about the same files.
#
# `make_layer_fixtures.rs` writes three more, for optional content (8.11): a hidden layer,
# a `/BaseState /OFF`, and the control with the layer on. **Three and not thirteen** —
# PDFKit honours only those two constructions and paints the other eleven the engine now
# hides, so the rest are held by `crates/fepdf/tests/optional_content_test.rs` against the
# clause instead. See ADR-0021.
#
# **Four numbers, not a pixel diff.** Each fixture is black in one quadrant, and the
# comparator is the mean luminance of each quadrant. Two renderers legitimately disagree
# about an edge; they do not disagree about which quarter of the page is black. And the
# four say *which way* a defect went — inverted, flipped, transposed or smeared each
# produce a different four, where a single "different by 3%" says only that something is.
#
# macOS only: the independent renderer is PDFKit. Skips cleanly elsewhere.
#
#   ./scripts/test/crosscheck_image.sh
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

if [ "$(uname)" != "Darwin" ] || ! command -v swiftc >/dev/null 2>&1; then
    echo "skipped: needs macOS and swiftc for the independent renderer"
    exit 0
fi

work="${TMPDIR:-/tmp}/fepdf-crosscheck-image"
mkdir -p "$work"
renderer="$work/pdfquadrants"

if [ ! -x "$renderer" ] || [ "$0" -nt "$renderer" ]; then
    cat > "$work/pdfquadrants.swift" <<'SWIFT'
import Foundation
import PDFKit
import AppKit

// Renders page 1 at one pixel per point onto white, and prints the mean luminance of
// each quadrant: top-left, top-right, bottom-left, bottom-right. White paper is the
// starting point on both sides, so an area neither renderer paints agrees at 255.
let path = CommandLine.arguments[1]
guard let doc = PDFDocument(url: URL(fileURLWithPath: path)), let page = doc.page(at: 0) else {
    exit(1)
}
let box = page.bounds(for: .mediaBox)
let width = Int(box.width), height = Int(box.height)
guard width > 0, height > 0,
      let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                          bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
else { exit(1) }

ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
ctx.fill(CGRect(x: 0, y: 0, width: width, height: height))
ctx.translateBy(x: -box.origin.x, y: -box.origin.y)
page.draw(with: .mediaBox, to: ctx)

guard let data = ctx.data else { exit(1) }
let pixels = data.bindMemory(to: UInt8.self, capacity: width * height * 4)
var totals = [Double](repeating: 0, count: 4)
var counts = [Double](repeating: 0, count: 4)
for y in 0..<height {
    for x in 0..<width {
        // A bitmap context's *memory* starts at the top row, whatever its coordinate
        // system does — so `y` here is a raster row and needs no flip. Reading it as a
        // CoreGraphics coordinate was this script's first bug, and the fixture caught
        // it: both renderers agreed the black quarter was on the left and disagreed
        // about top or bottom, which is what an asymmetric fixture is for.
        let at = (y * width + x) * 4
        let luma = 0.299 * Double(pixels[at]) + 0.587 * Double(pixels[at + 1])
                 + 0.114 * Double(pixels[at + 2])
        let q = (x >= width / 2 ? 1 : 0) + (y >= height / 2 ? 2 : 0)
        totals[q] += luma
        counts[q] += 1
    }
}
print((0..<4).map { String(Int(totals[$0] / max(counts[$0], 1))) }.joined(separator: " "))
SWIFT
    swiftc -O "$work/pdfquadrants.swift" -o "$renderer" 2>/dev/null || {
        echo "skipped: could not build the PDFKit renderer"
        exit 0
    }
fi

if [ ! -d target/scans ]; then
    echo "fixtures absent — cargo run --example make_scan_fixtures -p fepdf-model"
    exit 1
fi
if [ ! -d target/layers ]; then
    echo "fixtures absent — cargo run --example make_layer_fixtures -p fepdf-model"
    exit 1
fi
if [ ! -d target/colour ]; then
    echo "fixtures absent — cargo run --example make_colour_fixtures -p fepdf-model"
    exit 1
fi

cargo build --release -q -p fepdf --features render --example page_quadrants || exit 1
ours=target/release/examples/page_quadrants

# How far apart two renderers may be per quadrant, out of 255. Antialiasing along one
# edge of a quarter-page square moves a mean by well under this; a wrong polarity moves
# it by 255, a flip by the difference between the quadrants, and a wrong stride by tens.
tolerance=12

status=0
compared=0
skipped=0
printf '%-34s %-24s %-24s %s\n' file fepdf PDFKit verdict
# The made fixtures, the files of the external corpus that carry a codec, and the three
# that carry **optional content** or an **annotation appearance** — those last are the
# independent oracles the layer and annotation work did not have, and they earned their
# place. `PDF 2.0 UTF-8 string and annotation.pdf` is a page with no `/Contents` at all
# whose only mark is an annotation's appearance: this engine drew it blank while every
# other reader painted it, which is 6.3.2.2 asking for something that was not there.
# The optional content one: `pdf20-utf8-test.pdf` is where this engine
# was found drawing two layers a file had turned off, because their `/OC` is an OCMD
# written in place with a single `/OCGs` reference and neither form was read.
# `target/colour/` is **expected to disagree**, and is here for that reason: it is where
# ROADMAP.md's Phase P numbers come from, and a phase that quotes four numbers has to leave
# the command that re-derives them. They go green when 7.10 gets an evaluator.
for input in target/scans/*.pdf target/layers/*.pdf target/colour/*.pdf \
             target/external/pdf20examples/pdf20-utf8-test.pdf \
             "target/external/pdf20examples/PDF 2.0 UTF-8 string and annotation.pdf" \
             target/external/pdfua2/8.7-t02-*.pdf \
             target/external/pdf-differences/UnknownFilter-*.pdf; do
    [ -e "$input" ] || continue
    name=$(basename "$input" .pdf)

    mine=$("$ours" "$input" 2>/dev/null)
    theirs=$("$renderer" "$input" 2>/dev/null)
    if [ -z "$theirs" ]; then
        # Counted, not swallowed. `UnknownFilter-Linearized.pdf` is one of these: this
        # engine opens it and PDFKit does not (Phase G), so there is no second opinion to
        # be had — and a check that quietly passed on a file nobody read would be the
        # vacuous pass `crosscheck_roundtrip.sh` already had to remove once.
        printf '%-34s %-24s %-24s %s\n' "$name" "${mine:-—}" "—" "not comparable — PDFKit will not open it"
        skipped=$((skipped + 1))
        continue
    fi
    if [ -z "$mine" ]; then
        printf '%-34s %-24s %-24s %s\n' "$name" "—" "$theirs" "THIS ENGINE RENDERED NOTHING"
        status=1
        continue
    fi
    compared=$((compared + 1))

    verdict=$(awk -v a="$mine" -v b="$theirs" -v t="$tolerance" 'BEGIN {
        n = split(a, x, " "); split(b, y, " ");
        worst = 0
        for (i = 1; i <= n; i++) { d = x[i] - y[i]; if (d < 0) d = -d; if (d > worst) worst = d }
        printf (worst <= t) ? "agree (worst %d)" : "DISAGREE by %d", worst
    }')
    printf '%-34s %-24s %-24s %s\n' "$name" "$mine" "$theirs" "$verdict"
    case "$verdict" in DISAGREE*) status=1 ;; esac
done

echo
if [ "$status" -eq 0 ]; then
    echo "both renderers see the same page — $compared compared, $skipped without a second opinion"
else
    echo "A RENDERER SEES SOMETHING ELSE"
fi
exit "$status"
