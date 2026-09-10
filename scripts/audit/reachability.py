#!/usr/bin/env python3
"""UI-4 and UI-12: one home for each feature, and shortcuts to it.

The two are halves of one property. A feature with no visible entry point is unreachable
by anyone who does not already know it exists — seven of this window's twelve `Operation`
variants lived that way, behind the command palette — and a feature with two is a reader
asking which one is the real one. Exactly one, and any number of shortcuts to it.

A shortcut is a key, the command palette, or a capture plan. Everything else is a surface:
a control someone can see and press.

Drawers are not counted here and are not unchecked. The rail walks `ActiveDrawer::ALL`
rather than a list beside it, and `ActiveDrawer::face` has no wildcard arm, so a new
drawer does not compile until it has an icon and a name — which is a stronger guarantee
than this script can give. What is checked is that `ALL` still holds every variant, since
an array cannot be exhaustive on its own.

Exits non-zero with a line per feature. No arguments.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUI = ROOT / "crates/fepdf-gui/src"
DRAWERS = GUI / "sidebar/mod.rs"

SHORTCUT_FILES = {"command_palette.rs", "capture.rs"}
SHORTCUT_FNS = {
    "handle_file_and_edit_shortcuts",
    "handle_history_shortcuts",
    "handle_zoom_shortcuts",
    "handle_page_and_selection_shortcuts",
}
FUNCTION = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?fn (\w+)")


def enclosing(lines: list[str], index: int) -> str:
    for line in reversed(lines[:index]):
        match = FUNCTION.match(line)
        if match:
            return match.group(1)
    return ""


def drawers_are_all_listed() -> list[str]:
    """`ActiveDrawer::ALL` against the variants of the enum it claims to hold."""
    text = DRAWERS.read_text()
    body = text.split("pub enum ActiveDrawer {")[1].split("\n}")[0]
    variants = {v for v in re.findall(r"^\s{4}(\w+),", body, re.M) if v != "None"}
    listed = set(re.findall(r"Self::(\w+),", text.split("pub const ALL")[1].split("];")[0]))
    missing = sorted(variants - listed)
    return [f"sidebar/mod.rs: ActiveDrawer::ALL omits {v}, so the rail draws no door" for v in missing]


def main() -> int:
    failures = drawers_are_all_listed()

    text = "\n".join(p.read_text() for p in GUI.rglob("*.rs"))
    features = sorted(set(re.findall(r"pub (show_\w+): bool", text)))

    for feature in features:
        setter = re.compile(rf"\b{feature}\s*=\s*(?:true|!)")
        surfaces: list[str] = []
        shortcuts = 0
        for path in sorted(GUI.rglob("*.rs")):
            lines = path.read_text().splitlines()
            for line_no, line in enumerate(lines):
                if not setter.search(line):
                    continue
                where = enclosing(lines, line_no)
                if path.name in SHORTCUT_FILES or where in SHORTCUT_FNS:
                    shortcuts += 1
                else:
                    surfaces.append(f"{path.name}:{where}")
        if not surfaces:
            failures.append(f"{feature}: no visible entry point, {shortcuts} shortcuts (UI-4)")
        elif len(surfaces) > 1:
            failures.append(f"{feature}: {len(surfaces)} homes — {', '.join(surfaces)} (UI-12)")

    for line in failures:
        print(line)
    print(f"UI-4/UI-12: {len(features)} features and every drawer checked, {len(failures)} failing")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
