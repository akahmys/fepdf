# ADR-0111: A redaction keeps the remaining glyphs where they were

- **Status**: Accepted
- **Date**: 2026-10-04
- **Commit**: (see the commit that adds this file)

## Context

`Operation::Redact` removes each glyph that meets a region. It rewrites the run as the
glyphs that remain, with `TJ` offsets standing for the ones that went, so the glyphs after
a removed one are drawn exactly where they were (ROADMAP Y-10).

Keeping those positions keeps the width of what was removed. The offset is that width, to
a thousandth of an em. The font's `/Widths`, and `Tc`, `Tw` and `Tz`, stay in the file for
the text that remains. A reader with a list of candidates can set each one in the same
font and spacing, and compare its width with the gap. Bland, Iyer and Levchenko, "Story
Beyond the Eye: Glyph Positions Break PDF Text Redaction" (arXiv 2206.02285), recover
redacted words from Acrobat's output this way. Writing the following glyphs as a separate
run placed by coordinates keeps the same width, because the next glyph still starts where
it did.

Two answers were put to the owner on 2026-10-04:

1. Move the glyphs after a region on the same line so that they start at the region's
   right edge. The gap is then the region's width, which the fill already shows, and the
   rest of the line moves by the margin the region left.
2. Keep every remaining glyph exactly where it was. Nothing on the page moves, and the
   width stays recoverable, as it does from Acrobat.

## Decision

**The remaining glyphs keep their places.** The owner chose 2, so that a redaction moves
nothing it did not remove.

## Consequences

- A redacted run's width can be recovered from the file, and so can a list of candidates
  matching it. A caller who needs the width hidden as well redacts the whole line, or
  through to the end of what can be identified.
- `redaction_test.rs`'s `the_glyphs_outside_keep_their_places` holds the positions, and is
  the test a change of this decision would change.
