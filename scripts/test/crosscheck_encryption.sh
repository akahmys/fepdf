#!/usr/bin/env bash
# Encrypts every sample and has PDFKit open it, which fepdf had no part in.
#
# The engine decrypting what the engine encrypted proves only that the two halves agree
# with each other; both could share one wrong reading of Algorithm 8 and round-trip
# perfectly. PDFKit already reads the corpus's AES-256 files at revisions 5 and 6, so it
# is a reader with an independent opinion about what a /U means.
#
# Three questions per file, and the middle one is the one that is easy to lose:
#   1. Is the output actually encrypted?  (PDFKit reports it locked before unlocking)
#   2. Does the password open it?          (unlock succeeds)
#   3. Did encrypting cost anything?       (per-page text matches the plain save)
#
# macOS only: skips cleanly elsewhere.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

if [ "$(uname)" != "Darwin" ] || ! command -v swiftc >/dev/null 2>&1; then
    echo "skipped: needs macOS and swiftc for the independent reader"
    exit 0
fi
[ -x target/release/fepdf ] || { echo "build first: cargo build --release"; exit 1; }

WORK="${TMPDIR:-/tmp}/fepdf-crosscheck-encryption"
rm -rf "$WORK"; mkdir -p "$WORK"
PASSWORD="open sesame"

cat > "$WORK/reader.swift" <<'SWIFT'
import Foundation
import PDFKit
// Prints "locked <bool>", "unlocked <bool>", then one line per page: the character
// count. The caller compares those pairwise against the plain save, because a document
// total dilutes the failure that matters (ADR-0010 emptied five pages and moved the
// total by 0.02%).
let args = CommandLine.arguments
guard let doc = PDFDocument(url: URL(fileURLWithPath: args[1])) else {
    print("unreadable"); exit(1)
}
print("locked \(doc.isLocked)")
if doc.isLocked {
    print("unlocked \(doc.unlock(withPassword: args.count > 2 ? args[2] : ""))")
} else {
    print("unlocked n/a")
}
for i in 0..<doc.pageCount { print(doc.page(at: i)?.string?.count ?? 0) }
SWIFT
swiftc -O "$WORK/reader.swift" -o "$WORK/reader" 2>/dev/null || {
    echo "skipped: could not build the PDFKit reader"; exit 0
}

FAILED=0
for src in samples/*.pdf; do
    name=$(basename "$src" .pdf)

    if ! target/release/fepdf publish upgrade "$src" "$WORK/$name.plain.pdf" \
            >/dev/null 2>&1; then
        echo "  $name: the plain save failed, so there is nothing to compare against"
        FAILED=1; continue
    fi
    if ! target/release/fepdf publish upgrade "$src" "$WORK/$name.enc.pdf" \
            --encrypt-password "$PASSWORD" >"$WORK/$name.log" 2>&1; then
        echo "  $name: ENCRYPT FAILED"; sed 's/^/    /' "$WORK/$name.log"; FAILED=1; continue
    fi

    "$WORK/reader" "$WORK/$name.enc.pdf" "$PASSWORD" > "$WORK/$name.enc.txt" 2>/dev/null
    locked=$(sed -n '1p' "$WORK/$name.enc.txt")
    unlocked=$(sed -n '2p' "$WORK/$name.enc.txt")

    if [ "$locked" != "locked true" ]; then
        echo "  $name: PDFKIT DOES NOT SEE IT AS ENCRYPTED ($locked)"; FAILED=1; continue
    fi
    if [ "$unlocked" != "unlocked true" ]; then
        echo "  $name: PDFKIT COULD NOT OPEN IT WITH THE PASSWORD ($unlocked)"; FAILED=1; continue
    fi

    "$WORK/reader" "$WORK/$name.plain.pdf" > "$WORK/$name.plain.txt" 2>/dev/null
    if ! diff -q <(tail -n +3 "$WORK/$name.plain.txt") \
                 <(tail -n +3 "$WORK/$name.enc.txt") >/dev/null; then
        echo "  $name: ENCRYPTING CHANGED THE TEXT PDFKIT READS"
        diff <(tail -n +3 "$WORK/$name.plain.txt") <(tail -n +3 "$WORK/$name.enc.txt") \
            | head -5 | sed 's/^/    /'
        FAILED=1; continue
    fi

    chars=$(tail -n +3 "$WORK/$name.enc.txt" | paste -sd+ - | bc)
    echo "  $name: PDFKit opens it with the password, $chars chars, unchanged"
done

# The check has to be able to fail: a document a wrong password opens is not encrypted.
if [ "$FAILED" -eq 0 ]; then
    wrong=$("$WORK/reader" "$WORK/sample.enc.pdf" "not the password" 2>/dev/null | sed -n '2p')
    if [ "$wrong" = "unlocked false" ]; then
        echo "  (a wrong password does not open it, so the check can fail)"
    else
        echo "  A WRONG PASSWORD OPENED THE DOCUMENT ($wrong)"; FAILED=1
    fi
fi

echo
if [ "$FAILED" -eq 0 ]; then
    echo "every sample encrypts; PDFKit opens each with the password and reads it unchanged"
else
    echo "FAILURES above"
fi
exit "$FAILED"
