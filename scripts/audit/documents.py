#!/usr/bin/env python3
"""What the documents must satisfy, since `AGENTS.md`'s writing rules had nothing behind them.

`AGENTS.md` states five rules for writing the documentation and names an enforcer for none
of them. Its own fourth rule says what that means: **a rule that is not checked is a
comment.** Three of the five were broken between 2026-08-29 and 2026-09-07 —
[ADR-0039](../../docs/adr/0039-the-design-document-was-narrating-its-own-corrections.md)
cut 2,200 words of past-tense self-correction out of `ARCHITECTURE.md`, and nine days later
the same shape was back, one paragraph of it added by the agent doing the cleaning.

Three things are checkable and each one caught a real violation on the day it was written:

1. **Tense.** `ARCHITECTURE.md` and `AGENTS.md` say what is true now; `docs/adr/` says how
   it came to be. A dated self-correction — a line carrying both a date and a past-tense
   verb — is the shape that keeps coming back, and there were eight.
2. **Links.** Every relative link in the documents and in source doc comments must resolve.
   Seven of the twenty-two in source pointed nowhere, all by miscounting `../`.
3. **The ADR index.** No duplicate or missing number, index and directory agreeing in both
   directions, every record carrying a Status and a Date.

What this cannot check is whether a quoted figure is current, which is the fifth rule and
the one that cost the most: `TESTING.md` stood eight days at 4× wrong. A number's freshness
is not a property of its text. What the rule asks for instead is a *derivation beside the
number*, and that is a habit rather than a pattern.
"""

import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent

# The documents that answer "what is true now". `AGENTS.md`: "Present tense here and in
# `ARCHITECTURE.md`; past tense in `docs/adr/`."
PRESENT_TENSE = ["ARCHITECTURE.md", "AGENTS.md"]

DATE = re.compile(r"\b20\d\d-\d\d-\d\d\b")
PAST = re.compile(
    r"\b(was|were|had|used to|became|grew|stopped|turned out|came to|"
    r"no longer|once read|is deleted|were deleted|is fiction|was fiction)\b"
)

# Dated lines that are not self-correction. Each says *when a measurement was taken*,
# which rule 3 requires of a quoted figure — the opposite of the thing rule 2 forbids.
DATED_MEASUREMENTS = {
    ("ARCHITECTURE.md", "re-derived 2026-09-07"),
}


def tense() -> list[str]:
    out = []
    for name in PRESENT_TENSE:
        path = ROOT / name
        fence = False
        for number, line in enumerate(path.read_text().split("\n"), 1):
            if line.strip().startswith("```"):
                fence = not fence
                continue
            if fence or not DATE.search(line) or not PAST.search(line):
                continue
            if any(name == doc and marker in line for doc, marker in DATED_MEASUREMENTS):
                continue
            out.append(f"  {name}:{number}: dated past tense — {line.strip()[:80]}")
    return out


def links() -> list[str]:
    out = []
    pattern = re.compile(r"\]\(([^)#][^)]*\.(?:md|rs|sh|py|toml|json))\)")
    docs = list(ROOT.glob("*.md")) + list((ROOT / "docs").rglob("*.md"))
    sources = [p for p in (ROOT / "crates").rglob("*.rs") if "target" not in p.parts]
    for path in docs + sources:
        for match in pattern.finditer(path.read_text(errors="replace")):
            target = (path.parent / match.group(1)).resolve()
            if not target.exists():
                out.append(f"  {path.relative_to(ROOT)}: {match.group(1)} does not exist")
    return out


def adr_index() -> list[str]:
    out = []
    directory = ROOT / "docs" / "adr"
    files = sorted(f for f in directory.glob("*.md") if f.name != "README.md")
    numbers = [int(f.name[:4]) for f in files]

    for number in set(numbers):
        if numbers.count(number) > 1:
            out.append(f"  ADR-{number:04d} is used by more than one file")
    for number in range(1, max(numbers) + 1):
        if number not in numbers:
            out.append(f"  ADR-{number:04d} is missing; the numbering has a gap")

    readme = (directory / "README.md").read_text()
    indexed = set(re.findall(r"\]\((\d{4}-[a-z0-9-]+\.md)\)", readme))
    on_disk = {f.name for f in files}
    for name in sorted(indexed - on_disk):
        out.append(f"  the index names {name}, which is not there")
    for name in sorted(on_disk - indexed):
        out.append(f"  {name} is not in the index")

    for f in files:
        head = f.read_text().split("## ")[0]
        for field in ("**Status**", "**Date**"):
            if field not in head:
                out.append(f"  {f.name} has no {field}")
    return out


def main() -> int:
    failures = []
    for label, check in (("tense", tense), ("links", links), ("ADR index", adr_index)):
        found = check()
        if found:
            failures.append(f"  FAIL ({label}):")
            failures.extend(found)
    if failures:
        print("\n".join(failures))
        return 1
    print("  PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
