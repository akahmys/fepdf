#!/usr/bin/env python3
"""UI-1: every icon codepoint resolves to a glyph that draws.

Two ways an icon button can be blank, and this crate has had both:

  * the codepoint is past the end of the font — `U+E8E8`, where `lucide.ttf` stops at
    `U+E6FD`, drew tofu in the rail for as long as the drawer existed;
  * the codepoint is answered by a *different* font — `U+E0FF` is the Ubuntu logo in
    egui's own `Ubuntu-Light`, which sat ahead of the icon font in the proportional
    family, so the continuous-scroll button drew nothing.

The second is why this also fails on an icon codepoint written anywhere but
`app/icons.rs`: a glyph in a proportional string is looked up in the proportional
family, where the icon font is not.

Exits non-zero with a line per failure. No arguments.
"""

from __future__ import annotations

import re
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GUI = ROOT / "crates/fepdf-gui/src"
ICONS = GUI / "app/icons.rs"
FONT = ROOT / "crates/fepdf-gui/assets/lucide.ttf"

# The private-use range Lucide occupies. `U+0078` is the letter x, which Lucide also
# maps; it is reached the same way and is checked the same way.
CODEPOINT = re.compile(r"\\u\{([0-9a-fA-F]{4})\}")


def tables(data: bytes) -> dict[str, tuple[int, int]]:
    count = struct.unpack(">H", data[4:6])[0]
    out = {}
    for i in range(count):
        rec = 12 + 16 * i
        tag = data[rec : rec + 4].decode("latin1")
        out[tag] = struct.unpack(">II", data[rec + 8 : rec + 16])
    return out


def cmap_format4(data: bytes, tabs: dict[str, tuple[int, int]]) -> int | None:
    base = tabs["cmap"][0]
    chosen = None
    for i in range(struct.unpack(">H", data[base + 2 : base + 4])[0]):
        rec = base + 4 + 8 * i
        offset = struct.unpack(">HHI", data[rec : rec + 8])[2]
        if struct.unpack(">H", data[base + offset : base + offset + 2])[0] == 4:
            chosen = base + offset
    return chosen


def glyph_for(data: bytes, sub: int, code: int) -> int:
    seg_x2 = struct.unpack(">H", data[sub + 6 : sub + 8])[0]
    segments = seg_x2 // 2
    ends, starts = sub + 14, sub + 14 + seg_x2 + 2
    deltas, ranges = starts + seg_x2, starts + 2 * seg_x2
    for s in range(segments):
        end = struct.unpack(">H", data[ends + 2 * s : ends + 2 * s + 2])[0]
        if code > end:
            continue
        start = struct.unpack(">H", data[starts + 2 * s : starts + 2 * s + 2])[0]
        if code < start:
            return 0
        delta = struct.unpack(">h", data[deltas + 2 * s : deltas + 2 * s + 2])[0]
        offset = struct.unpack(">H", data[ranges + 2 * s : ranges + 2 * s + 2])[0]
        if offset == 0:
            return (code + delta) & 0xFFFF
        at = ranges + 2 * s + offset + 2 * (code - start)
        gid = struct.unpack(">H", data[at : at + 2])[0]
        return 0 if gid == 0 else (gid + delta) & 0xFFFF
    return 0


def outline_bytes(data: bytes, tabs: dict[str, tuple[int, int]], gid: int) -> int:
    head = tabs["head"][0]
    long_loca = struct.unpack(">h", data[head + 50 : head + 52])[0]
    loca = tabs["loca"][0]
    if long_loca:
        a, b = (struct.unpack(">I", data[loca + 4 * g : loca + 4 * g + 4])[0] for g in (gid, gid + 1))
    else:
        a, b = (struct.unpack(">H", data[loca + 2 * g : loca + 2 * g + 2])[0] * 2 for g in (gid, gid + 1))
    return b - a


def main() -> int:
    failures: list[str] = []

    # Every icon codepoint is declared in one file, so that it is drawn through the
    # icon family and cannot be answered by a text font.
    for path in sorted(GUI.rglob("*.rs")):
        if path == ICONS:
            continue
        for line_no, line in enumerate(path.read_text().splitlines(), 1):
            for match in CODEPOINT.finditer(line):
                code = int(match.group(1), 16)
                if 0xE000 <= code <= 0xF8FF:
                    rel = path.relative_to(ROOT)
                    failures.append(
                        f"{rel}:{line_no}: U+{code:04X} outside app/icons.rs — a glyph in a "
                        f"proportional string is looked up in the proportional family"
                    )

    data = FONT.read_bytes()
    tabs = tables(data)
    sub = cmap_format4(data, tabs)
    if sub is None:
        print("lucide.ttf has no format-4 cmap", file=sys.stderr)
        return 1

    declared = 0
    for line_no, line in enumerate(ICONS.read_text().splitlines(), 1):
        match = CODEPOINT.search(line)
        if not match or "pub const" not in line:
            continue
        declared += 1
        code = int(match.group(1), 16)
        name = line.split("pub const ")[1].split(":")[0]
        gid = glyph_for(data, sub, code)
        if gid == 0:
            failures.append(f"app/icons.rs:{line_no}: {name} U+{code:04X} is not in lucide.ttf")
        elif outline_bytes(data, tabs, gid) == 0:
            failures.append(
                f"app/icons.rs:{line_no}: {name} U+{code:04X} is an empty glyph (gid {gid})"
            )

    for line in failures:
        print(line)
    print(f"UI-1: {declared} icon codepoints checked, {len(failures)} failing")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
