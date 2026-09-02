# ADR-0053: The viewer has two modes and a zoom ladder, because a multiplier could not reach 100%

- **Status**: Accepted
- **Date**: 2026-09-02
- **Commit**: (see the commit that adds this file)

## Context

Four things about zoom and layout were settled together because each was a consequence of
the others.

**The tile grid used page 1's size as the cell for every page.** `compute_layouts` took
`ref_w`/`ref_h` from `doc_page_sizes.first()` and used them for the column count, the pitch
and the centring. A page wider than `ref_w + 2 * gap_x` was therefore drawn on top of its
neighbour: an A3 landscape sheet in an A4 document overlapped the next page by **274pt, 46%
of its width**. The single-column and horizontal paths accumulate each page's own size and
were never wrong; only the multi-column path, which is the zoomed-out one.

**Neither corpus can say whether this matters.** No mixed-size document exists in either:
9 samples, and 515 external files of which **only 5 have more than one page**. The second
figure is why the first proves nothing — a conformance suite of single-page files cannot
disagree about how pages are arranged.

**Zoom was continuous, and its buttons multiplied.** `+` and `−` scaled the current zoom by
1.2, so from any value not already on a round number — anything a pinch or a fit had
produced — **no number of presses ever arrived at 100%**. A separate reset button existed to
paper over it. Worse, the label is `{:.0}%`, so 99.6% printed as `100%`: the view rendered
at one zoom and told the reader another.

**Nine call sites across three files tested `zoom < 0.65`** — the column count, the
double-click destination, the drag-pan gate, the page badge size, the page border, tile
drag-and-drop, the marquee, `Cmd+A`, and text selection. That number was the boundary
between two modes and had no name.

**Text selection was already off below 0.65**, not on everywhere as assumed when this work
was scoped. The request was to stop content interactions below roughly 40%; at 40% that
would have *loosened* the rule. What had no zoom gate at all was the reading-order overlay.

## Decision

**The two boundaries are named, and each answers one question.**
`PDFView::OVERVIEW_ZOOM` (0.65) is *how pages are arranged* — below it they tile, a drag
reorders them, `Cmd+A` takes all of them. `PDFView::LEGIBLE_ZOOM` (0.40) is *whether what is
drawn can be read* — below it nothing acts on the content of a page. Body text is set at 10
to 11pt, so at 40% it draws at a little over 4pt with an x-height near 2pt.

**Zoom moves along a ladder of 18 steps**, from 10% to 1000%. Buttons, keyboard and menu
step between them, so 100% is reachable from anywhere. Pinch and trackpad stay continuous
and snap when within 2% of a step. Fit-to-width and fit-to-height set what they need and do
not snap, because a fit that snapped would not fit.

**The mode boundaries sit between steps, never on one**: 33% and 50% straddle 40%, and 50%
and 67% straddle 65%. Stepping therefore never lands on a boundary and leaves the mode
undetermined. Double-clicking out goes to 33% rather than the old 35% for the same reason.

**The tile grid became a flow.** Each row takes as many pages as fit at the current zoom,
by their own widths; rows hold different numbers of pages; short pages are centred against
the tallest in their row. **On a document whose pages are all one size this is the previous
layout exactly**, which is every document in both corpora.

**Text selection stays at `OVERVIEW_ZOOM`** rather than being moved down to 40%, since
moving it would have enabled selection in the 40–65% band where it is currently off. The
40% gate went on the reading-order overlay, which had none.

## Consequences

A mixed-size document lays out without overlap. `+` from 83% reaches 100% in two presses.
The percentage in the status bar is the percentage in force whenever the view is on a step.

**The status bar names the mode** — reading or overview. Nothing else announces that
content interactions have stopped; that was asked for and is deliberate.

**The reading-order overlay off below 40% is also the largest saving where it matters
most.** It draws borders per node of every visible page, and the zoom floor is exactly
where the most pages are visible — 138 of `volvo_xc90.pdf` at 10% in one measured window.

**Snapping needed a second value to be correct.** Computing a gesture's next zoom from the
snapped one made every small delta land back inside the snap band, so a pinch reaching 100%
stuck there for good. `zoom_unsnapped` is the gesture's own memory; nothing renders from it.

**Each part fails a test when removed**, verified by removing each in turn: a fixed column
count (2 tests), no vertical centring (1), right-to-left ignored (1), no snap band (1),
snapping the accumulator (1), and multiplicative steps (2).

**What is still open** is the document whose pages are so large that fit-to-width wants a
zoom below the 0.1 floor and is silently clamped, and `/UserUnit` (Table 31), which is
unimplemented — a page declaring one is laid out at 1.0.
