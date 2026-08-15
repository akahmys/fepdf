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
import re
import sys
from pathlib import Path

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


VARIANTS = {
    "rc4_40.pdf": dict(version=1, revision=2, key_len=5),
    "rc4_128.pdf": dict(version=2, revision=3, key_len=16),
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path("samples/sample.pdf"))
    parser.add_argument("--out", type=Path, default=Path("target/encrypted"))
    args = parser.parse_args()

    source = args.source.read_bytes()
    args.out.mkdir(parents=True, exist_ok=True)
    for name, spec in VARIANTS.items():
        path = args.out / name
        path.write_bytes(build(source, **spec))
        print(f"  {path}  V{spec['version']} R{spec['revision']} {spec['key_len'] * 8}-bit RC4")
    return 0


if __name__ == "__main__":
    sys.exit(main())
