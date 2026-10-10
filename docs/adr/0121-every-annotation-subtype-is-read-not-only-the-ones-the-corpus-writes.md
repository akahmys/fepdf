# ADR-0121: Every annotation subtype is read, not only the ones the corpus writes

- **Status**: Accepted
- **Date**: 2026-10-10
- **Commit**: (see the commit that adds this file)

## Context

`entries_read_for` in `fepdf-model/src/annotation.rs` gives the census each subtype's
entries, marked read or not. It had readers for the subtypes the corpus writes more than
once. It stopped there on the stated ground that "a sample of one is not a reason to
build a type". Eleven of Table 171's twenty-eight subtypes had no reader of their own
table: Text, FreeText, Line, Ink, Sound, Screen, PrinterMark, TrapNet, 3D, Projection and
RichMedia. Stamp, Redact, Polygon and Caret had readers for part of their tables.

That ground inverts AGENTS.md principle 3. A corpus can justify building something; only
a use case can justify not building it. The owner gave the use case on 2026-10-10: all
twenty-eight subtypes read and written.

## Decision

**Every subtype of Table 171 has a reader of its own table, entire.** Projection has
none, because 12.5.6.24 gives it no entries beyond the markup ones. Entries that hold
content this engine carries and does not interpret are read as `Object`: a 3D stream, a
RichMedia content dictionary, a sound, a measure.

## Consequences

- The census says "read" for every entry the standard defines on every subtype. "Not
  read" now means the entry is outside the standard, or is a key the census does not
  attribute to a table.
- `every_subtype_of_table_171_reads_its_own_entries` names one key of each subtype that
  no other table gives it, and fails when a reader is taken away.
