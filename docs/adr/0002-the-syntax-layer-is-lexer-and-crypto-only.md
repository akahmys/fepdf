# ADR-0002: The syntax layer is the lexer and the cryptography, nothing more

- **Status**: Accepted, amending an earlier scope
- **Date**: 2026-08-13
- **Commit**: `7732138`

## Context

`ARCHITECTURE.md` described `fepdf-syntax` as "lexer, parser, stream filters,
encryption/decryption" — the byte layer, sitting below the document model.

Measuring what those four modules actually depend on gave a different picture:

| Module | Depends on |
| :--- | :--- |
| `lexer` | nothing but `PdfResult` |
| `security` | nothing but `PdfError`/`PdfResult` |
| `parser` | arena, object, handle, error, lexer |
| `filters` | arena, object, handle, error |

The parser builds `Object`s, which requires the arena. A filter reads its own
`/DecodeParms` out of a PDF dictionary, which also requires the arena. Neither can
sit below the model without inverting a dependency they genuinely need — the same
shape of error as ADR-0001, caught before building rather than after.

## Decision

`fepdf-syntax` contains the lexer and the cryptography. The parser and the stream
filters stay in the model, and `ARCHITECTURE.md` says why.

The crate carries its own `SyntaxError` rather than dragging `PdfError` down a layer:
`PdfError` describes ingestion, clause violations and linearisation, which the byte
layer can never produce. `PdfError` gains `#[from] SyntaxError`, so call sites needed
only `?`.

## Consequences

- 850 lines at the time, not the 1,200 the earlier scope implied. It has since
  grown with the file-structure work described below.
- The cryptography can be reviewed without reasoning about document structure, which
  is the property worth having in that particular module.
- The byte layer later grew the file-structure work — header scanning, cross-reference
  parsing, recovery — which fits precisely because it is offsets and bytes, with
  object construction left to the model. That split held under load, which is some
  evidence the corrected boundary is the right one.
