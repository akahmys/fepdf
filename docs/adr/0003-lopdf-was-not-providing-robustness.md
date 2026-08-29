# ADR-0003: lopdf is not what makes malformed files readable

- **Status**: Accepted
- **Date**: 2026-08-15
- **Commit**: `3da4ccf` and the reader work that follows it

## Context

`lopdf` was adopted so that badly-behaved PDFs could still be read, and the belief
that it delivered that was written into the code:

```rust
pub fn open_repair(data, options) -> PdfResult<Self> {
    // lopdf's load_mem is already quite robust, but we could add more repair logic here
    Self::open(data, options)
}
```

The belief was never tested. Deciding whether to replace the reader required knowing
whether it was true, so six malformed files were built from a healthy sample and fed
through the existing path.

| Damage | Result |
| :--- | :--- |
| Header version mangled | read |
| `startxref` offset destroyed | failed |
| Cross-reference table corrupted | failed |
| Bytes prepended before `%PDF-` | failed |
| `trailer` keyword destroyed | failed |
| Truncated to 60% | failed |

`fepdf edit repair` failed on the same files with the same errors. Reading lopdf
0.34's source confirmed why: it has no reconstruction path at all — `xref_and_trailer`
either parses or returns an error.

The prepended-bytes case matters most. Files routinely arrive with bytes added by
mail gateways and scanners; readers are expected to scan for the header, and the
major ones do.

## Decision

Write the file-structure layer (ISO 32000-2 7.5) rather than delegate it.

The robustness argument does not weigh against this, because the robustness does not
exist. Removing `lopdf` costs one behaviour — tolerating a mangled header version —
which is a few lines to reproduce. Everything else on that list has to be built
either way, so replacing the reader and *gaining* the robustness are the same work,
not competing ones.

## Consequences

- What `fepdf` can read is no longer bounded by what `lopdf` implements, which was
  the other reason to do this: PDF 2.0 conformance cannot be claimed on top of a
  reader whose coverage is someone else's decision.
- The six files above became the acceptance test. Reading now recovers 111 objects
  from five of them — the same count as the undamaged file — and 77 from the
  truncated one, being all that survives.
- Roughly ninety of the ninety-five `lopdf` references were conversion code between
  its object model and ours. Removing the dependency deletes them rather than
  replacing them, so the reader is not as much net new code as the count suggests.
- Every tolerance the new reader applies is recorded as a `Decision`
  (`ARCHITECTURE.md` §4.3). `lopdf` logged its repairs to stderr, where the caller
  could not see them: a wrong `/Length` was silently absorbed and the document
  reported as read.
