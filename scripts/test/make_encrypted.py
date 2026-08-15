#!/usr/bin/env python3
"""Builds RC4-encrypted PDFs from a healthy sample, for clause 7.6 regression tests.

Independent on purpose. The corpus has exactly one encrypted file, AES-128, and no
RC4 file at all — so the engine's RC4 path had nothing to read. Generating the fixtures
with fepdf's own cryptography would test it against itself; ISO 32000-2 Algorithms 1, 2,
4 and 5 are implemented here from the standard, with RC4 and MD5 from the Python
standard library, so a disagreement means one of the two is wrong.

RC4 is a stream cipher, so ciphertext is the same length as plaintext. Streams can
therefore be encrypted in place without moving a single byte, and only the trailer needs
a new `/Encrypt` and `/ID` — added as an incremental update (7.5.6), which the reader
already handles.

    python3 scripts/test/make_encrypted.py [--out target/encrypted]
"""

from __future__ import annotations

import argparse
import hashlib
import unicodedata
import os
import re
import sys
from pathlib import Path

import aes

PAD = bytes([
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
])


def rc4(key: bytes, data: bytes) -> bytes:
    s = list(range(256))
    j = 0
    for i in range(256):
        j = (j + s[i] + key[i % len(key)]) % 256
        s[i], s[j] = s[j], s[i]
    out = bytearray()
    i = j = 0
    for byte in data:
        i = (i + 1) % 256
        j = (j + s[i]) % 256
        s[i], s[j] = s[j], s[i]
        out.append(byte ^ s[(s[i] + s[j]) % 256])
    return bytes(out)


def pad_password(password: bytes) -> bytes:
    """Algorithm 2, step (a)."""
    return (password + PAD)[:32]


def owner_string(owner: bytes, user: bytes, revision: int, key_len: int) -> bytes:
    """Algorithm 3: `/O`."""
    digest = hashlib.md5(pad_password(owner or user)).digest()
    if revision >= 3:
        for _ in range(50):
            digest = hashlib.md5(digest).digest()
    key = digest[:key_len]
    out = rc4(key, pad_password(user))
    if revision >= 3:
        for i in range(1, 20):
            out = rc4(bytes(b ^ i for b in key), out)
    return out


def file_key(user: bytes, o: bytes, p: int, file_id: bytes, revision: int, key_len: int) -> bytes:
    """Algorithm 2."""
    h = hashlib.md5()
    h.update(pad_password(user))
    h.update(o)
    h.update(p.to_bytes(4, "little", signed=True))
    h.update(file_id)
    digest = h.digest()
    if revision >= 3:
        for _ in range(50):
            digest = hashlib.md5(digest[:key_len]).digest()
    return digest[:key_len]


def user_string(key: bytes, file_id: bytes, revision: int) -> bytes:
    """Algorithm 4 (revision 2) or Algorithm 5 (revision 3 and later): `/U`."""
    if revision == 2:
        return rc4(key, PAD)
    seed = hashlib.md5(PAD + file_id).digest()
    out = rc4(key, seed)
    for i in range(1, 20):
        out = rc4(bytes(b ^ i for b in key), out)
    return out + PAD[:16]


def object_key(key: bytes, number: int, generation: int) -> bytes:
    """Algorithm 1, without the AES salt."""
    material = key + number.to_bytes(3, "little") + generation.to_bytes(2, "little")
    return hashlib.md5(material).digest()[: min(len(key) + 5, 16)]


def encrypt_streams(data: bytes, key: bytes) -> bytes:
    """Encrypts every stream payload in place. RC4 preserves length, so offsets hold."""
    out = bytearray(data)
    count = 0
    for match in re.finditer(rb"(?<![0-9])(\d+)\s+(\d+)\s+obj\b", data):
        number, generation = int(match.group(1)), int(match.group(2))
        head = data.find(b"stream", match.end())
        end = data.find(b"endobj", match.end())
        if head < 0 or (end >= 0 and head > end):
            continue
        start = head + 6
        if data[start : start + 2] == b"\r\n":
            start += 2
        elif data[start : start + 1] in (b"\n", b"\r"):
            start += 1
        stop = data.find(b"endstream", start)
        if stop < 0:
            continue
        payload = data[start:stop]
        # `endstream` is preceded by an EOL that is not part of the data.
        while payload.endswith(b"\n") or payload.endswith(b"\r"):
            payload = payload[:-1]
        if not payload:
            continue
        out[start : start + len(payload)] = rc4(object_key(key, number, generation), payload)
        count += 1
    return bytes(out), count


