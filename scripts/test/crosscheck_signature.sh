#!/usr/bin/env bash
# Verifies a signature fepdf wrote using openssl, which had no part in making it.
#
# The engine's own tests can only say the signature agrees with the digest the engine
# computed. This takes the file at its word instead: it reads /ByteRange out of the
# bytes, concatenates exactly the ranges named there, pulls the DER out of /Contents,
# and asks `openssl cms -verify` whether that signature covers that content. Nothing
# here links against fepdf.
#
# `-no_check_time` and `-partial_chain` are on purpose: the test certificate is
# self-signed and short-lived, so a trust decision would be testing openssl's opinion of
# a throwaway CA rather than testing the signature. What is under test is whether the
# bytes fepdf signed are the bytes fepdf wrote.
set -euo pipefail

cd "$(dirname "$0")/../.."
WORK="${TMPDIR:-/tmp}/fepdf-crosscheck-signature"
rm -rf "$WORK"; mkdir -p "$WORK"

command -v openssl >/dev/null || { echo "openssl not found"; exit 1; }
[ -x target/release/fepdf ] || { echo "build first: cargo build --release"; exit 1; }

echo "--- identity (generated here; nothing is checked in) ---"
openssl req -x509 -newkey rsa:2048 -keyout "$WORK/key.pem" -out "$WORK/cert.pem" \
    -days 1 -nodes -subj "/CN=fepdf crosscheck signer/O=fepdf" 2>/dev/null
openssl x509 -in "$WORK/cert.pem" -outform der -out "$WORK/cert.der"
openssl pkcs8 -topk8 -nocrypt -in "$WORK/key.pem" -outform der -out "$WORK/key.der"

# A second independent reader, for a second question: signing adds objects to the file,
# and the file has to survive that. PDFKit's per-page character counts must match
# between the signed save and the plain one -- same write path, so any difference is the
# signature structure disturbing the document.
READER="$WORK/pdftext"
if [ "$(uname)" = "Darwin" ] && command -v swiftc >/dev/null 2>&1; then
    cat > "$WORK/pdftext.swift" <<'SWIFT'
import Foundation
import PDFKit
let path = CommandLine.arguments[1]
guard let doc = PDFDocument(url: URL(fileURLWithPath: path)) else { exit(1) }
for i in 0..<doc.pageCount { print(doc.page(at: i)?.string?.count ?? 0) }
SWIFT
    swiftc -O "$WORK/pdftext.swift" -o "$READER" 2>/dev/null || READER=""
else
    READER=""
fi
[ -n "$READER" ] || echo "  (no PDFKit reader; the text comparison is skipped)"

FAILED=0
for src in samples/*.pdf; do
    name=$(basename "$src" .pdf)
    out="$WORK/$name.pdf"

    if ! target/release/fepdf publish sign "$src" "$out" \
        --certificate "$WORK/cert.der" --private-key "$WORK/key.der" \
        --reason "crosscheck" >"$WORK/$name.log" 2>&1; then
        echo "  $name: SIGN FAILED"; sed 's/^/    /' "$WORK/$name.log"; FAILED=1; continue
    fi

    if [ -n "$READER" ]; then
        target/release/fepdf publish upgrade "$src" "$WORK/$name.plain.pdf" >/dev/null 2>&1
        if ! "$READER" "$out" > "$WORK/$name.signed.txt" 2>/dev/null; then
            echo "  $name: PDFKIT CANNOT OPEN THE SIGNED FILE"; FAILED=1; continue
        fi
        "$READER" "$WORK/$name.plain.pdf" > "$WORK/$name.plain.txt" 2>/dev/null || true
        if ! diff -q "$WORK/$name.plain.txt" "$WORK/$name.signed.txt" >/dev/null; then
            echo "  $name: SIGNING CHANGED THE TEXT PDFKIT READS"
            diff "$WORK/$name.plain.txt" "$WORK/$name.signed.txt" | head -5 | sed 's/^/    /'
            FAILED=1; continue
        fi
    fi

    # Split the file the way /ByteRange says, and recover the DER, in python3 so that
    # nothing in this path is the code under test.
    if ! python3 scripts/test/extract_signature.py "$out" "$WORK/$name.content" \
            "$WORK/$name.der" >"$WORK/$name.extract" 2>&1; then
        echo "  $name: EXTRACT FAILED"; sed 's/^/    /' "$WORK/$name.extract"; FAILED=1; continue
    fi

    if openssl cms -verify -binary -inform der -in "$WORK/$name.der" \
        -content "$WORK/$name.content" -certfile "$WORK/cert.pem" \
        -CAfile "$WORK/cert.pem" -no_check_time -partial_chain \
        -out /dev/null 2>"$WORK/$name.openssl"; then
        echo "  $name: openssl verifies the signature over $(cat "$WORK/$name.extract")"
    else
        echo "  $name: OPENSSL REJECTED"; sed 's/^/    /' "$WORK/$name.openssl"; FAILED=1
    fi
done

echo
if [ "$FAILED" -eq 0 ]; then
    echo "every sample signs, and openssl verifies each signature over the bytes"
else
    echo "FAILURES above"
fi
exit "$FAILED"
