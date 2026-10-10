# ADR-0119: An annotation without an appearance is given one from its own entries

- **Status**: Accepted
- **Date**: 2026-10-10
- **Commit**: (see the commit that adds this file)

## Context

XFDF (ISO 19444-1) carries an annotation's appearance only for a stamp (6.5.2). Every
other kind arrives as its entries alone: rectangle, colour, quadrilaterals, vertices,
ink, words. Table 166 of ISO 32000-2 requires an `/AP` on every annotation except a
Popup, Projection or Link and one whose rectangle is a point. So an engine that imports
XFDF and writes PDF 2.0 has to draw the appearance itself, or write a file that does not
conform.

The engine already drew appearances, but only for the twelve kinds `AddAnnotation`
makes. It drew them from `AnnotationKind`, the caller's description, not from a
dictionary. XFDF's `annots` element admits nineteen kinds (6.4.1).

The owner chose, on 2026-10-10, that every kind the standard defines gets an appearance.
The other options were to import only the kinds that could be drawn, or to import
everything and write the rest non-conforming.

## Decision

**An annotation that has no `/AP` is given one, drawn from its own entries.** This covers
every subtype XFDF admits that Table 166 does not exempt: Text, FreeText, Line, Square,
Circle, Polygon, PolyLine, Highlight, Underline, Squiggly, StrikeOut, Caret, Stamp, Ink,
FileAttachment, Sound and Redact.

- **The entries are read as the standard defines them.** Each subtype's table in 12.5.6
  says which entry holds what: `/QuadPoints`, `/Vertices`, `/InkList`, `/L` and `/LE`,
  `/IC`, `/BS`, `/RD`, `/DA` and `/Q`, `/Sy`. The drawing follows the entry, and an
  entry the drawing does not use is not invented.
- **Icons are this engine's.** Table 175 (Text), Table 187 (FileAttachment), Table 188
  (Sound) and Table 184 (Stamp) name icons and say that an interactive processor "shall
  provide predefined icon appearances". They do not draw them. Each name gets a simple
  drawing here, and an unknown name gets the subtype's default.
- **One generator, from the dictionary.** The import of XFDF uses it, and so does the
  import of an FDF annotation without `/AP`. `AddAnnotation` keeps its own drawing,
  which knows the caller's intent, until the two are shown to agree.

## Consequences

- An XFDF import writes a conforming file for every kind it admits.
- The appearance of an imported annotation is this engine's drawing of its entries, not
  the drawing of the program that made it. XFDF does not carry that drawing, so no
  importer has it.
- Drawing a stamp's name in words needs a face, through the same ladder as any text this
  engine sets ([ADR-0089](0089-a-face-is-embedded-only-where-it-permits-it.md)).
