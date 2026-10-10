# ADR-0122: An annotation whose appearance is its content is drawn only when it is made

- **Status**: Accepted
- **Date**: 2026-10-10
- **Commit**: (see the commit that adds this file)

## Context

The owner asked on 2026-10-10 that every subtype be creatable. The exceptions were the
three deprecated in PDF 2.0 (Sound, Movie, TrapNet) and 3D and RichMedia, which are shown
and not made. That left ten subtypes `AddAnnotation` could not make: Polygon, PolyLine,
Caret, FileAttachment, Screen, Popup, PrinterMark, Watermark, Redact and Projection.

Five of them, and Polygon and PolyLine as shapes, are drawn by
[ADR-0119](0119-an-annotation-without-an-appearance-is-given-one-from-its-own-entries.md)
from their entries. For three the appearance *is* the content: a printer's mark, a
watermark and a screen. [ADR-0120](0120-a-document-is-opened-with-every-annotation-given-an-appearance.md)
gives an opened PrinterMark or Watermark that lacks an appearance none, because its
entries do not say what it showed.

## Decision

**Each kind writes the entries its table names. It is drawn from them where ADR-0119's
drawing draws that subtype. Where the appearance is the content, it is drawn from what
the caller gave, and only when this engine makes it.**

- A printer's mark is a registration target or a colour bar, named by `/MN`.
- A watermark is its words, as large as the size given and no wider than the rectangle,
  at `/CA`'s opacity. It is refused when no face here draws them.
- A screen is a frame and a play mark. Given a clip, it gets a rendition action whose
  `/AN` names the screen itself (Table 214).
- **A popup and its parent name each other** (`/Parent`, `/Popup`). It is refused when
  the parent is not a markup annotation held as an object, or already has a popup.
- **Only markup annotations are named and signed** (Table 171). A screen, popup,
  printer's mark and watermark are not markup annotations, and carry no `/T` author.

## Consequences

- An opened document's appearance-less printer's mark or watermark is still left without
  one, as ADR-0120 decides. The drawings here are not reached from `give_missing`.
- Twenty-three of the twenty-eight subtypes can be made: link, widget (through form
  fields) and the twenty-one markup and other kinds. Sound, Movie and TrapNet cannot, by
  deprecation; 3D and RichMedia cannot, by the owner's choice.