def build(source: bytes, revision: int, key_len: int, version: int) -> bytes:
    file_id = hashlib.md5(source[:4096]).digest()
    p = -4  # everything permitted except the two reserved low bits
    o = owner_string(b"", b"", revision, key_len)
    key = file_key(b"", o, p, file_id, revision, key_len)
    u = user_string(key, file_id, revision)

    body, encrypted = encrypt_streams(source, key)
    if encrypted == 0:
        raise SystemExit("no streams found to encrypt")

    highest = max(int(m.group(1)) for m in re.finditer(rb"(?<![0-9])(\d+)\s+\d+\s+obj\b", source))
    encrypt_num = highest + 1

    out = bytearray(body)
    if not out.endswith(b"\n"):
        out.extend(b"\n")
    encrypt_at = len(out)
    length_entry = b"" if version == 1 else b" /Length %d" % (key_len * 8)
    out.extend(
        b"%d 0 obj\n<< /Filter /Standard /V %d /R %d%s /O <%s> /U <%s> /P %d >>\nendobj\n"
        % (encrypt_num, version, revision, length_entry, o.hex().encode(), u.hex().encode(), p)
    )

    # An incremental update naming only the new object (7.5.6).
    prev = int(re.findall(rb"startxref\s+(\d+)", source)[-1])
    xref_at = len(out)
    out.extend(b"xref\n0 1\n0000000000 65535 f \n%d 1\n%010d 00000 n \n" % (encrypt_num, encrypt_at))
    root = re.search(rb"/Root\s+(\d+\s+\d+\s+R)", source).group(1)
    out.extend(
        b"trailer\n<< /Size %d /Root %s /Prev %d /Encrypt %d 0 R /ID [<%s> <%s>] >>\n"
        b"startxref\n%d\n%%%%EOF\n"
        % (encrypt_num + 1, root, prev, encrypt_num, file_id.hex().encode(),
           file_id.hex().encode(), xref_at)
    )
    return bytes(out)


def hash_2b(password: bytes, salt: bytes, udata: bytes = b"") -> bytes:
    """Algorithm 2.B, 7.6.4.3.4: the revision 6 hardened hash.

    Transcribed from the standard rather than from another implementation: K starts as
    SHA-256 of the input, then at least 64 rounds each build K1 from 64 repetitions of
    (password, K, udata), encrypt it with AES-128-CBC keyed by the halves of K, and
    rehash with SHA-256, 384 or 512 chosen by the first 16 bytes of E modulo 3.
    """
    k = hashlib.sha256(password + salt + udata).digest()
    round_number = 0
    while True:
        k1 = (password + k + udata) * 64
        e = aes.cbc_encrypt(k[:16], k[16:32], k1)
        # 256 is 1 modulo 3, so summing the bytes gives the same remainder as reading
        # the 16 bytes as one big-endian integer, which is what step (c) says.
        remainder = sum(e[:16]) % 3
        if remainder == 0:
            k = hashlib.sha256(e).digest()
        elif remainder == 1:
            k = hashlib.sha384(e).digest()
        else:
            k = hashlib.sha512(e).digest()
        round_number += 1
        # Steps (e) and (f): from round 64 on, stop once the last byte of E is small.
        if round_number >= 64 and e[-1] <= round_number - 32:
            return k[:32]


def hash_2a(revision: int, password: bytes, salt: bytes, udata: bytes = b"") -> bytes:
    """The hash Algorithm 2.A calls for.

    Revision 5 is Adobe's original extension and hashes once; revision 6 is what ISO
    32000-2 standardised and runs Algorithm 2.B.
    """
    if revision < 6:
        return hashlib.sha256(password + salt + udata).digest()
    return hash_2b(password, salt, udata)


