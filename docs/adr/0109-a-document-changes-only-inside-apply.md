# ADR-0109: A document changes only inside `apply`

- **Status**: Accepted
- **Date**: 2026-10-03
- **Commit**: (see the commit that adds this file)

## Context

Rule D says every change to a document is an `Operation` that `apply` carries out.
`scripts/audit/layering.py` held it by counting the facade's `&mut self` methods. The
arena writes through `&self`, so that count could not see a write made through a shared
reference, and nothing else looked (ROADMAP Y-11).

On 2026-10-03 the arena's seven writers were made to panic while the document was sealed,
and the whole test suite was run. 106 failures traced to five sources outside `apply`.
A sixth, `appearance_of`, failed once the first five were fixed, because the page's own
missing `/Resources` had failed its one test first.

| Source | What it wrote | Tests |
| :--- | :--- | ---: |
| `Page::resources_handle`, from rendering and `fonts_of_page` | an empty dictionary, each call, for a page with no `/Resources` | 70 |
| `ActionReport::of` | a copy of the catalogue dictionary, to read `/Requirements` through | 17 |
| the interpreter's `d` operator | the dash array, to pass it through the operand stack | 8 |
| `apply_physical_redaction_to_page` | the redaction, from the facade, `fepdf-mcp` and `fepdf-gui` | 3 |
| tests | fixtures built by writing into an opened document, and three calling the font writer on one | 8 |
| `PdfDocument::appearance_of` | an empty dictionary for an appearance with no resources | (1) |

The first three rows and the last are readers. Each wrote into the document it read, every
time it read it.

## Decision

**The facade seals a document's arena once it is opened**, and also one it builds by
merging, by extracting pages or from `PdfDocument::from_document`. A write while sealed
panics in a debug build and costs one atomic load in a release build. `apply` unseals the
arena for the one operation, through `Document::change`. `PdfArena::transaction` is
`pub(crate)`, so nothing above the model can unseal.

- Readers write nothing. A page with no `/Resources` is given an empty one at load, with a
  7.7.3.3 `Decision`, since Table 31 requires the entry. A reader that needs "no
  resources" draws with one empty dictionary the document allocates when it is built.
  `ActionReport::of` reads the catalogue object it already has. The `d` operator sets the
  dash in the graphics state directly.
- Redaction is let through by name. `Document::redaction_until_y10` unseals for
  `apply_physical_redaction_to_page` alone, and Y-10 removes both when it makes redaction
  an `Operation`.
- A test that builds a fixture by writing builds it as a model `Document` and hands it to
  the facade with `PdfDocument::from_document`.
- `PdfDocument::create_empty` gives its page an empty `/Resources`. Without one, opening a
  new document recorded a repair.

## Consequences

- `sealed_document_test.rs` holds that the seal fires. Removing `seal()` from the facade
  failed that test.
- The check runs only in debug builds, the profile `cargo test --workspace` uses. The
  arena's handle check (`PdfArena::ours`) avoids `debug_assert!` so that it behaves the
  same in both profiles. This one does not, because a check in release would put a cost
  on every write for a defect that a test finds.
- A model `Document` the facade never wraps is never sealed. Ingestion, the copy a save
  writes, and assembly all write through one.
