# ADR-0106: An `impl` block is held to 800 lines

- **Status**: Accepted
- **Date**: 2026-10-03
- **Commit**: (see the commit that adds this file)

## Context

Rule 1 held a function to 50 lines and its type to nothing. On 2026-10-03 eight `impl`
blocks in production code ran past 800 lines: `FontResource` (2,570), `PdfWriter`
(2,440), `FontReconstructor` (1,372), `PdfDocument` (1,311), `FepdfApp` in
`view_panel.rs` (1,205), `Document` (1,177), `PDFView` (1,135) and `Sublimator` (815).
ROADMAP Y-7 split each into files of one subject, as moves, with the golden comparison
agreeing after each.

Y-7's entry closed with "nothing gates the 800". AGENTS.md's fourth writing rule says
what that is worth: a rule that is not checked is a comment, and eight blocks grew past
the line while no rule said where it was.

## Decision

Rule 1 has a second limit: **no `impl` block in production code runs past 800 lines**,
counted from the `impl` line to its closing brace, comments and blank lines included.
`scripts/audit/impl_length.py` measures it and `verify_compliance.sh` fails on any block
over. A block inside a `#[cfg(test)]` module, and anything under `tests/`, `examples/` or
`benches/`, is not counted.

The count includes comments because what the limit is for is what a reader scrolls, and
this codebase's doc comments are a large share of it. 800 is the line Y-7 was written
against, and the largest block after it is 701.

## Consequences

- A type that grows past 800 is split by subject into child modules holding further
  `impl` blocks, as Y-7 did; its private methods become `pub(super)`.
- The check was shown to fire: 600 comment lines appended to `view/anchoring.rs`'s block
  read 858 lines and failed it.
