#!/usr/bin/env python3
"""UI-14: which view answers an act is declared once, in `view::Act`.

The split between the page view and the tile view was spelled four ways — `is_page_view`,
`selects_pages`, `selects_text` and a bare `zoom < TILE_ZOOM` — and declared nowhere, so
what a reader could do where was something you found out by reading the whole window. Two
of the four had drifted by the time this was written: a content tool could be switched on
in the grid, where its clicks were thrown away, and the arrow keys built a page selection
that only the other view showed.

Two halves, because the table is worth nothing if the call sites do not read it and
nothing if a variant sits in it unread:

1. The zoom boundary is compared in `view.rs` and nowhere else. Everything else asks
   `PDFView::does(Act::…)`, or `is_page_view()` when the question is what to *draw*.
2. Every `Act` variant is asked about somewhere outside `view.rs`. A variant nothing
   consults is a rule nobody enforces.

Tests and comments are not code for either half. Exits non-zero with a line per finding.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUI = ROOT / "crates/fepdf-gui/src"
HOME = GUI / "view.rs"

BOUNDARY = re.compile(r"\bTILE_ZOOM\b")
VARIANT = re.compile(r"^\s{4}([A-Z][A-Za-z]*),$", re.MULTILINE)
ASKED = re.compile(r"does\(\s*(?:crate::view::)?Act::([A-Za-z]+)\s*\)")


def code_only(text: str) -> str:
    """The source with its comments and its `#[cfg(test)]` modules blanked out."""
    out = []
    for line in text.splitlines():
        stripped = line.lstrip()
        out.append("" if stripped.startswith("//") else line)
    body = "\n".join(out)
    cut = body.find("#[cfg(test)]")
    return body if cut < 0 else body[:cut]


def act_variants() -> list[str]:
    """The names declared in the `Act` enum."""
    text = HOME.read_text(encoding="utf-8")
    start = text.index("pub enum Act {")
    return VARIANT.findall(text[start : text.index("\n}", start)])


def main() -> int:
    failures: list[str] = []
    asked: set[str] = set()

    for path in sorted(GUI.rglob("*.rs")):
        body = code_only(path.read_text(encoding="utf-8"))
        asked.update(ASKED.findall(body))
        if path == HOME:
            continue
        for number, line in enumerate(body.splitlines(), 1):
            if BOUNDARY.search(line):
                here = path.relative_to(ROOT)
                failures.append(f"  {here}:{number} compares the zoom boundary: {line.strip()}")

    variants = act_variants()
    for name in variants:
        if name not in asked:
            failures.append(f"  view::Act::{name} is declared but nothing asks for it")

    for line in failures:
        print(line)
    print(
        f"UI-14: {len(variants)} acts declared, {len(asked)} asked for, "
        f"{len(failures)} failing"
    )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
