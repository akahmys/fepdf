# ADR-0005: The layering rules are enforced by Cargo, not by review

- **Status**: Accepted
- **Date**: 2026-08-13
- **Commit**: `6c813df`

## Context

`ARCHITECTURE.md` §7 claimed the layering rules were "enforced by Cargo itself — a
violation fails to compile". They were not. All four frontends declared `fepdf-core`
directly:

```
fepdf-cli    fepdf-sdk fepdf-core
fepdf-gui    fepdf-core fepdf-render fepdf-sdk
fepdf-mcp    fepdf-sdk fepdf-core
fepdf-wasm   fepdf-sdk fepdf-core
```

Rule A — storage abstractions stop at the facade — held only because the code
happened not to reach past it. Counting zero `PdfArena` references in a frontend was
being reported as though it proved something structural; it proved only that nobody
had done it yet.

What the frontends actually used from the model was small and already
facade-shaped: ingestion options, `FontResource`, `FontSummary`, `MetadataInfo`,
`graphics::Rect`, and the document-extension spec types. `fepdf-mcp` and `fepdf-wasm`
used nothing at all — their dependency was dead.

## Decision

Re-export what frontends legitimately need through `fepdf-sdk`, and remove
`fepdf-core` from all four manifests. A model type can then not be *named* from a
frontend, so reaching for one is a compile error rather than a review finding.

Two facade APIs were themselves the leak and had to change, since callers could not
avoid handles while they existed:

```
PdfDocument::get_font(Handle<Object>)  ->  get_font(obj_id: u32)
FontSummary::handle: Handle<Object>    ->  object_id: u32
```

## Consequences

- This is the substance the separate `fepdf` facade crate was meant to deliver. The
  crate itself would now be a rename, so `ARCHITECTURE.md` §3 records the facade as
  living in `fepdf-sdk` rather than claiming a crate that does not exist.
- An architecture rule that is not checked is a comment. This one is now checked by
  the build, which is the only reason §7 can make the claim it makes.
- A frontend needing a new model type must go through the facade, which is friction
  by design: it forces the question of whether the type belongs in the public
  vocabulary.
