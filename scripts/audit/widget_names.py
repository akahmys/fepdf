#!/usr/bin/env python3
"""UI-2: a widget whose only content is a glyph carries a name by some other means.

egui takes a widget's text for its accessible name, and the text of an icon button in
this window is a private-use codepoint — so twenty-six controls announced themselves as
`U+E0CC` and the like. AccessKit is compiled in (eight crates in `Cargo.lock`, because
`eframe`'s default features are not switched off), which means the tree is being published
with those names in it.

This is the rule the product has least excuse for breaking. `fepdf-gui` audits documents
against the Matterhorn protocol, and principle P4 says the checks it makes of a file apply
to its own window.

The check: an icon codepoint reaches the screen through `icons::icon_action`, which takes
the name, or through a `ui.label` — decoration, which is not interactive and needs no
name. Anything else — a `Button` assembled by hand around a glyph — is a control a screen
reader cannot announce.

Exits non-zero with a line per unnamed control. No arguments.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUI = ROOT / "crates/fepdf-gui/src"
ICONS = GUI / "app/icons.rs"

GLYPH = re.compile(r"\bglyph::([A-Z_]+)")
# The ways a glyph may reach the screen.
NAMED = re.compile(r"icon_action\(")
# Decoration: a glyph that is not a control and so has nothing to name.
#
# `ui.label` makes no `Response` anyone reads, and a `Painter` makes none at all — it
# cannot be clicked, focused or reached by AccessKit, so a glyph drawn straight onto one
# is a mark on the canvas rather than a control. The page-still-drawing icon is drawn that
# way, over a page the reader cannot press.
DECORATION = re.compile(r"ui\.label\(|painter\(\)\.text\(|painter\.text\(")


def main() -> int:
    failures: list[str] = []
    named = 0
    decorative = 0

    for path in sorted(GUI.rglob("*.rs")):
        if path == ICONS:
            continue
        lines = path.read_text().splitlines()
        for line_no, line in enumerate(lines, 1):
            if not GLYPH.search(line):
                continue
            # A tuple of (variant, glyph, key) is data; the call that draws it is checked
            # where it happens.
            if line.rstrip().endswith("),") and "(" in line and "icon_action" not in line:
                continue
            window = "\n".join(lines[max(0, line_no - 6) : line_no + 2])
            if NAMED.search(window):
                named += 1
            elif DECORATION.search(window):
                decorative += 1
            else:
                rel = path.relative_to(ROOT)
                failures.append(f"{rel}:{line_no}: {line.strip()[:80]}")

    for line in failures:
        print(line)
    print(f"UI-2: {len(failures)} unnamed icon controls, {named} named, {decorative} decorative")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
