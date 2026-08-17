#!/usr/bin/env bash
# Reads certificate-encrypted documents (7.6.5) built by something other than fepdf.
#
# There is no corpus file for this clause and no widely-deployed reader to compare
# against — pdf.js rejects any non-Standard /Filter outright, PDFium only handles
# Standard, and qpdf says so in its own documentation. So the check runs the other way
# round: an independent producer makes the file, and fepdf has to get the plaintext back.
#
# `make_pubsec.py` hands the CMS EnvelopedData to `openssl cms -encrypt` and does only
# the PDF-level half itself, so the asymmetric cryptography in the fixture shares no code
# with the engine reading it. The test is that the text out of the encrypted file equals
# the text out of the plaintext one it was made from — the whole file, not a page count.
#
# The comparison is on the text, not on the exit status. `inspect text` exits non-zero
# when any page fails to extract, and `samples/fy05.pdf` has six that do — a defect of
# its own, unrelated to encryption and present on the plaintext file. With `pipefail`
# that non-zero status made this script report fy05 as unreadable, which was read as the
# fixture builder being at fault and written down as such in three places. It was not:
# the fixture is correct, and the two texts match exactly. Comparing content says what
# this check is actually for.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

command -v openssl >/dev/null || { echo "openssl not found"; exit 1; }
[ -x target/release/fepdf ] || { echo "build first: cargo build --release"; exit 1; }

WORK="${TMPDIR:-/tmp}/fepdf-crosscheck-pubsec"
rm -rf "$WORK"; mkdir -p "$WORK"

