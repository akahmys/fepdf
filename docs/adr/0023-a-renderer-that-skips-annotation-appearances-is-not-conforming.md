# ADR-0023: A renderer that skips annotation appearances is not conforming

- **Status**: Accepted
- **Date**: 2026-08-22
- **Commit**: *(the commit that adds `render_annotations`)*

## Context

While answering a question about whether this engine needs an ECMAScript interpreter, the
standard was read with this engine — `inspect text` over `docs/specs/`'s copy, 1020 pages
in 3.7 seconds — and clause **6.3.2.2, "PDF processors providing rendering"** turned up
two `shall`s that bind anything that draws a page. One is honouring optional content
(8.11), which Phase N-1 had already built without knowing it was answering a requirement.
The other is rendering the appropriate appearance stream for every annotation that has
one, unless the annotation flags say otherwise.

**This engine drew no annotations at all.** Nothing in `fepdf-content` or `fepdf-render`
so much as read an `/AP`. Measured on `pdf20examples/PDF 2.0 UTF-8 string and
annotation.pdf`, a conforming file whose page carries no `/Contents` and whose only mark
is a `/Highlight`'s appearance:

| | quadrants |
| :--- | :--- |
| this engine | `254 254 254 254` — blank paper |
| PDFKit | `251 253 255 255` |

6.3.2.1 lets a processor choose which subsets of PDF functionality to support, which is
what makes declining ECMAScript conforming (ADR-0022). It does **not** help here: 6.3.2.2
is a provision *of* the rendering subset, and this engine has chosen rendering.

**The cross-check could not have caught it.** `crosscheck_image.sh` compares this engine
with PDFKit through `examples/page_quadrants.rs`, and that example built its own
interpreter and executed the page's content streams. It never called `render_page`, so
whatever `render_page` did or did not do was outside what the comparison could see. A file
this project did not write, in a corpus this project did not choose, compared against a
second renderer — and the hole was in the harness rather than in the corpus.

## Decision

**Annotations are rendered, as part of rendering a page.** `render_page` walks `/Annots`
after the content streams and draws each appearance through its own interpreter: the
appearance is a form XObject with its own coordinate system and its own `/Resources`, so
it cannot share the page's.

**12.5.5's algorithm is implemented as written**, including the step that is easy to skip.
The appearance's `/BBox` is transformed by its `/Matrix`, and what gets mapped onto
`/Rect` is *the smallest upright rectangle around the resulting quadrilateral* — not the
quadrilateral and not the original box. The answer is `Matrix × A`, so the appearance's own
matrix still applies inside the mapping. Writing that composition the other way round put
a rotated appearance outside the rectangle it had just been measured against, and a unit
test caught it before anything rendered.

**Two flags stop it and a third does not.** `Hidden` (bit 2) and `NoView` (bit 6) are
honoured, because this renders to a screen. `Print` (bit 3) alone does not stop anything —
reading it as "print only" would hide half a document. `Invisible` (bit 1) is **not**
applied and the reason is written where the code is: it governs annotations of no standard
type *for which no handler exists*, and this engine has no handlers at all — it draws
appearance streams, which is what the flag's own "if clear" branch describes.

**A broken content stream no longer cancels the annotations.** The two are the two halves
6.3.2.2 asks for, and `UnknownFilter-PageContentStream.pdf` is the case: its content stream
dictionary is malformed, and drawing nothing at all because of that loses every annotation
the page carries. The error is still returned — `extract_text` reports it and the corpus
measurement counts it — but after the page has been drawn as far as it can be.

**`page_quadrants` now calls `render_page`.** The comparator's job is to answer the same
question a reader would, and building the interpreter itself meant answering a narrower
one.

## Consequences

- **`crosscheck_image.sh` gained the file that exposed this**, and is 18 compared with all
  agreeing. The annotation page is now `250 252 254 254` against PDFKit's
  `251 253 255 255`.
- **Optional content on an annotation is honoured for the first time.** `/OC` on an
  annotation (8.11.3.2) had nothing to act on while nothing drew annotations; it does now,
  and a test covers it.
- **An `/AP /N` that is a dictionary of states with no `/AS` draws nothing** and records
  why. One state and no `/AS` is not ambiguous, so it is drawn and recorded as a repair.
  Choosing between several would be this engine deciding what the document did not.
- **What is still not drawn**: an annotation with no appearance stream at all. 12.5.2 lets
  a processor synthesise one from `/C`, `/IC`, `/Border` and the subtype's own entries, and
  that is a per-subtype renderer rather than a walk. The corpus says what it would buy:
  30,016 of the 30,098 annotations are `/Link`, which carry no appearance and are not
  meant to be seen.
- **`status.sh`'s decision-site row had to learn two more crates**, for the second time.
  It searched three and the new sites are in `fepdf-doc` and `fepdf`; the figure read 69
  when it was 75. A row that names the places it looks will keep missing new ones.
