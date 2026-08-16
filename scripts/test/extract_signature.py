#!/usr/bin/env python3
"""Pulls a PDF signature apart the way a verifier does, with no fepdf code involved.

Reads /ByteRange out of the file, writes exactly the bytes it names to one file, and
writes the DER out of /Contents to another. Deliberately naive: it takes the file at
its word rather than reasoning about what the writer intended, which is the point --
if /ByteRange is wrong, this produces the wrong content and openssl says so.

Usage: extract_signature.py <signed.pdf> <content-out> <der-out>
"""

import re
import sys


def main() -> int:
    pdf_path, content_path, der_path = sys.argv[1:4]
    with open(pdf_path, "rb") as handle:
        pdf = handle.read()

    match = re.search(rb"/ByteRange\s*\[([^\]]*)\]", pdf)
    if not match:
        print("no /ByteRange in the file", file=sys.stderr)
        return 1
    numbers = [int(n) for n in match.group(1).split()]
    if len(numbers) != 4:
        print(f"/ByteRange is not four numbers: {numbers}", file=sys.stderr)
        return 1
    a, b, c, d = numbers

    if a != 0:
        print(f"the first range starts at {a}, not the start of the file", file=sys.stderr)
        return 1
    if c + d != len(pdf):
        print(f"the ranges end at {c + d}, the file is {len(pdf)} bytes", file=sys.stderr)
        return 1

    # The gap between the ranges must be exactly the /Contents string, brackets included.
    gap = pdf[a + b : c]
    if not (gap.startswith(b"<") and gap.endswith(b">")):
        print(f"the gap is not a hex string: {gap[:16]!r}...{gap[-16:]!r}", file=sys.stderr)
        return 1

    signature = bytes.fromhex(gap[1:-1].decode("ascii"))
    # DER is self-delimiting; everything past the outer element is reservation padding.
    length = der_element_length(signature)
    if any(signature[length:]):
        print("the padding after the signature is not zero", file=sys.stderr)
        return 1

    with open(content_path, "wb") as handle:
        handle.write(pdf[a : a + b] + pdf[c : c + d])
    with open(der_path, "wb") as handle:
        handle.write(signature[:length])

    print(f"{b + d} of {len(pdf)} bytes, signature {length} of {len(signature)} reserved")
    return 0


def der_element_length(data: bytes) -> int:
    """Total length of the DER element at the start of `data`, header included."""
    first = data[1]
    if first < 0x80:
        return 2 + first
    count = first & 0x7F
    return 2 + count + int.from_bytes(data[2 : 2 + count], "big")


if __name__ == "__main__":
    sys.exit(main())
