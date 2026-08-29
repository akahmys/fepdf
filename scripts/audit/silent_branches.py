#!/usr/bin/env python3
"""Wildcard arms that answer a file-supplied value with silence (Rule 20's blind spot).

Rule 5 forbids a `_ =>` over a domain enum, and `clippy::wildcard_enum_match_arm`
enforces it. A PDF's domain values do not arrive as enums: `/ShadingType`, `/V`, `/LC`
and `/LJ` are integers read out of a file, so a `match` on one *needs* a wildcard and the
lint cannot see it. Rule 20 is what covers that ground — record a `Decision` naming the
clause — and nothing checks Rule 20.

This counts the arms where an unrecognised value produces neither a `Decision` nor an
error: a default is substituted, or `None` is returned, and the caller cannot tell that
the file said something this engine did not understand.

**A count, not a verdict.** Some of these are defensible — an unknown `/V` makes the
document fail to open, which is loud enough. The number is here so that a new one is
visible, not so that zero is the goal.
"""

import re
import sys
from pathlib import Path

SILENT = re.compile(r'\s*(return\s+)?(None|\(\)|\{\s*\}|Self::\w+|0|false|"")\s*[,}]')
LOUD = re.compile(r'\brecord\b|Decision|Err\(|panic|unreachable|todo!')


def arms(path: Path):
    """Every `match` on something with numeric arms, and the text of its wildcard."""
    lines = path.read_text().split("\n")
    for i, line in enumerate(lines):
        head = re.search(r'\bmatch\s+([A-Za-z_][A-Za-z0-9_.()\[\]]*)\s*\{\s*$', line)
        if not head:
            continue
        depth, body = 0, []
        for j in range(i, min(i + 400, len(lines))):
            depth += lines[j].count("{") - lines[j].count("}")
            body.append(lines[j])
            if depth <= 0 and j > i:
                break
        block = "\n".join(body)
        if not re.search(r'^\s*\d+\s*=>', block, re.M):
            continue
        wild = re.search(r'^\s*_\s*=>(.*?)$', block, re.M | re.S)
        if wild:
            yield i + 1, head.group(1), wild.group(1)[:160]


def main() -> int:
    found = []
    for path in sorted(Path("crates").glob("*/src/**/*.rs")):
        for line, scrutinee, arm in arms(path):
            if LOUD.search(arm) or not SILENT.match(arm):
                continue
            found.append((path, line, scrutinee, arm.strip()[:40]))
    for path, line, scrutinee, arm in found:
        print(f"{path}:{line}  match {scrutinee}  _ => {arm}")
    print(f"{len(found)} silent wildcard arms over a numeric domain value")
    return 0


if __name__ == "__main__":
    sys.exit(main())
