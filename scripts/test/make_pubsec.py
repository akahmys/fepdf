#!/usr/bin/env python3
"""Builds a certificate-encrypted PDF (ISO 32000-2, 7.6.5), independently of fepdf.

The same idea as `make_encrypted.py`: a fixture the engine is tested against must not
be produced by the engine, or the test only says it agrees with itself. The hard part
here — the CMS `EnvelopedData` that wraps the seed for each recipient — is handed to
`openssl cms -encrypt`, which is a wholly separate implementation. What this script does
itself is the PDF-level half: generate the seed, ask openssl to envelope it, and derive
the file encryption key the way 7.6.5.3 says.

    python3 scripts/test/make_pubsec.py samples/sample.pdf target/pubsec

Writes `pubsec.pdf` with `cert.der` and `key.der` beside it. Reading it back:

    fepdf inspect text target/pubsec/pubsec.pdf \\
        --recipient-certificate target/pubsec/cert.der \\
        --recipient-key target/pubsec/key.der

The key derivation, from the clause: SHA-256 (SHA-1 below AES-256) of the 20-byte seed
followed by every /Recipients entry in order, truncated to the key length. Hashing the
entries is what binds the key to the recipient list, so adding a reader changes the key.
`/KDFSalt` is *not* part of it — that belongs to PDF 2.0's document MAC, and treating it
as key material produces a key nothing else computes.
"""

import hashlib
import os
import re
import secrets
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import aes  # noqa: E402  — scripts/test/aes.py, checked against FIPS-197



def run(args, stdin=None):
    return subprocess.run(args, input=stdin, capture_output=True, check=True).stdout


def make_identity(out):
    """A throwaway RSA identity with the keyEncipherment bit a recipient needs."""
    run([
        "openssl", "req", "-x509", "-newkey", "rsa:2048",
        "-keyout", f"{out}/key.pem", "-out", f"{out}/cert.pem",
        "-days", "1", "-nodes", "-subj", "/CN=fepdf pubsec recipient",
        "-addext", "keyUsage=critical,keyEncipherment,dataEncipherment",
    ])
    run(["openssl", "x509", "-in", f"{out}/cert.pem", "-outform", "der",
         "-out", f"{out}/cert.der"])
    run(["openssl", "pkcs8", "-topk8", "-nocrypt", "-in", f"{out}/key.pem",
         "-outform", "der", "-out", f"{out}/key.der"])


def envelope(out, seed, permissions):
    """The CMS EnvelopedData for one recipient: 20 bytes of seed, 4 of permissions.

    openssl does the enveloping, so nothing about the asymmetric half of this file
    comes from code written for this project.
    """
    content = seed + permissions.to_bytes(4, "big", signed=True)
    return run([
        "openssl", "cms", "-encrypt", "-binary", "-outform", "DER",
        "-aes-256-cbc", f"{out}/cert.pem",
    ], stdin=content)


def aes_cbc_encrypt(key, data):
    """AES-256-CBC with a random IV and PKCS#7 padding.

    `aes.py`, not a subprocess: this is called once per string and once per stream, and
    `samples/intel_sdm.pdf` flattened has 332,000 objects. It is still an independent
    implementation — a pure-Python AES checked against FIPS-197 — and the asymmetric
    half, which is the part worth being paranoid about, is still openssl's.
    """
    iv = secrets.token_bytes(16)
    return iv + aes.cbc_encrypt(key, iv, aes.pkcs7(data))


def main():
    source, out = sys.argv[1], sys.argv[2]
    os.makedirs(out, exist_ok=True)
    make_identity(out)

    seed = secrets.token_bytes(20)
    recipients = [envelope(out, seed, -1)]

    # 7.6.5.3: the seed, then every recipient entry in order. /EncryptMetadata is true
    # here, so the 0xFFFFFFFF suffix that a plaintext-metadata file adds is absent.
    digest = hashlib.sha256()
    digest.update(seed)
    for entry in recipients:
        digest.update(entry)
    file_key = digest.digest()[:32]

    pdf = open(source, "rb").read()
    objects, trailer_root = parse_objects(pdf)

    body = bytearray(b"%PDF-2.0\r\n%\xE2\xE3\xCF\xD3\r\n")
    offsets = {}
    for number in sorted(objects):
        offsets[number] = len(body)
        body += encrypt_object(number, objects[number], file_key)

    encrypt_number = max(objects) + 1
    offsets[encrypt_number] = len(body)
    body += encrypt_dictionary(encrypt_number, recipients)

    body += cross_reference(offsets, encrypt_number, trailer_root, len(body))
    open(f"{out}/pubsec.pdf", "wb").write(bytes(body))
    print(f"{out}/pubsec.pdf: 7.6.5, one recipient, AES-256")


def parse_objects(pdf):
    """Every `N 0 obj ... endobj` in the source, verbatim, plus the /Root number.

    A stream's end comes from its `/Length`, not from searching for `endobj`. Searching
    worked on most files and failed on `samples/fy05.pdf` about half the time, because
    compressed data contains the bytes `endobj` by chance often enough to matter — and
    the file changes between runs, so the failure moved. It looked like an engine bug
    for two rounds of investigation.

    Still narrow in one way: it wants loose objects, which is what `--no-obj-stm` is for.
    """
    objects = {}
    for match in re.finditer(rb"(?m)^(\d+) 0 obj\r?\n?", pdf):
        number = int(match.group(1))
        body_start = match.end()
        stream = re.compile(rb"stream\r?\n").search(pdf, body_start)
        end = pdf.find(b"endobj", body_start)
        if stream and stream.start() < end:
            length = re.search(rb"/Length\s+(\d+)", pdf[body_start:stream.start()])
            if not length:
                raise SystemExit(f"object {number} has no direct /Length; use --no-obj-stm output")
            end = pdf.find(b"endobj", stream.end() + int(length.group(1)))
        objects[number] = pdf[body_start:end].rstrip(b"\r\n")
    root = re.search(rb"/Root\s+(\d+) 0 R", pdf)
    return objects, int(root.group(1))


