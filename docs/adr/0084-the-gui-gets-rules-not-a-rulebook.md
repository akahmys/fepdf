# ADR-0084: The GUI gets rules, not a rulebook

- **Status**: Accepted
- **Date**: 2026-09-10
- **Commit**: (see the commit that adds this file)

## Context

`fepdf-gui` is 11,239 lines and the only frontend a person looks at. None of the four
phase documents said anything about it, and the first day anyone did look — running it and
screenshotting it rather than reading it — found two icons that drew nothing, a control
whose label was painted transparent, a save that emptied the window it had just written
from, and a first screen that was blank.

**This repository has twice written a document for that, and deleted both.**

* `.agents/rules/desktop-ui.md`, a four-section "UI Design Protocol", went with
  [ADR-0038](0038-one-hierarchy-of-truth-and-the-parallel-rulebook-is-deleted.md) for
  instructing the opposite of a live rule: its §1 required UI widgets to consume
  SDK-native handles, where Rule A stops arena types at the facade. Measured against the
  code on 2026-09-10, **none of its four sections held**: there is no dark mode for its
  HSL pairing to span, no transitions, neither of its two named fonts, no accessible name
  on any of the twenty-four icon buttons its §3 required identifiers for, and nothing had
  ever measured the contrast its §4 mandated.
* `docs/specs/app_design.md` went in the Phase O-4 audit for naming five crates and four
  types that do not exist and calling the CLI binary the GUI. Its §3 decided a
  **"Menu-less Workflow: Abandon large fixed menus in favor of a context-oriented UX"**,
  and that decision is the direct ancestor of the state found here: no menu bar, ten
  unlabelled icons, and seven of the GUI's twelve document operations reachable only
  through a command palette.

Both failed the same way, and `AGENTS.md` rule 4 names it: neither document said what
enforced any of it. ADR-0038 also observed that being labelled subordinate does not stop a
document from being applied — an agent read `desktop-ui.md` and reported it as a live
contradiction.

`docs/specs/` cannot be the home either. Its README opens **"Not authoritative"** and puts
itself at the bottom of the hierarchy, and the last GUI document to live there became
fiction.

## Decision

**No GUI document. Thirteen rules in `CODING.md` §3, each naming what enforces it, and
the principles they come from in the same section so the derivation is readable.**

A second table rather than rows in the RR-15 matrix: RR-15 is aerospace-derived safety and
these are not, and CODING.md already records that reassigning a rule number is what made
Rules 9 and 14 mean two things. The prefix is `UI-`.

Four principles, ordered, each answering "when both options are allowed, which?":

| | | rejects |
| :--- | :--- | :--- |
| **P1** | Make it reversible before you make it reachable | "a confirmation dialog makes it safe" |
| **P2** | Show it before you let it be edited | "the engine has an `apply`, so put it in the GUI" |
| **P3** | An entry point nobody can find is not an entry point | "menu-less", and a naive reading of Nielsen's minimalism |
| **P4** | Run the product's own checks over the product's own window | "the UI is outside the product" |

**When the two collide, operation beats decoration.** Discoverability is not traded for
simplicity — `app_design.md` made that trade and this is what it bought.

Three of the thirteen have a check: `scripts/audit/icon_glyphs.py` (UI-1),
`scripts/audit/palette.py` (UI-9), and `rustc` (UI-3, where a success and a failure are
different types). The other ten say "nothing", which rule 4 permits and rule 4 requires
to be visible.

## Consequences

- **P2 had already decided something before it was written.** Sorting the eighteen
  `Operation` variants the GUI cannot reach, it rejected `AddAnnotation` — the renderer
  draws no annotations, so one could be added and never seen or removed — and
  `UpdateOutlines`, where the reader returns counts and no tree; and it admitted
  `SetFormFieldValue`, where `InteractiveReport` already returns each field's name, type
  and value. A principle that decides nothing is decoration.
- **UI-6 and UI-7 enter unchecked, and they are the two that matter most.** Reversibility
  and progress are the highest-severity gaps found — `grep -rn "dirty\|unsaved\|on_close"`
  over the crate returns nothing — so the table opens with its weakest entries at its most
  important rows. Naming them beats the alternative, which is the state this replaces.
- **UI-8 shows what an unchecked rule costs.** `desktop-ui.md` §4 mandated 4.5:1 for text
  and nothing measured it. Measured now, every text pair passes — 7.58:1, 14.63:1, 7.38:1
  — and what fails is the boundary criterion it never named: a sheet met the canvas at
  1.09:1, a disabled control at 1.23:1. A rule with no checker does not merely go stale;
  it points somewhere else.
- **A fourth document about the GUI would now be the thing to refuse.** This record exists
  mostly so that the next person to feel the pull of one finds the two attempts and their
  measurements before writing a third.
