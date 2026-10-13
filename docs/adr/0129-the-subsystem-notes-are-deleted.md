# ADR-0129: The subsystem notes are deleted

- **Status**: Accepted
- **Date**: 2026-10-13
- **Commit**: (see the commit that adds this file)

## Context

`docs/specs/` held four notes: `core-pipeline.md`, `refinery_engine.md`, `rendering.md`
and `sdk-pipeline.md`. Their own index called them "not authoritative" and said they
predated the current design. Two audits on 2026-08-22 corrected them in place, and the
corrections were written into the text, so most claims were followed by a dated paragraph
saying they were wrong. By 2026-10-13 the corrections themselves were stale.
`rendering.md` gave sixteen `log::warn!` sites where the engine keeps two.

`AGENTS.md` ranks them last in the hierarchy of truth, below `ARCHITECTURE.md`, which says
what the design is, and below `docs/adr/`, which says how it came to be. A document that
the two above it always override, and that its own readers are told not to trust, has no
question left to answer.

## Decision

The four notes are deleted, together with the half of `docs/specs/README.md` that
recorded their audits. The README keeps the list of standards a working copy needs, and a
pointer to `pdf-2-0-deprecations.md`, which is current and which `ARCHITECTURE.md` cites.
This is the same move [ADR-0038](0038-one-hierarchy-of-truth-and-the-parallel-rulebook-is-deleted.md)
made for `docs/history/`: git holds what they said.

## Consequences

- `AGENTS.md`'s fifth rank is now the deprecation table alone.
- A comment in `fepdf-model/src/arena.rs` that cited the README's record of
  `refinery_engine.md` now states the fact without the citation.
