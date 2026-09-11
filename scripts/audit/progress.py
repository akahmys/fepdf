#!/usr/bin/env python3
"""UI-7: work the reader waits for says that it is happening.

Undo was the first thing in this window to say it was running, and it is the one that
needed saying least: a save writes a whole document, a `PDF/UA` audit walks the structure
tree, and both were silent. The window looked like it had ignored the click.

The worker names the work and the window says it — `WorkerResponse::Busy { key }` and
`Idle` — because this thread holds the document and not the reader's language.

Every arm of `WorkerRequest` sends `Busy`, or is named below with what it reports through
instead:

  * `Open` — `LoadingProgress`, which carries the engine's own stages;
  * `RenderPage` — the page draws a placeholder card saying which page it is;
  * `SetLayerVisible` — a toggle, and the redraw is the answer;
  * `RemovePages`, `DuplicatePage`, `ReorderPagesBatch`, `RotatePages` — the page grid
    changes under the reader in the same frame.

Exits non-zero with a line per silent arm. No arguments.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKER = ROOT / "crates/fepdf-gui/src/worker.rs"

REPORTS_OTHERWISE = {
    "Open",
    "RenderPage",
    "SetLayerVisible",
    "RemovePages",
    "DuplicatePage",
    "ReorderPagesBatch",
    "RotatePages",
}


def variants(text: str) -> list[str]:
    body = text.split("pub enum WorkerRequest {")[1].split("\n}")[0]
    return re.findall(r"^    (\w+)", body, re.M)


def arm_text(text: str, name: str) -> str:
    """Everything from this arm's head to the next arm's, or the end of the match."""
    start = text.find(f"WorkerRequest::{name}", text.index("for request in rx"))
    if start < 0:
        return ""
    nxt = text.find("            WorkerRequest::", start + 1)
    return text[start : nxt if nxt > 0 else start + 4000]


def main() -> int:
    text = WORKER.read_text()
    failures: list[str] = []
    saying = 0

    for name in variants(text):
        if name in REPORTS_OTHERWISE:
            continue
        if "WorkerResponse::Busy" in arm_text(text, name):
            saying += 1
        else:
            failures.append(f"worker.rs: `{name}` runs without saying so, and is not named as quick")

    for line in failures:
        print(line)
    print(
        f"UI-7: {saying} arms report, {len(REPORTS_OTHERWISE)} named as reporting otherwise, "
        f"{len(failures)} silent"
    )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
