#!/usr/bin/env python3
"""Builds an unencrypted wrapper document (ISO 32000-2, 7.6.7).

A wrapper is how a producer using a *non-standard* security handler tells a reader what
it needs. The real file is embedded as an encrypted payload; the wrapper itself is
plain, and carries instructions for a reader that lacks the filter.

The clause defines recognition precisely, which is what makes it implementable without
being able to decrypt anything:

  - `/Collection` in the catalogue, `/View /H`, naming the payload as the initial
    document
  - the payload's file specification in the `EmbeddedFiles` name tree, which "shall
    contain exactly one entry"
  - the same file specification in the catalogue's `/AF` array
  - `/AFRelationship /EncryptedPayload` on that file specification
  - an `/EP` dictionary on it, whose `/Subtype` names the filter needed (Table 28)

Built here rather than found, because no corpus file is one. The payload is real: the
bytes of `samples/sample.pdf` with every byte XOR-ed under a name this standard does
not define, which is the point — a conforming reader must recognise the wrapper and
report the filter it cannot supply.

    python3 scripts/test/make_wrapper.py [--out target/encrypted]
"""

from __future__ import annotations

import argparse
import sys
import zlib
from pathlib import Path

FILTER_NAME = "AcmeCustomCrypto"
FILTER_VERSION = "1.0"


def payload_bytes(source: bytes) -> bytes:
    """Stands in for a document encrypted by a handler this standard does not define."""
    return bytes(b ^ 0x5A for b in source)


def build(source: bytes) -> bytes:
    payload = payload_bytes(source)
    notice = (
        b"BT /F1 14 Tf 60 700 Td (This document is an unencrypted wrapper.) Tj\n"
        b"0 -22 Td (The content is an encrypted payload requiring the) Tj\n"
        b"0 -22 Td (" + FILTER_NAME.encode() + b" security handler.) Tj ET\n"
    )
    notice_z = zlib.compress(notice)

    objects: dict[int, bytes] = {}
    # 1 catalogue, 2 pages, 3 page, 4 font, 5 notice stream,
    # 6 filespec, 7 payload stream, 8 names, 9 embedded files
    objects[1] = (
        b"<< /Type /Catalog /Pages 2 0 R /Names 8 0 R /AF [6 0 R]"
        b" /Collection << /Type /Collection /View /H /D (payload) >> >>"
    )
    objects[2] = b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>"
    objects[3] = (
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]"
        b" /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
    )
    objects[4] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"
    objects[5] = (b"<< /Length %d /Filter /FlateDecode >>" % len(notice_z), notice_z)
    objects[6] = (
        b"<< /Type /Filespec /F (payload.pdf) /UF (payload.pdf)"
        b" /AFRelationship /EncryptedPayload"
        b" /Desc (This embedded file is encrypted using the " + FILTER_NAME.encode() + b" filter)"
        b" /EF << /F 7 0 R >>"
        b" /EP << /Type /EncryptedPayload /Subtype /" + FILTER_NAME.encode() +
        b" /Version /" + FILTER_VERSION.encode() + b" >> >>"
    )
    objects[7] = (b"<< /Length %d /Type /EmbeddedFile >>" % len(payload), payload)
    objects[8] = b"<< /EmbeddedFiles 9 0 R >>"
    # "shall contain exactly one entry, for the encrypted payload document"
    objects[9] = b"<< /Names [(payload) 6 0 R] >>"

    out = bytearray(b"%PDF-2.0\n")
    offsets: dict[int, int] = {}
    for number in sorted(objects):
        body = objects[number]
        offsets[number] = len(out)
        if isinstance(body, tuple):
            head, data = body
            out.extend(b"%d 0 obj\n%s\nstream\n" % (number, head))
            out.extend(data)
            out.extend(b"\nendstream\nendobj\n")
        else:
            out.extend(b"%d 0 obj\n%s\nendobj\n" % (number, body))

    highest = max(objects)
    xref_at = len(out)
    out.extend(b"xref\n0 %d\n0000000000 65535 f \n" % (highest + 1))
    for number in range(1, highest + 1):
        out.extend(b"%010d 00000 n \n" % offsets[number])
    out.extend(
        b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (highest + 1, xref_at)
    )
    return bytes(out)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path("samples/sample.pdf"))
    parser.add_argument("--out", type=Path, default=Path("target/encrypted"))
    args = parser.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    path = args.out / "wrapper.pdf"
    path.write_bytes(build(args.source.read_bytes()))
    print(f"  {path}  7.6.7 unencrypted wrapper, payload needs /{FILTER_NAME}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
