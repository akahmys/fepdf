#!/usr/bin/env bash
# Packs every sample into object streams and has PDFKit read the result.
#
# Object streams change how a reader *finds* objects, not what they say, so the test is
# that an independent reader gets the same text out. It is the sharpest form of that
# check available here: with `--obj-stm` almost every object in the file moves inside a
# compressed container reached through a cross-reference stream type 2 entry, so a
# reader that gets the indirection wrong finds nothing at all rather than finding
# something slightly wrong.
#
# The size column is the point of the feature. `samples/intel_sdm.pdf` keeps 323,066 of
# its objects in 8,044 containers and comes out +131% without them.
#
# macOS only: skips cleanly elsewhere.
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

FAILED=0
printf "%-14s %12s %12s %12s   %s\n" file source plain "with objstm" "PDFKit"
for src in samples/*.pdf; do
    name=$(basename "$src" .pdf)

    target/release/fepdf publish upgrade "$src" "$WORK/$name.plain.pdf" >/dev/null 2>&1
    if ! target/release/fepdf publish upgrade "$src" "$WORK/$name.os.pdf" --obj-stm \
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

    s=$(stat -f%z "$src"); p=$(stat -f%z "$WORK/$name.plain.pdf"); o=$(stat -f%z "$WORK/$name.os.pdf")
    printf "%-14s %12s %11s%% %11s%%   same text\n" \
        "$name" "$s" "$(( (p - s) * 100 / s ))" "$(( (o - s) * 100 / s ))"
done

# Packing composes with the other two write-time features, and each combination has its
# own way to go wrong: the signature dictionary must stay at a byte offset, and the
# /Encrypt dictionary must get a real cross-reference entry rather than being marked
# free. Both were broken at some point in writing this.
if [ "$FAILED" -eq 0 ]; then
    target/release/fepdf publish upgrade samples/sample.pdf "$WORK/enc.pdf" \
        --obj-stm --encrypt-password pw >/dev/null 2>&1
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