def aes256_strings(
    password: bytes,
    key: bytes,
    p: int,
    encrypt_metadata: bool,
    owner: bytes = b"",
    revision: int = 6,
) -> dict:
    """Algorithms 8, 9 and 10: /U, /UE, /O, /OE and /Perms."""
    owner = owner or password
    u_vsalt, u_ksalt = os.urandom(8), os.urandom(8)
    u = hash_2a(revision, password, u_vsalt) + u_vsalt + u_ksalt
    ue = aes.cbc_encrypt(hash_2a(revision, password, u_ksalt), bytes(16), key)

    o_vsalt, o_ksalt = os.urandom(8), os.urandom(8)
    o = hash_2a(revision, owner, o_vsalt, u) + o_vsalt + o_ksalt
    oe = aes.cbc_encrypt(hash_2a(revision, owner, o_ksalt, u), bytes(16), key)

    # Algorithm 10: /P extended to 64 bits with the high half set, then the marker.
    block = bytearray((p & 0xFFFFFFFF).to_bytes(4, "little") + b"\xff" * 4)
    block.append(ord("T") if encrypt_metadata else ord("F"))
    block.extend(b"adb")
    block.extend(os.urandom(4))
    perms = aes.ecb_encrypt(key, bytes(block))

    return {"U": u, "UE": ue, "O": o, "OE": oe, "Perms": perms}


def build_aes256(source: bytes, user: bytes = b"", owner: bytes = b"", revision: int = 6) -> bytes:
    """An AES-256 revision 6 document, rebuilt rather than patched.

    AES adds an IV and padding, so a stream's ciphertext is longer than its plaintext
    and nothing can be encrypted in place. Every object is therefore re-serialised and
    the cross-reference rebuilt.
    """
    key = os.urandom(32)
    p = -4
    strings = aes256_strings(user, key, p, True, owner, revision)

    objects = parse_objects(source)
    encrypt_num = max(objects) + 1

    out = bytearray(b"%PDF-2.0\n")
    offsets = {}
    for number in sorted(objects):
        body, stream = objects[number]
        offsets[number] = len(out)
        if stream is None:
            out.extend(b"%d 0 obj\n%s\nendobj\n" % (number, body))
            continue
        # 7.6.4.3.4: revision 5 and later use the file key directly, with no per-object
        # derivation, and AES prefixes the IV.
        cipher = os.urandom(16)
        cipher += aes.cbc_encrypt(key, cipher, aes.pkcs7(stream))
        # `/Length` may be `N` or `N G R`, and samples/sample.pdf writes every one of
        # its 31 streams as an indirect reference. Matching only the integer turns
        # `/Length 5 0 R` into `/Length 1234 0 R`, which is why the first fixture
        # opened in PDFKit — the structure was fine — and rendered nothing.
        if re.search(rb"/Length\s+\d+\s+\d+\s+R", body):
            body = re.sub(rb"/Length\s+\d+\s+\d+\s+R", b"/Length %d" % len(cipher), body)
        elif re.search(rb"/Length\s+\d+", body):
            body = re.sub(rb"/Length\s+\d+", b"/Length %d" % len(cipher), body)
        else:
            body = body.rstrip()[:-2] + b" /Length %d >>" % len(cipher)
        out.extend(b"%d 0 obj\n%s\nstream\n" % (number, body))
        out.extend(cipher)
        out.extend(b"\nendstream\nendobj\n")

    offsets[encrypt_num] = len(out)
    out.extend(
        b"%d 0 obj\n<< /Filter /Standard /V 5 /R %d /Length 256 /P %d"
        b" /CF << /StdCF << /CFM /AESV3 /Length 32 /AuthEvent /DocOpen >> >>"
        b" /StmF /StdCF /StrF /StdCF"
        b" /O <%s> /U <%s> /OE <%s> /UE <%s> /Perms <%s> >>\nendobj\n"
        % (
            encrypt_num,
            revision,
            p,
            strings["O"].hex().encode(),
            strings["U"].hex().encode(),
            strings["OE"].hex().encode(),
            strings["UE"].hex().encode(),
            strings["Perms"].hex().encode(),
        )
    )

    highest = encrypt_num
    xref_at = len(out)
    out.extend(b"xref\n0 %d\n0000000000 65535 f \n" % (highest + 1))
    for number in range(1, highest + 1):
        if number in offsets:
            out.extend(b"%010d 00000 n \n" % offsets[number])
        else:
            out.extend(b"0000000000 65535 f \n")
    root = re.search(rb"/Root\s+(\d+)\s+\d+\s+R", source).group(1)
    file_id = hashlib.md5(source[:4096]).hexdigest().encode()
    out.extend(
        b"trailer\n<< /Size %d /Root %s 0 R /Encrypt %d 0 R /ID [<%s> <%s>] >>\n"
        b"startxref\n%d\n%%%%EOF\n"
        % (highest + 1, root, encrypt_num, file_id, file_id, xref_at)
    )
    return bytes(out)


