#!/usr/bin/env python3
"""UI-11: spacing, type size and corner radius come from the declared scales.

There were eight spacing values, eight type sizes and three radii in a window that holds
no document text — which is to say there was no scale, only a habit of picking a number.
A step that says nothing about the break it represents is a step the eye cannot read, and
`app/layout.rs` had already reached the other half of this conclusion for the page grid:
the row gap is twice the column gap *because a row break is a bigger break*.

The forms checked are the ones that set a chrome dimension. Geometry on the page —
a snap marker's radius, a tick's length, a stroke's width — is not chrome and is not here;
those live in the canvas's own 72pt space.

Exits non-zero with a line per literal. No arguments.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUI = ROOT / "crates/fepdf-gui/src"
TOKENS = GUI / "app/theme.rs"

FORMS = (
    re.compile(r"add_space\(\s*[0-9]+\.[0-9]+"),
    re.compile(r"\.size\(\s*[0-9]+\.[0-9]+"),
    re.compile(r"FontId::(?:proportional|monospace)\(\s*[0-9]+\.[0-9]+"),
    re.compile(r"corner_radius\(\s*[0-9]+\.[0-9]+"),
    re.compile(r"(?:rect_filled|rect_stroke)\([^,]+,\s*[0-9]+\.[0-9]+"),
)


def main() -> int:
    failures: list[str] = []

    for path in sorted(GUI.rglob("*.rs")):
        if path == TOKENS:
            continue
        for line_no, line in enumerate(path.read_text().splitlines(), 1):
            for form in FORMS:
                match = form.search(line)
                if match:
                    rel = path.relative_to(ROOT)
                    failures.append(f"{rel}:{line_no}: {match.group(0).strip()}")
                    break

    for line in failures:
        print(line)
    print(f"UI-11: {len(failures)} dimensions outside the scales in theme.rs")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
