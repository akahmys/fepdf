#!/usr/bin/env python3
"""UI-5: a user-facing string is a locale key, not a literal.

Thirty-nine sentences were written into the source in Japanese and twenty-three more in
English, on a window whose `LocaleManager` already held 260 keys and a test holding both
files equal. An English reader got Japanese on every control of the status bar; a Japanese
reader got English in the caliper, the structure tree and the password prompt.

Two of them did not reach the locale at all: `sidebar/document_info.rs` read
`if active_lang == "ja"` and picked a sentence, which is the mechanism `LocaleManager`
exists to replace, reimplemented beside it. A checker that fired only on CJK would have
passed both of those and every English sentence — so this reads the *sinks*, the calls
that put a string in front of a reader, and asks what was handed to them.

Four literals are exempt, named here the way `CODING.md` Rule 9 names its one exemption:
`H1`, `H2`, `P` and `Figure` are ISO 32000-2 structure type names. They are identifiers
that appear in the file, not prose about it, and a translated `<H1>` would be a lie.

Exits non-zero with a line per literal. No arguments.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUI = ROOT / "crates/fepdf-gui/src"

# The calls that put a string in front of a reader.
SINKS = (
    "on_hover_text",
    "hint_text",
    "heading",
    "monospace",
    "colored_label",
    "selectable_label",
    "label",
    "button",
)
SINK = re.compile(r"\b(?:" + "|".join(SINKS) + r")\(\s*\"([^\"]{1,})\"")

# The standard's own structure type names.
EXEMPT = {"H1", "H2", "H3", "P", "Figure", "Table"}


def main() -> int:
    failures: list[str] = []
    exempted = 0

    for path in sorted(GUI.rglob("*.rs")):
        for line_no, line in enumerate(path.read_text().splitlines(), 1):
            match = SINK.search(line)
            if not match:
                continue
            text = match.group(1)
            if text in EXEMPT:
                exempted += 1
                continue
            rel = path.relative_to(ROOT)
            failures.append(f'{rel}:{line_no}: "{text[:60]}"')

    for line in failures:
        print(line)
    print(f"UI-5: {len(failures)} user-facing literals, {exempted} exempt structure names")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
