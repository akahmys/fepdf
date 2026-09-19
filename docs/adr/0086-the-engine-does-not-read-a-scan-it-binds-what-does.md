# ADR-0086: The engine does not read a scan; it binds what does

- **Status**: Accepted
- **Date**: 2026-09-19
- **Commit**: (see the commit that adds this file)

## Context

Phase M taught this engine to *draw* a scan: `CCITTFaxDecode`, `JBIG2Decode` and
`JPXDecode` all decode, and a scanned page renders. It could not *read* one. A page whose
only content is an image has no text, so extraction returns nothing, the structure tree has
nothing to hold, and every feature built on text — search, redaction by pattern, reading
order, the accessibility audit — has nothing to work with. JUST PDF [編集Pro] answers this
with 検索可能なPDFにする, and the comparison on 2026-09-19 left it as the largest capability
this product had no answer for at all.

Building one was never architecturally blocked. It was a question of what this engine is:
a recogniser is a model and a training story, it is the part of the work least related to
reading PDF, and the quality bar is set by services that do nothing else. The same
reasoning had already been applied to a DOCX converter and to `fepdf-wasm` — refusals that
rest on the nature of the thing, or on a product judgement, rather than on a corpus count.

What made the question different in 2026 is that the caller is frequently an assistant with
a vision model already in hand, and `fepdf-mcp` already builds 31 of the 32 operations —
the most complete frontend this engine has. The missing part was never the recogniser. It
was the **receiving end**: nothing could take text with coordinates and write it back onto
the page it came from.

## Decision

No OCR engine is built, and a window is opened for one.

- **Out**: the page rasterised, its dimensions, and whatever text is already on it with its
  positions.
- **In**: `Operation::AddTextLayer { page, items }` — text placed at rendering mode 3,
  invisible, carrying a `/ToUnicode` on an embedded font.
- **Where**: two tools on `fepdf-mcp`, and `fepdf edit text-layer --json` beside them for
  callers that are not an assistant.

This engine takes no responsibility for whether the recognition is correct, and full
responsibility for how the result is bound into the file: the encoding, the `/ToUnicode`,
the reading order, and the promise that nothing visible changed.

## Consequences

- **The check writes itself, and it is exact.** An invisible text layer is correct when the
  text comes back out of `inspect text` and **the page's CPU rasterisation does not change
  by one byte**. `--cpu` exists because vello's GPU pipeline gives more than one image for
  one scene; here it is what makes "nothing visible changed" a thing a test can assert.
- **It waits on font embedding.** A text layer with no `/ToUnicode` on a font that is not
  in the file is the defect this engine reports in other people's documents
  ([ADR-0085](0085-editing-what-a-page-draws-is-in-scope.md)).
- **A scanned page can become accessible without this engine recognising anything**, which
  is the outcome that matters: text, a language, a structure tree and an audit over the
  result.
- **What is declined with it**: no image pre-processing (deskew, despeckle, binarisation)
  and no confidence model. A caller that wants those has them where the recogniser is.
- **The refusal is reviewable on one condition** — a pure-Rust recogniser worth depending
  on, judged the way Phase M judged `hayro-jpeg2000` when `JPXDecode`'s refusal expired.
  Naming that condition is what keeps this from being permanent by inertia.
