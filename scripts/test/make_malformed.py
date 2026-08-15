#!/usr/bin/env python3
"""Rebuilds the six deliberately malformed PDFs used as the reader's acceptance test.

ADR-0003 and ROADMAP.md both quote results measured against these files, but the files
themselves were only ever in a temporary directory. This regenerates them from a healthy
sample so the measurements can be repeated rather than trusted.

Each damage targets one part of ISO 32000-2 clause 7.5, and each is located by content
rather than by offset, so the script survives a different sample.

    python3 scripts/test/make_malformed.py [--source samples/sample.pdf] [--out DIR]
"""

import argparse
import pathlib
import re
import sys


def wrong_indirect_length(data: bytes) -> bytes:
    """Points a stream's /Length at the wrong object (7.3.8.2)."""
    match = re.search(rb"/Length (\d+) 0 R", data)
    if not match:
        raise LookupError("no indirect /Length to damage")
    wrong = b"2" if match.group(1) != b"2" else b"3"
    return data[: match.start(1)] + wrong + data[match.end(1) :]


def broken_startxref(data: bytes) -> bytes:
    """Sends startxref to an offset that holds nothing (7.5.5)."""
    match = None
    for match in re.finditer(rb"startxref\s*\n(\d+)", data):
        pass
    if match is None:
        raise LookupError("no startxref to damage")
    return data[: match.start(1)] + b"999999" + data[match.end(1) :]


def corrupt_xref_table(data: bytes) -> bytes:
    """Overwrites the head of the cross-reference table (7.5.4)."""
    at = data.rfind(b"xref\n0 ")
    if at < 0:
        raise LookupError("no cross-reference table to damage")
    return data[: at + 4] + b"X" * 55 + data[at + 59 :]


def prepend_junk(data: bytes) -> bytes:
    """Puts bytes before %PDF-, as mail gateways and scanners do (7.5.2)."""
    return b"X" * 300 + data


def destroy_trailer(data: bytes) -> bytes:
    """Removes the trailer keyword, leaving its dictionary behind (7.5.5)."""
    at = data.rfind(b"trailer")
    if at < 0:
        raise LookupError("no trailer to damage")
    return data[:at] + b"X" * 7 + data[at + 7 :]


def truncate(data: bytes) -> bytes:
    """Cuts the file at 60%, before the page tree and the trailer."""
    return data[: len(data) * 6 // 10]


DAMAGE = {
    "bad_length.pdf": wrong_indirect_length,
    "bad_startxref.pdf": broken_startxref,
    "bad_xreftable.pdf": corrupt_xref_table,
    "junk_prefix.pdf": prepend_junk,
    "no_trailer.pdf": destroy_trailer,
    "truncated.pdf": truncate,
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", default="samples/sample.pdf", type=pathlib.Path)
    parser.add_argument("--out", default="target/malformed", type=pathlib.Path)
    args = parser.parse_args()

    if not args.source.exists():
        print(f"no such sample: {args.source}", file=sys.stderr)
        return 2

    data = args.source.read_bytes()
    args.out.mkdir(parents=True, exist_ok=True)
    for name, damage in DAMAGE.items():
        path = args.out / name
        path.write_bytes(damage(data))
        print(f"{path}  {path.stat().st_size} bytes")

    print(f"\n{len(DAMAGE)} files from {args.source}. To measure what the reader makes of them:")
    print(f"  cargo run -q -p fepdf-model --example read_probe -- {args.out}/*.pdf")
    return 0


if __name__ == "__main__":
    sys.exit(main())
