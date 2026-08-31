# ADR-0047: Text extraction reconstructs logical reading order

- **Status**: Accepted
- **Date**: 2026-08-31
- **Commit**: (see the commit that adds this file)

## Context

As measured in [ROADMAP.md](../../ROADMAP.md) §Phase T, `TextExtractionBackend` previously appended
characters to an output string incrementally upon receiving `show_text` operators in content-stream
order. However, PDF producer software regularly emits text operators out of reading order (such as
footer page numbers drawn first at `y ≈ 50pt`, running headers emitted out of order, or reordered
body fragments). Across 7,727 pages of the 9 sample PDFs, 7,093 pages (92%) exhibited order-only
differences against reference readers like PDFKit. Furthermore, vertical writing modes
(`TextState.is_vertical == true`) were ignored.

## Decision

1. **Intermediate Positioned Run Collection**: `TextExtractionBackend` now collects `ExtractedRun`
   records containing page-space coordinates `(x, y)`, size, scale, advance width, `is_vertical`, and
   original stream `op_index`. `/ActualText` sections attach replacement text to their respective
   measured coordinate runs.
2. **Horizontal Reading Order Reconstruction**: For horizontal pages, runs are clustered into lines
   by baseline/center `y` proximity and sorted top-to-bottom (`y` descending). Within each line, runs
   are sorted left-to-right (`x` ascending). The 0.25 em kerning-versus-space threshold is preserved
   along each line.
3. **Vertical Writing Mode Support (Tatechugaki)**: For vertical pages (e.g. `bokutokitan.pdf`), runs
   are clustered into vertical columns and sorted right-to-left (`x` descending). Within each column,
   runs are sorted top-to-bottom (`y` descending).

## Consequences

- **Logical Reading Order Established**: Footer page numbers, headers, and out-of-order text fragments
  now appear in their natural reading positions (e.g. `constitution.pdf` page 3 footer page number `3`
  is placed at the bottom of the extracted page rather than the top).
- **Vertical Japanese Preserved**: Vertical text flow in Japanese literature is preserved.
- **Compliance & Tests**: All 17 text extraction tests and full workspace test suites pass.