def encrypt_object(number, body, key):
    """Encrypts the strings and the stream of one object.

    Both, because 7.6.2 encrypts both. Leaving the strings alone produced a fixture
    that decrypted to a document with six corrupted `/Title` entries out of 4,574
    objects — small enough to be mistaken for an engine bug, which is what it looked
    like until the objects were compared one by one.

    For /V 5 the file key is used directly, with no per-object derivation
    (7.6.4.3.4), which is the one simplification that makes this script short enough
    to read.
    """
    match = re.search(rb"stream\r?\n", body)
    dictionary, tail = (body, b"") if not match else (body[:match.start()], body[match.start():])
    dictionary = encrypt_strings(dictionary, key)

    if match:
        end = tail.rfind(b"endstream")
        data = tail[re.match(rb"stream\r?\n", tail).end():end].rstrip(b"\r\n")
        data = aes_cbc_encrypt(key, data)
        dictionary = re.sub(rb"/Length\s+\d+", b"/Length " + str(len(data)).encode(),
                            dictionary)
        tail = b"stream\r\n" + data + b"\r\nendstream"
    return f"{number} 0 obj\r\n".encode() + dictionary + tail + b"\r\nendobj\r\n"


def encrypt_strings(dictionary, key):
    """Encrypts every literal string in a dictionary, writing them back as hex.

    Hex on the way out because ciphertext is arbitrary bytes and a literal string has
    to escape several of them; the engine reading this has to accept both spellings
    anyway. Only `(...)` is rewritten — a `<...>` in these files is a hex string the
    same rule applies to, but the fixtures are built from this engine's own output and
    it writes text as literals.
    """
    out = bytearray()
    i = 0
    while i < len(dictionary):
        if dictionary[i:i + 1] != b"(":
            out += dictionary[i:i + 1]
            i += 1
            continue
        # Find the matching close paren, honouring escapes and nesting as 7.3.4.2 does.
        depth, j, raw = 1, i + 1, bytearray()
        while j < len(dictionary) and depth:
            c = dictionary[j:j + 1]
            if c == b"\\":
                raw += dictionary[j:j + 2]
                j += 2
                continue
            if c == b"(":
                depth += 1
            elif c == b")":
                depth -= 1
                if not depth:
                    break
            raw += c
            j += 1
        out += b"<" + aes_cbc_encrypt(key, unescape(bytes(raw))).hex().upper().encode() + b">"
        i = j + 1
    return bytes(out)


# 7.3.4.2 Table 3. `\r` is a carriage return, not the letter r, which is what a
# one-line `re.sub(rb"\\(.)", rb"\1")` makes of it.
ESCAPES = {b"n": b"\n", b"r": b"\r", b"t": b"\t", b"b": b"\b", b"f": b"\f"}


def unescape(raw):
    """Turns a literal string's escapes back into the bytes they stand for."""
    out = bytearray()
    i = 0
    while i < len(raw):
        if raw[i:i + 1] != b"\\":
            out += raw[i:i + 1]
            i += 1
            continue
        c = raw[i + 1:i + 2]
        if c in ESCAPES:
            out += ESCAPES[c]
            i += 2
        elif c.isdigit():
            # Up to three octal digits, which is how a binary byte is spelled.
            digits = re.match(rb"[0-7]{1,3}", raw[i + 1:]).group()
            out += bytes([int(digits, 8) & 0xFF])
            i += 1 + len(digits)
        else:
            # A backslash before anything else is that thing, parentheses included.
            out += c
            i += 2
    return bytes(out)


def encrypt_dictionary(number, recipients):
    entries = " ".join("<" + entry.hex().upper() + ">" for entry in recipients)
    return (
        f"{number} 0 obj\r\n<<\r\n/Filter /Adobe.PubSec\r\n/SubFilter /adbe.pkcs7.s5\r\n"
        f"/V 5\r\n/Length 256\r\n/EncryptMetadata true\r\n"
        f"/StmF /DefaultCryptFilter\r\n/StrF /DefaultCryptFilter\r\n"
        f"/CF << /DefaultCryptFilter << /CFM /AESV3 /Length 256 "
        f"/Recipients [ {entries} ] >> >>\r\n>>\r\nendobj\r\n"
    ).encode()


def cross_reference(offsets, encrypt_number, root, start):
    size = max(offsets) + 1
    table = bytearray(f"xref\r\n0 {size}\r\n0000000000 65535 f\r\n".encode())
    for number in range(1, size):
        table += f"{offsets.get(number, 0):010} 00000 n\r\n".encode()
    table += (
        f"trailer\r\n<< /Size {size} /Root {root} 0 R /Encrypt {encrypt_number} 0 R "
        f"/ID [<00> <00>] >>\r\nstartxref\r\n{start}\r\n%%EOF\r\n"
    ).encode()
    return bytes(table)


if __name__ == "__main__":
    sys.exit(main())
