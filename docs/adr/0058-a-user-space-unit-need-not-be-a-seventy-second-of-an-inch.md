# ADR-0058: A user space unit need not be a seventy-second of an inch

- **Status**: Accepted
- **Date**: 2026-09-05
- **Commit**: (see the commit that adds this file)

## Context

`/UserUnit` (14.11.2, Table 31) gives the size of a page's default user space unit in
multiples of 1/72 inch. **The engine had no notion of it.** The string did not appear
anywhere in the workspace, so every page was drawn as though its unit were a point.

**It is how a drawing exceeds the limit a box can express.** A `/MediaBox` entry is a
number, and the implementation limit on one is 14,400 units — 200 inches. A survey plan or
a plotter sheet larger than that says so by declaring a `/UserUnit`, leaving the
coordinates as written and making each one worth more. A reader that ignores it renders
such a page at a fraction of the size the document asks for.

**One file in 524 declares one**, and it is in `pdf-differences` — a corpus assembled to
expose readers disagreeing with one another. `LineCap-Degenerate.pdf` is
`/MediaBox [0 0 400 400]` with `/UserUnit 10`: 55 inches square, not 5.5.

## Decision

**`PdfDocument::get_page_user_unit` reads the page's own entry**, and rendering multiplies
its scale by it. Nothing else changes: `/UserUnit` does not alter the coordinate system,
only what a coordinate is worth, so the content is drawn exactly as before at a different
scale.

**Not inherited.** Table 31 lists it in the page dictionary and Table 30 does not list it
among the inheritable attributes, so this reads the page and does not walk up the tree —
unlike `/MediaBox` or `/Rotate`, which do.

**A value that is absent, unreadable or not positive gives 1.0**, which Table 31 makes the
default. A zero or a negative would scale a page to nothing or turn it inside out.

## Consequences

`LineCap-Degenerate.pdf` renders at 5,333 by 5,333 at 96 DPI, where it was 533 by 533.

**Two things surfaced while verifying it, and neither is fixed here.**

The CPU rasteriser panics on that page: `vello_shaders coarse.rs:213`, *"index out of
bounds: the len is 256 but the index is 256"*. The GPU path renders it. A normal page
renders on the CPU, so the trigger is the size — the same shape as
[ADR-0052](0052-the-scene-budget-is-counted-before-the-scene-is-submitted.md), a fixed
buffer in vello with nothing checking against it, in a different pass.

And the CLI's progress line had its arguments the wrong way round, printing *"Rendering
page 1 of <output> to <input>"*. Corrected.

**What is not done is everything but rendering.** `get_page_size` still answers in
coordinate units, which is what a caller laying out content wants; whether an interface
asking for a *physical* size should multiply is a question this record does not settle,
and no caller asks for one today.
