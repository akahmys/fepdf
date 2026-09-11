#!/usr/bin/env python3
"""UI-9: a colour is written in `app/theme.rs` or it is not written.

The palette's own discipline caught only half of this. `theme::colors` carries no
`#[allow(dead_code)]`, so rustc reports a constant nobody paints with — but nothing
reported a colour that never reached the palette at all, and there were forty of them
against seventeen constants. Twenty-eight sat in `view.rs`, which is the page and
everything drawn over it: the surface the reader looks at longest was the one the palette
did not govern.

Three sites are exempt, named here the way `CODING.md` Rule 9 names its one exemption. In
each the value is not a colour the interface chose:

  * two `Painter::image` tints, where white is the identity multiplier — tinting the
    document is the one thing this window must not do;
  * the fill of a committed redaction, which is black because burning writes black;
  * the white a page is rasterised onto, which is the sheet rather than a choice.

Exits non-zero with a line per unexempted literal. No arguments.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUI = ROOT / "crates/fepdf-gui/src"
PALETTE = GUI / "app/theme.rs"

# **Two colour types, because the window paints with two.** This read `Color32` alone
# and passed a `peniko::Color::from_rgb8(235, 237, 240)` in `vello_egui.rs` that was the
# workbench the reader actually saw — three shades from the `paper::CANVAS` it was
# supposed to be. A check that cannot see half its subject is indistinguishable from one
# nobody runs, which is the sentence `docs/specs/README.md` ends on.
LITERAL = re.compile(
    r"Color32::(?:from_rgba?_?\w*\(|[A-Z][A-Z_]{2,})"
    r"|peniko::Color::(?:from_rgb8?a?\(|[A-Z][A-Z_]{2,})"
)

# (path relative to the GUI source root, the call it appears in, why)
EXEMPT = {
    ("view.rs", "Painter::image tint (viewport texture)"),
    ("view.rs", "Painter::image tint (thumbnail)"),
    ("view.rs", "committed redaction fill"),
    ("vello_egui.rs", "the white a page is rasterised onto"),
}
# An exempt line must say so, so that moving one re-opens the question.
EXEMPT_MARK = "UI-9's"


def main() -> int:
    failures: list[str] = []
    exempted = 0

    for path in sorted(GUI.rglob("*.rs")):
        if path == PALETTE:
            continue
        lines = path.read_text().splitlines()
        for line_no, line in enumerate(lines, 1):
            if not LITERAL.search(line):
                continue
            # The reason sits in the comment block immediately above.
            window = "\n".join(lines[max(0, line_no - 5) : line_no])
            if EXEMPT_MARK in window:
                exempted += 1
                continue
            rel = path.relative_to(ROOT)
            failures.append(f"{rel}:{line_no}: {line.strip()[:96]}")

    for line in failures:
        print(line)
    print(
        f"UI-9: {len(failures)} colour literals outside theme.rs, "
        f"{exempted} exempt of the {len(EXEMPT)} named"
    )
    if exempted != len(EXEMPT):
        print(
            f"UI-9: {exempted} sites claim exemption and {len(EXEMPT)} are named — "
            f"the list in this file and the code disagree"
        )
        return 1
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