def parse_objects(data: bytes) -> dict:
    """Every `N 0 obj` in the file, as (dictionary bytes, stream payload or None)."""
    objects = {}
    for match in re.finditer(rb"(?<![0-9])(\d+)\s+(\d+)\s+obj\b", data):
        number = int(match.group(1))
        end = data.find(b"endobj", match.end())
        if end < 0:
            continue
        chunk = data[match.end() : end]
        head = chunk.find(b"stream")
        if head < 0:
            objects[number] = (chunk.strip(), None)
            continue
        body = chunk[:head].strip()
        start = head + 6
        if chunk[start : start + 2] == b"\r\n":
            start += 2
        elif chunk[start : start + 1] in (b"\n", b"\r"):
            start += 1
        stop = chunk.find(b"endstream", start)
        payload = chunk[start:stop]
        while payload.endswith(b"\n") or payload.endswith(b"\r"):
            payload = payload[:-1]
        objects[number] = (body, payload)
    return objects


VARIANTS = {
    "rc4_40.pdf": dict(version=1, revision=2, key_len=5),
    "rc4_128.pdf": dict(version=2, revision=3, key_len=16),
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path("samples/sample.pdf"))
    parser.add_argument("--out", type=Path, default=Path("target/encrypted"))
    args = parser.parse_args()

    aes.self_test()
    source = args.source.read_bytes()
    args.out.mkdir(parents=True, exist_ok=True)
    for name, spec in VARIANTS.items():
        path = args.out / name
        path.write_bytes(build(source, **spec))
        print(f"  {path}  V{spec['version']} R{spec['revision']} {spec['key_len'] * 8}-bit RC4")
    path = args.out / "aes256.pdf"
    path.write_bytes(build_aes256(source))
    print(f"  {path}  V5 R6 256-bit AES, empty passwords")
    path = args.out / "aes256_owner.pdf"
    path.write_bytes(build_aes256(source, user=b"userpw", owner=b"ownerpw"))
    print(f"  {path}  V5 R6 256-bit AES, user 'userpw' / owner 'ownerpw'")
    path = args.out / "aes256_r5.pdf"
    path.write_bytes(build_aes256(source, revision=5))
    print(f"  {path}  V5 R5 256-bit AES (Adobe extension, single SHA-256)")

    # 2.A step (a) requires SASLprep before the UTF-8 conversion, and the practical
    # half of SASLprep is NFKC. A producer that follows the clause stores /U for the
    # *normalised* password; a user types the form on their keyboard. A reader that
    # skips normalisation cannot open this file, and one that applies it can.
    typed = "\uFB01re"  # the ligature fi, which NFKC folds to "fi"
    stored = unicodedata.normalize("NFKC", typed).encode()
    path = args.out / "aes256_saslprep.pdf"
    path.write_bytes(build_aes256(source, user=stored, owner=stored))
    print(f"  {path}  V5 R6, password typed {typed!r} stored as {stored.decode()!r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
