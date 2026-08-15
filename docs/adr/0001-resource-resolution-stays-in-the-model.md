# ADR-0001: Resource resolution stays in the model

- **Status**: Accepted, superseding an earlier layering
- **Date**: 2026-08-13
- **Commit**: `6fd3c94`

## Context

An earlier revision of `ARCHITECTURE.md` placed a `fepdf-resource` crate between the
document model and its consumers, to turn PDF resource dictionaries into usable
resources — font dictionary to `FontResource`, colour spaces, images. The crate was
created and then never adopted.

The reason it stalled was structural, not neglect. The layering assumed font
dictionaries are resolved **lazily**, when content is interpreted. This engine
resolves them **eagerly, during ingestion**: `Document` owns the font cache,
`ingest::discover_fonts` populates it, and the refinery normalises it, with
`document.rs` alone referring to the font module in 29 places. A crate above the
model cannot own that work without inverting a dependency ingestion genuinely needs,
so the copy could never become the live one.

By the time this was noticed the crate had sat unused long enough to accumulate a
partial, divergent copy of the model's font module: 12 public items against 46, with
the shared files differing line by line. Nothing referenced it; `fepdf-sdk` merely
re-exported it.

## Decision

Remove `fepdf-resource`. Resource resolution is part of the model, and the topology
says so.

The separation the crate was meant to provide already exists and is real: font
*programs* live in `fepdf-font`, which knows no PDF, and PDF *dictionaries* live in
the model. That boundary was verified by measurement — at the time, 3,547 of the 6,590 lines
under `font/` referenced no PDF type — and it holds.

## Consequences

- 878 lines of divergent duplicate removed. Deleting it changed nothing: tests,
  lints and the audit were unaffected, which is what "unused" means.
- The layer diagram now matches how the engine works rather than how a plausible
  decomposition would have it work.
- The same mistake was made a second time, in the same session, with the scope of
  `fepdf-syntax` — see ADR-0002. The pattern is: a decomposition that sounds right,
  drawn without measuring the coupling it assumes. Measuring first is cheap; the
  second time it was done before building rather than after.
