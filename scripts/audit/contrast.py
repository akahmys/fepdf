#!/usr/bin/env python3
"""UI-8: body text reaches 4.5:1 and a meaningful boundary reaches 3:1.

`.agents/rules/desktop-ui.md` mandated 4.5:1 for text and nothing checked it, which is
the shape ADR-0084 records. Measured when the palette was rewritten, every text pair
already passed — 7.58:1, 14.63:1, 7.38:1 — and what failed was the criterion that document
never named: a sheet met the canvas at 1.09:1, a border at 1.48:1, a disabled control at
1.23:1. A rule with no checker does not merely go stale; it points somewhere else.

The pairs are derived rather than listed. Every foreground role must clear its threshold
against every surface a widget can sit on, so a colour that is lightened later fails here
rather than in front of a reader:

  * text — `steel::TEXT`, `steel::MUTED`, `rust::ACCENT`, every `note::*` — 4.5:1
    (WCAG 1.4.3);
  * boundaries that carry meaning — `steel::EDGE` — 3:1 (WCAG 1.4.11);
  * `steel::RULE` is decoration and is deliberately below both. Nothing may depend on
    seeing it, which is a claim about its use rather than its value, so it is named here
    and not measured.

Exits non-zero with a line per pair that falls short. No arguments.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
THEME = ROOT / "crates/fepdf-gui/src/app/theme.rs"

TEXT_MIN = 4.5
BOUNDARY_MIN = 3.0
# Decoration: named, and not measured. See the module note.
UNMEASURED = {"steel::RULE"}

CONST = re.compile(
    r"pub mod (\w+)\s*\{|pub const (\w+): Color32 = Color32::from_rgb\((\d+), (\d+), (\d+)\)"
)


def palette() -> dict[str, tuple[int, int, int]]:
    """Every colour in `theme::colors`, keyed `family::NAME`."""
    out: dict[str, tuple[int, int, int]] = {}
    family = ""
    for match in CONST.finditer(THEME.read_text()):
        if match.group(1):
            family = match.group(1)
        elif family:
            out[f"{family}::{match.group(2)}"] = (
                int(match.group(3)),
                int(match.group(4)),
                int(match.group(5)),
            )
    return out


def luminance(rgb: tuple[int, int, int]) -> float:
    def channel(value: int) -> float:
        v = value / 255
        return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4

    r, g, b = (channel(c) for c in rgb)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def ratio(a: tuple[int, int, int], b: tuple[int, int, int]) -> float:
    la, lb = luminance(a), luminance(b)
    high, low = max(la, lb), min(la, lb)
    return (high + 0.05) / (low + 0.05)


def main() -> int:
    colours = palette()
    surfaces = {n: c for n, c in colours.items() if n.startswith("paper::")}
    if not surfaces:
        print("no paper surfaces found — has theme.rs moved?", file=sys.stderr)
        return 1

    failures: list[str] = []
    checked = 0
    for name, colour in colours.items():
        if name.startswith("paper::") or name in UNMEASURED:
            continue
        need = BOUNDARY_MIN if name == "steel::EDGE" else TEXT_MIN
        for surface, ground in surfaces.items():
            checked += 1
            got = ratio(colour, ground)
            if got < need:
                failures.append(f"{name} on {surface}: {got:.2f}:1, needs {need}:1")

    for line in failures:
        print(line)
    print(
        f"UI-8: {checked} pairs checked, {len(failures)} below threshold, "
        f"{len(UNMEASURED)} named as decoration"
    )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
