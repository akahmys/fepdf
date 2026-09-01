#!/usr/bin/env python3
"""One page at a time: same characters, same order? Same characters, different order?

Called by `crosscheck_reading_order.sh`, which explains why the question is asked per
page rather than per document. Prints four numbers: pages compared, identical, order-only,
content-differs.
"""
import collections
import re
import sys


def read(path: str, marker: str) -> dict[int, str]:
    pages: dict[int, list[str]] = {}
    cur = None
    for line in open(path, encoding="utf-8", errors="replace"):
        m = re.match(marker, line.rstrip("\n"))
        if m:
            cur = int(m.group(1))
            pages[cur] = []
        elif cur is not None:
            pages[cur].append(line)
    return {k: "".join(v) for k, v in pages.items()}


def squeeze(s: str) -> str:
    """Whitespace out. The two readers disagree about spacing by design, and that is a
    different question from order."""
    return "".join(c for c in s if not c.isspace())


def main() -> int:
    ours = read(sys.argv[1], r"^--- \[ PAGE (\d+) \] ---$")
    theirs = read(sys.argv[2], "^\x01PAGE (\\d+)\x01$")
    identical = order = content = 0
    agreed = total = 0
    for n in sorted(set(ours) & set(theirs)):
        x, y = squeeze(ours[n]), squeeze(theirs[n])
        if x == y:
            identical += 1
        elif collections.Counter(x) == collections.Counter(y):
            order += 1
        else:
            content += 1
        # How far the two agree before they first part. **A page is a coarse unit.**
        # One misplaced running head keeps a page off the identical column however much
        # of it is right, which is how the ruby work of 2026-09-01 moved
        # `bokutokitan.pdf` page 11 from 59 agreeing characters to 332 of 389 and moved
        # no column at all. This is what sees that.
        prefix = 0
        for a, b in zip(x, y):
            if a != b:
                break
            prefix += 1
        agreed += prefix
        total += max(len(x), len(y))
    pct = (100 * agreed / total) if total else 0.0
    print(len(set(ours) & set(theirs)), identical, order, content, f"{pct:.1f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
