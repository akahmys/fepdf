# ADR-0088: What a crop puts outside the sheet is removed

- **Status**: Accepted
- **Date**: 2026-09-19
- **Commit**: (see the commit that adds this file)

## Context

`ResizePages` could already put a page on a smaller sheet. What hung over the edge stopped
being displayed, because `/CropBox` became the sheet and 14.11.2 makes that the region a
viewer shows — and it stayed in the file. The window said both things, in two sentences,
because only one of them is visible; `document_tools.rs` records the measurement beside
them: `print_sample.pdf` page 3 shifted 300 points right renders with its right-hand half
gone, and `extract_text` returns all 428 characters it did before.

As a description of a resize that was honest. As the foundation for **splitting one page
into several** — the operation JUST PDF calls ページの分割, and the one the comparison on
2026-09-19 found missing — it is not: half a drawing, still searchable, on a page that
shows the other half, is a leak dressed as a feature. A reader who cuts an A3 assembly
drawing into two A4 sheets to send one of them has sent both.

The machinery for removing it already existed and had a known shape. Physical redaction
walks the page's marks, finds the ones intersecting a rectangle, and rewrites the stream
([ADR-0064](0064-redaction-removed-the-second-run-of-a-page-and-no-other.md)). Two
properties of it decide the cost here:

- It works at the granularity of **one text-showing operator**. A run that straddles the
  boundary goes whole — and a line of text crossing the cut is the ordinary case for a
  page split, not the edge case.
- It **replaces** the string rather than deleting the run, writing `[REDACTED]`. That is
  right for redaction, where something was deliberately withheld, and wrong here, where
  the content simply belongs to the other sheet.

## Decision

Content that a crop or a split puts outside the sheet is removed from the file, not hidden
by `/CropBox`.

- Text runs are split **at a glyph**, which needs the advance widths that
  [ADR-0085](0085-editing-what-a-page-draws-is-in-scope.md)'s W-E4 builds.
- Images are re-encoded to the part that remains.
- Paths are clipped and rebuilt, rather than clipped for display.
- Nothing is substituted in place of what goes.

A crop that only moves `/CropBox` remains available and remains described the way it is
described now, for the reader who wants a view rather than a cut.

## Consequences

- **The check fails against today's behaviour on the day it is written**, which is why it
  is worth writing: `extract_text` on the cropped side returning a character that was cut
  away is the failure, and the resize above produces it 428 times.
- **It waits on font work**, like everything else in Phase W. Splitting a run without
  widths would leave the remaining glyphs at the wrong advances, which is a worse defect
  than the leak it replaces.
- **Redaction does not change.** Its granularity and its `[REDACTED]` substitution are
  right for what it does; this decision does not reach them, and ADR-0064 stands.
- **An image cut in half is re-encoded**, so the output is not byte-identical to the input
  for that object. Saving already produces a new document
  ([ADR-0012](0012-saving-produces-a-new-document.md)), so this costs nothing that was
  promised.
