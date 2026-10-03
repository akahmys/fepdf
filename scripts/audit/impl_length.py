#!/usr/bin/env python3
"""Rule 1's second limit: no `impl` block in production code runs past 800 lines.

A function is held to 50 lines and its type was held to nothing. On 2026-10-03 eight
`impl` blocks ran past 800, the largest `FontResource` at 2,570 lines: a file nobody reads
the middle of, and a type whose subjects could not be told apart by where they lived.
ROADMAP Y-7 split them by subject into files of their own
([ADR-0106](../../docs/adr/0106-an-impl-block-is-held-to-800-lines.md)).

Counted the way `wc -l` would: from the `impl` line to its closing brace, comments and
blank lines included, because what is measured is what a reader scrolls. Only a block
that opens at the start of a line is counted, which leaves out the ones inside
`#[cfg(test)]` modules, and `tests/`, `examples/` and `benches/` are not production code.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LIMIT = 800
OPENS = re.compile(r"(pub(\([a-z]+\))? )?impl\b")


def blocks(path: Path) -> list[tuple[int, int, str]]:
    """Each top-level `impl` block of `path`: its first line, its length, its head."""
    lines = path.read_text(errors="ignore").split("\n")
    out, i = [], 0
    while i < len(lines):
        if OPENS.match(lines[i]):
            depth, j, started = 0, i, False
            while j < len(lines):
                code = re.sub(r"//.*", "", lines[j])
                code = re.sub(r'"(\\.|[^"\\])*"', '""', code)
                code = re.sub(r"'(\\.|[^'\\])'", "''", code)
                depth += code.count("{") - code.count("}")
                started = started or "{" in code
                if started and depth <= 0:
                    break
                j += 1
            out.append((i + 1, j - i + 1, lines[i].strip()))
            i = j
        i += 1
    return out


def main() -> int:
    over = []
    for path in sorted((ROOT / "crates").glob("*/src/**/*.rs")):
        for first, length, head in blocks(path):
            if length > LIMIT:
                where = path.relative_to(ROOT)
                over.append(f"  {where}:{first} ({length} lines, limit {LIMIT}) {head}")
    print(f"impl blocks over {LIMIT} lines: {len(over)}")
    for line in over:
        print(line)
    if over:
        return 1
    print("  PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
