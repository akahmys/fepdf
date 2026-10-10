# ADR-0120: A document is opened with every annotation given an appearance it can be

- **Status**: Accepted
- **Date**: 2026-10-10
- **Commit**: (see the commit that adds this file)

## Context

This engine draws an annotation by its `/AP` (12.5.5), and nothing else. Table 166
requires an `/AP` on every annotation except a Popup, a Link, a Projection and one whose
rectangle is a point. Before PDF 2.0 it was optional, so files written to 1.x lack it.

Measured over the samples and the external corpus, 525 files, on 2026-10-10:

- 11 of the 30 widgets in `sample_02c.pdf` have none: text, choice and button fields.
- One widget, five movies, one 3D, one RichMedia and one file attachment have none, all
  in isartor and veraPDF files made to fail.

Such an annotation was not drawn, and translating the file wrote it on into a PDF 2.0
file that does not conform. [ADR-0119](0119-an-annotation-without-an-appearance-is-given-one-from-its-own-entries.md)
built the drawing for seventeen subtypes, but used it only on import.

The owner asked on 2026-10-10 that all twenty-eight subtypes be shown.

## Decision

**Opening a document gives every annotation that has no appearance the one it can be
given, before the arena is sealed, and records a `Decision` for each.** This is the
normalisation [ADR-0013](0013-a-document-is-one-normalised-state.md) makes at load, not
an edit. A replay of the history opens the same bytes and makes the same appearances.

- **Seventeen subtypes** are drawn from their entries (ADR-0119).
- **A widget** is drawn from its field. A text or choice value is set in the field's
  `/DA`, through the code that sets a value, which already draws it. A check box or radio
  button gets an on state and `/Off`, the on state named by `/AS` or `/Yes`. A push
  button gets its `/MK /CA` caption in a frame.
- **Movie, 3D and RichMedia** get a frame and a sign of what they hold: a play mark, or a
  cube for 3D. Their real appearance is a still of media this engine does not read.
- **A Screen** gets none and nothing is recorded. 12.5.6.18 says one without `/AP` has no
  default appearance and is not printed, so the file is conforming as it is.
- **PrinterMark, TrapNet and Watermark** get none. The appearance *is* the content of
  each, so without one there is nothing to draw. The `Decision` is a violation of Table
  166.

## Consequences

- Every subtype a file can carry is shown, where anything can be shown for it.
- Translating such a file writes the appearances the engine made, so the output conforms
  where the input did not. The decisions say which appearances are the engine's.
- Opening costs a pass over every page's `/Annots`. It is one dictionary lookup per
  annotation where all have an `/AP`.
