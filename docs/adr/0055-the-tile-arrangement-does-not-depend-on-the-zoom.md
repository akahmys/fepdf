# ADR-0055: The tile arrangement does not depend on the zoom, so zooming is a change of distance

- **Status**: Accepted
- **Date**: 2026-09-05
- **Commit**: (see the commit that adds this file)

## Context

The tile view took its column count from the viewport width divided by the zoom. Every zoom
step therefore reflowed the grid: pages moved between rows underneath the cursor, and what
was meant to be a change of scale was a rearrangement that happened to also change scale.
Zooming in on a page did not bring you closer to it — it put a different page where that
one had been.

`zoom_at` has anchored to the point under the cursor since ADR-0053, and it could not
help, because the point it anchored to had moved by the time the frame was drawn.

## Decision

**The column count is fixed** (`TILE_COLUMNS`, ten), so the arrangement lives in page space
and reads neither the zoom nor the window. Zooming is then what it looks like: moving
towards or away from one sheet of pages that is holding still.

The signature is the guarantee. `grid_rows` takes the sizes, the count and the gaps, and
there is nothing to pass it that zooming could change — a test states exactly that.

**Rows are further apart than columns** (112 against 48). With both at one value the grid
read as an even field with nothing for the eye to travel along, and the page numbers, which
sit in that space, had no room to be read as labels.

**The page number lost its frame.** A filled rounded rectangle with a border around a
two-digit number is a control the reader cannot press, and a grid of them reads as a row of
buttons between the rows of pages.

**A page selection is drawn only in the tile view.** The state survives into the page view —
selecting pages, zooming in to check one and zooming back out to act is the ordinary way to
use it — but marking a page there would mark something the reader can neither select,
deselect, nor delete.

## Consequences

The grid no longer fits itself to the window, which is what a fixed arrangement means: ten
A4 pages across is 6,382 page units, filling an ordinary viewport at 20% and overflowing it
at 25%. **That is where the zoom floor came from** — below 20% the grid only shrinks into
the middle of the screen (ADR-0053).

**The reach of the overview fell with it**, from the 253 pages measured at the old floor to
40 or 50. Seeing more at once is now a question for the column count, and ten is a guess:
it fits a laptop window at the floor and was not derived from anything else.

**Each part fails a test when removed**, verified by removing each in turn: a wrong column
count, a row that does not clear the tallest page in it, right-to-left ignored, the column
gap dropped, and the row gap set back to the column gap.

**Two things were fixed on the way and belong to neither decision.** egui's own keyboard
zoom was still enabled, so `Cmd -` both stepped the document zoom and shrank the whole
interface — and inflated the viewport measured in logical points, which is where
ADR-0054's unexplained 1,960 visible pages came from. And `compute_layouts` stopped reading
`viewport_w` and `zoom` at all, which the compiler reported as two unused variables: the
warning was the proof that the arrangement had come free of the zoom.
