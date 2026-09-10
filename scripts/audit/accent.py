#!/usr/bin/env python3
"""UI-10: rust marks what the reader is touching, and marks nothing else.

The accent has one meaning, and the way to lose it is not to paint a hundred things with
it by hand — it is to put it somewhere egui reads from for something else. That is what
happened: `widgets.active.fg_stroke` was the accent, and `Visuals::strong_text_color`
returns `widgets.active.text_color()`, so **every `RichText::strong()` in the application
drew in the accent** — the document properties, the About window's own name, the heading
of every drawer. 3,724 accent pixels in one window. It passed twenty-five audit steps and
104 tests, because nothing there looks at a colour.

egui reaches the accent through `visuals.selection`, which is what a selection is. The
`widgets.*` grades are the states of a control — resting, hovered, pressed — and a pressed
control is not a selected one. So the accent may name a `selection` field and no other.

Exits non-zero with a line per assignment that puts it elsewhere. No arguments.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUI = ROOT / "crates/fepdf-gui/src"

# `visuals.<something> = ...rust::ACCENT...` — the assignment, not the use.
ASSIGNMENT = re.compile(r"visuals\.([\w.]+)\s*=.*rust::(?:ACCENT|wash)")
ALLOWED_PREFIX = "selection"
# The one exception, and it is not a widget grade: a hyperlink is a thing being reached
# for, and egui has no other channel for it.
ALLOWED_EXACT = {"hyperlink_color"}


def main() -> int:
    failures: list[str] = []
    allowed = 0

    for path in sorted(GUI.rglob("*.rs")):
        for line_no, line in enumerate(path.read_text().splitlines(), 1):
            match = ASSIGNMENT.search(line)
            if not match:
                continue
            field = match.group(1)
            if field.startswith(ALLOWED_PREFIX) or field in ALLOWED_EXACT:
                allowed += 1
                continue
            rel = path.relative_to(ROOT)
            failures.append(
                f"{rel}:{line_no}: the accent reaches `visuals.{field}`, which is not a selection"
            )

    for line in failures:
        print(line)
    print(f"UI-10: {len(failures)} accents outside a selection, {allowed} through one")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