FAILED=0
for src in samples/*.pdf; do
    name=$(basename "$src" .pdf)

    # Flattened first: the fixture builder walks `N 0 obj` with a regular expression and
    # would miss anything inside an object stream, which is now the default form.
    if ! target/release/fepdf publish upgrade "$src" "$WORK/$name.plain.pdf" --no-obj-stm \
            >/dev/null 2>&1; then
        echo "  $name: could not flatten, so there is nothing to encrypt"; FAILED=1; continue
    fi
    # `make_pubsec.py` does AES in pure Python, once per string and once per stream.
    # That is fine for everything here except `intel_sdm.pdf`, which flattens to 58 MB
    # and 332,000 objects; building its fixture takes longer than the rest of the suite
    # put together and tests nothing the others do not. Skipped by size, and said so.
    if [ "$(stat -f%z "$WORK/$name.plain.pdf")" -gt 20000000 ]; then
        echo "  $name: skipped, too large for a pure-Python fixture builder"
        continue
    fi
    if ! python3 scripts/test/make_pubsec.py "$WORK/$name.plain.pdf" "$WORK/$name" \
            >"$WORK/$name.log" 2>&1; then
        echo "  $name: FIXTURE FAILED"; tail -3 "$WORK/$name.log" | sed 's/^/    /'; FAILED=1; continue
    fi

    # A fixture that will not parse is the builder's bug, and the engine never got as
    # far as the key. Worth telling apart from a decryption failure.
    if ! target/release/fepdf inspect structure "$WORK/$name/pubsec.pdf" >/dev/null 2>&1; then
        echo "  $name: the fixture builder produced a file that will not parse — its bug, not the engine's"
        FAILED=1; continue
    fi

    # Exit status deliberately ignored: it reports pages that would not extract from the
    # plaintext either, which is a different defect and not this check's business.
    target/release/fepdf inspect text "$WORK/$name.plain.pdf" 2>/dev/null | tail -n +2 \
        > "$WORK/$name.want" || true
    target/release/fepdf inspect text "$WORK/$name/pubsec.pdf" \
        --recipient-certificate "$WORK/$name/cert.der" \
        --recipient-key "$WORK/$name/key.der" 2>/dev/null | tail -n +2 \
        > "$WORK/$name.got" || true

    if ! diff -q "$WORK/$name.want" "$WORK/$name.got" >/dev/null; then
        echo "  $name: THE DECRYPTED TEXT DIFFERS FROM THE PLAINTEXT"
        diff "$WORK/$name.want" "$WORK/$name.got" | head -4 | sed 's/^/    /'
        FAILED=1; continue
    fi
    echo "  $name: opens with the certificate, $(wc -c < "$WORK/$name.got" | tr -d ' ') bytes, unchanged"
done

# The other direction: what this engine *writes* to a certificate, it has to read back.
# Weaker than the checks above — an engine agreeing with itself is the weakest statement
# available — but it exercises the sealing side, which the fixtures cannot: they are
# built by openssl and say nothing about whether this engine can produce one.
#
# Deliberately not gated on the loop above. The fixture builder's own bugs must not stop
# the writing side being checked; they are unrelated code.
if [ -f "$WORK/sample/cert.der" ]; then
    target/release/fepdf publish upgrade samples/sample.pdf "$WORK/written.pdf" \
        --encrypt-to "$WORK/sample/cert.der" >/dev/null 2>&1
    target/release/fepdf inspect text samples/sample.pdf 2>/dev/null | tail -n +2 \
        > "$WORK/w.want" || true
    target/release/fepdf inspect text "$WORK/written.pdf" \
        --recipient-certificate "$WORK/sample/cert.der" \
        --recipient-key "$WORK/sample/key.der" 2>/dev/null | tail -n +2 > "$WORK/w.got" || true
    if diff -q "$WORK/w.want" "$WORK/w.got" >/dev/null; then
        echo "  (--encrypt-to: what this engine sealed, it reads back unchanged)"
    else
        echo "  WHAT THIS ENGINE SEALED IT CANNOT READ BACK"; FAILED=1
    fi

    # And the certificate is genuinely required. The output is packed by default, so
    # its structure is inside the encrypted containers and it does not open at all —
    # the message says that rather than reporting the missing catalogue it causes.
    # Captured first, not piped: `pipefail` makes `cmd | grep` report *cmd's* status,
    # and a refusal exits non-zero by design — so the pipeline failed even when grep
    # found what it was looking for. That is the same trap that made this script blame
    # its own fixture builder for fy05.
    said=$(target/release/fepdf inspect text "$WORK/written.pdf" 2>&1 || true)
    if printf '%s' "$said" | grep -q "was not unlocked"; then
        echo "  (--encrypt-to output refuses to open without one, and says why)"
    else
        echo "  THE WRITTEN DOCUMENT OPENED WITHOUT A CERTIFICATE"; FAILED=1
    fi
fi

# The check has to be able to fail. A document that opens without the key it was
# addressed to is not encrypted, and one that opens with the wrong key is worse.
if [ "$FAILED" -eq 0 ]; then
    said=$(target/release/fepdf inspect text "$WORK/sample/pubsec.pdf" 2>&1 || true)
    if printf '%s' "$said" | grep -q "7.6.5 : the document is encrypted to a certificate"; then
        echo "  (with no certificate: refused, and says which kind it wanted)"
    else
        echo "  IT OPENED WITHOUT A CERTIFICATE"; FAILED=1
    fi

    openssl req -x509 -newkey rsa:2048 -keyout "$WORK/other.pem" -out "$WORK/other.crt" \
        -days 1 -nodes -subj "/CN=not a recipient" \
        -addext "keyUsage=critical,keyEncipherment" 2>/dev/null
    openssl x509 -in "$WORK/other.crt" -outform der -out "$WORK/other.cert.der"
    openssl pkcs8 -topk8 -nocrypt -in "$WORK/other.pem" -outform der -out "$WORK/other.key.der"
    said=$(target/release/fepdf inspect text "$WORK/sample/pubsec.pdf" \
        --recipient-certificate "$WORK/other.cert.der" \
        --recipient-key "$WORK/other.key.der" 2>&1 || true)
    if printf '%s' "$said" | grep -q "7.6.1 :.*could not be unlocked"; then
        echo "  (a certificate it was not addressed to: refused)"
    else
        echo "  A STRANGER'S CERTIFICATE OPENED THE DOCUMENT"; FAILED=1
    fi
fi

echo
if [ "$FAILED" -eq 0 ]; then
    echo "every sample encrypts to a certificate and reads back unchanged"
else
    echo "FAILURES above"
fi
exit "$FAILED"
