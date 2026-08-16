#!/usr/bin/env bash
# Compares the default save against --no-obj-stm, and has other readers read both.
#
# Object streams change how a reader *finds* objects, not what they say, so the test is
# that an independent reader gets the same text out. It is the sharpest form of that
# check available here: by default almost every object in the file sits inside a
# compressed container reached through a cross-reference stream type 2 entry, so a
# reader that gets the indirection wrong finds nothing at all rather than finding
# something slightly wrong.
#
# The size column is the point of the feature. `samples/intel_sdm.pdf` keeps 323,066 of
# its objects in 8,044 containers and comes out +131% with --no-obj-stm.
#
# A second reader runs when one is supplied. `pypdfium2` is PDFium — Chrome's engine,
# sharing no code with PDFKit — and two readers agreeing is what turned packing from an
# option into the default (ADR-0016), so the measurement that decided it should be
# repeatable rather than a thing someone once did:
#
#   python3 -m venv /tmp/fepdf-pdfium && /tmp/fepdf-pdfium/bin/pip install pypdfium2
#   PDFIUM=/tmp/fepdf-pdfium/bin/python ./scripts/test/crosscheck_objstm.sh
#
# macOS only for the PDFKit half: skips cleanly elsewhere.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

if [ "$(uname)" != "Darwin" ] || ! command -v swiftc >/dev/null 2>&1; then
    echo "skipped: needs macOS and swiftc for the independent reader"
    exit 0
fi
[ -x target/release/fepdf ] || { echo "build first: cargo build --release"; exit 1; }

WORK="${TMPDIR:-/tmp}/fepdf-crosscheck-objstm"
rm -rf "$WORK"; mkdir -p "$WORK"

cat > "$WORK/reader.swift" <<'SWIFT'
import Foundation
import PDFKit
guard let doc = PDFDocument(url: URL(fileURLWithPath: CommandLine.arguments[1])) else {
    print("unreadable"); exit(1)
}
for i in 0..<doc.pageCount { print(doc.page(at: i)?.string?.count ?? 0) }
SWIFT
swiftc -O "$WORK/reader.swift" -o "$WORK/reader" 2>/dev/null || {
    echo "skipped: could not build the PDFKit reader"; exit 0
}

cat > "$WORK/pages.py" <<'PYTHON'
import sys, pypdfium2
doc = pypdfium2.PdfDocument(sys.argv[1], password=sys.argv[2] if len(sys.argv) > 2 else None)
for i in range(len(doc)):
    print(len(doc[i].get_textpage().get_text_range()))
PYTHON
[ -n "${PDFIUM:-}" ] || echo "  (PDFIUM is unset, so only PDFKit reads these — see the header)"

FAILED=0
printf "%-14s %12s %12s %12s   %s\n" file source "--no-obj-stm" default "readers agree"
for src in samples/*.pdf; do
    name=$(basename "$src" .pdf)

    target/release/fepdf publish upgrade "$src" "$WORK/$name.plain.pdf" --no-obj-stm >/dev/null 2>&1
    if ! target/release/fepdf publish upgrade "$src" "$WORK/$name.os.pdf" \
            >"$WORK/$name.log" 2>&1; then
        echo "  $name: PACKING FAILED"; sed 's/^/    /' "$WORK/$name.log"; FAILED=1; continue
    fi

    if ! "$WORK/reader" "$WORK/$name.os.pdf" > "$WORK/$name.os.txt" 2>/dev/null; then
        echo "  $name: PDFKIT CANNOT OPEN THE PACKED FILE"; FAILED=1; continue
    fi
    "$WORK/reader" "$WORK/$name.plain.pdf" > "$WORK/$name.plain.txt" 2>/dev/null

    if ! diff -q "$WORK/$name.plain.txt" "$WORK/$name.os.txt" >/dev/null; then
        echo "  $name: PACKING CHANGED THE TEXT PDFKIT READS"
        diff "$WORK/$name.plain.txt" "$WORK/$name.os.txt" | head -5 | sed 's/^/    /'
        FAILED=1; continue
    fi

    # PDFium, when the caller supplied one. Page by page, not on a total: a total
    # dilutes exactly the failure this is looking for.
    if [ -n "${PDFIUM:-}" ]; then
        "$PDFIUM" "$WORK/pages.py" "$WORK/$name.plain.pdf" > "$WORK/$name.plain.fium" 2>/dev/null
        if ! "$PDFIUM" "$WORK/pages.py" "$WORK/$name.os.pdf" > "$WORK/$name.os.fium" 2>/dev/null; then
            echo "  $name: PDFIUM CANNOT OPEN THE PACKED FILE"; FAILED=1; continue
        fi
        if ! diff -q "$WORK/$name.plain.fium" "$WORK/$name.os.fium" >/dev/null; then
            echo "  $name: PDFIUM READS THE PACKED FILE DIFFERENTLY"
            diff "$WORK/$name.plain.fium" "$WORK/$name.os.fium" | head -5 | sed 's/^/    /'
            FAILED=1; continue
        fi
    fi

    s=$(stat -f%z "$src"); p=$(stat -f%z "$WORK/$name.plain.pdf"); o=$(stat -f%z "$WORK/$name.os.pdf")
    printf "%-14s %12s %11s%% %11s%%   %s\n" \
        "$name" "$s" "$(( (p - s) * 100 / s ))" "$(( (o - s) * 100 / s ))" \
        "PDFKit${PDFIUM:+ and PDFium}"
done

# Packing composes with the other two write-time features, and each combination has its
# own way to go wrong: the signature dictionary must stay at a byte offset, and the
# /Encrypt dictionary must get a real cross-reference entry rather than being marked
# free. Both were broken at some point in writing this.
if [ "$FAILED" -eq 0 ]; then
    target/release/fepdf publish upgrade samples/sample.pdf "$WORK/enc.pdf" \
        --encrypt-password pw >/dev/null 2>&1
    cat > "$WORK/unlock.swift" <<'SWIFT'
import Foundation
import PDFKit
guard let doc = PDFDocument(url: URL(fileURLWithPath: CommandLine.arguments[1])) else {
    print("unreadable"); exit(1)
}
if doc.isLocked, !doc.unlock(withPassword: CommandLine.arguments[2]) { print("locked out"); exit(1) }
var total = 0
for i in 0..<doc.pageCount { total += doc.page(at: i)?.string?.count ?? 0 }
print(total)
SWIFT
    swiftc -O "$WORK/unlock.swift" -o "$WORK/unlock" 2>/dev/null
    got=$("$WORK/unlock" "$WORK/enc.pdf" pw 2>/dev/null)
    want=$(paste -sd+ - < "$WORK/sample.plain.txt" | bc)
    if [ "$got" = "$want" ]; then
        echo "  (packed and encrypted together: PDFKit reads $got, unchanged)"
    else
        echo "  PACKED AND ENCRYPTED TOGETHER READS $got, EXPECTED $want"; FAILED=1
    fi
fi

echo
if [ "$FAILED" -eq 0 ]; then
    echo "every sample packs; PDFKit reads each unchanged, encrypted or not"
else
    echo "FAILURES above"
fi
exit "$FAILED"
