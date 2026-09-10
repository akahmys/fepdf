#!/usr/bin/env python3
"""UI-6: a change to the document is recorded, so an undo covers it.

`fepdf-gui` takes an operation back by opening the file again and replaying what is left
(`ARCHITECTURE.md` §4.1), which works only if the journal holds *every* change. A mutation
that reaches `Document::apply` on its own is not in it, and nothing about the window says
so: the undo control stays lit, takes back the operation before it, and the edit that was
never recorded silently survives.

That is not hypothetical. Retagging an element — `Operation::UpdateStructElem`, the
tagging brush — called `apply` directly for as long as the history existed, so an undo
after retagging took back the wrong thing.

Two call sites are allowed, and both are in `worker.rs`:

  * `apply_recorded`, which is the one path a mutation takes;
  * `handle_open`, which is the replay itself, applying a journal rather than adding to
    one.

Exits non-zero with a line per unrecorded mutation. No arguments.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUI = ROOT / "crates/fepdf-gui/src"

APPLY = re.compile(r"\.apply\(")
FUNCTION = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?fn (\w+)")
ALLOWED = {"apply_recorded", "handle_open"}


def enclosing(lines: list[str], index: int) -> str:
    """The name of the function the line at `index` sits in, or `""`."""
    for line in reversed(lines[:index]):
        match = FUNCTION.match(line)
        if match:
            return match.group(1)
    return ""


def main() -> int:
    failures: list[str] = []
    allowed_seen = 0

    for path in sorted(GUI.rglob("*.rs")):
        lines = path.read_text().splitlines()
        for line_no, line in enumerate(lines, 1):
            if not APPLY.search(line):
                continue
            where = enclosing(lines, line_no - 1)
            if where in ALLOWED:
                allowed_seen += 1
                continue
            rel = path.relative_to(ROOT)
            failures.append(f"{rel}:{line_no}: `apply` in `{where}`, which is not the one path")

    for line in failures:
        print(line)
    print(f"UI-6: {len(failures)} unrecorded mutations, {allowed_seen} through the named two")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
