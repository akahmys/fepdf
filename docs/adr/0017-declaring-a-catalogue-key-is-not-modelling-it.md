# ADR-0017: Declaring a catalogue key is not modelling it

- **Status**: Accepted
- **Date**: 2026-08-18
- **Commit**: this record

## Context

`inspect catalog` exists to show gaps. Its module doc opens "The point is the gaps", and
`ROADMAP.md` quotes its figure as the measure of how much of clause 7.7.2 the engine
understands.

In one session that figure went from **15 of 32** to **32 of 32**, and `inspect catalog`
began reporting `untyped: 0` on every file in the corpus. A tool built to make gaps
visible was showing none.

The change behind it was 17 new fields on `PdfCatalog`, of which 16 are `Option<Object>`
and one — `needs_rendering: Option<bool>` — describes what it holds. `Option<Object>`
returns whatever the arena already had; the entry becomes reachable by name and its
contents stay exactly as opaque as before the field existed. Counted properly, the
engine went from 5 entries whose contents it could read to 6.

Six of the new fields are `PageLabels`, `Threads`, `OutputIntents`, `OCProperties`,
`Collection` and `AF` — the entries `Support::TypeOnly` was invented to mark, meaning a
spec type exists and nothing reads the key. Giving them `Option<Object>` fields moved
them out of that category without giving them readers, so the count of known-hollow
entries fell to zero while nothing about them changed. Three of them — `DSS`, `AF`,
`DPartRoot` — were measured at zero occurrences across the corpus and were named in
Phase D as the ones *not* to type first, for exactly this reason.

The caveat that had been written into `ROADMAP.md` when the figure was 15 — that the
entries were not 15 domain types — was deleted rather than updated, leaving a clean
"32 of Table 29's 32".

None of this was wrong to build. A named field is better than walking the raw dictionary,
and it is how a reader finds the entry at all. What was wrong was that one number counted
both achievements, and the number was the one the documents quoted.

## Decision

**`Support` distinguishes `Modelled` from `Declared`.**

- `Modelled` — the field's type says what the entry holds: `Option<ViewerPreferences>`,
  `Option<DestsDictionary>`, `Option<PageMode>`, `Option<String>`, `Option<bool>`.
- `Declared` — the field is `Object` or a bare arena handle. Reachable, contents opaque.

The distinction is derived, not listed. `PdfSchema` gains `pdf_key_types()`, which the
derive macro fills with each key's field type as written, and `catalog.rs` classifies
against `PASSTHROUGH_TYPES` — the arena's own types, a closed set belonging to this
crate. Nobody has to remember to update a list when a field changes type.

`pdf_key_types()` has **no default implementation**. One returning `&[]` would classify
every key of an un-updated type as unmodelled, in silence, which is the failure this
whole record is about.

`CatalogReport::untyped` becomes `unmodelled`, and counts `Declared` entries: the method
is about whether the value is legible, and a field does not make it so.

## Consequences

`inspect catalog` reports four levels and shows gaps again — `intel_sdm.pdf` reads
2 modelled, 9 declared, and names the nine. `status.sh` prints "of which model their
contents" beneath the headline count, so the figure carries its own caveat rather than
depending on a paragraph elsewhere staying current.

The ROADMAP's 7.7 row states both numbers and why there are two.

**This is the third time this shape has been fixed here.** An empty `/ViewerPreferences`
read identically to five deliberate `false`s until every field became `Option`;
`dangling.len()` was added to a count of uses until the two were separated; and
`status.sh` reported 0 subcommands against a truth of 8 because a moved anchor and a
genuine zero were indistinguishable. Each time the defect is a category collapsed into a
number that then reads as progress. It is worth naming as a class, because the next one
will not look like the last three.

What this does not do is make the goal measurable. Six of 32 modelled is a better figure
than 32 of 32; it is still a figure about this engine's own struct, measured against a
corpus of nine files this project chose.
