# ADR-0124: Plain text is set by its own lines and paragraphs

- **Status**: Accepted
- **Date**: 2026-10-11
- **Commit**: (see the commit that adds this file)
- **Rests on**: [ADR-0091](0091-paragraphs-are-not-inferred-and-overflow-is-shown.md)

## Context

The owner chose "a PDF made from plain text" (ROADMAP AA-6) on 2026-10-10. ROADMAP had
already set four things:

- lines are broken by UAX #14 at the face's advances;
- pages are broken where they fill;
- each paragraph the input separates with a blank line is tagged `/P`;
- paragraphs are declared by the input, not inferred, as ADR-0091 decided of PDFs.

What remained open:

- what a plain text file's own line breaks mean;
- which encodings are read;
- what a page looks like when nothing is said about it;
- how the paragraphs enter a document that already has a structure tree, or has none.

## Decision

**`Operation::InsertText` sets text on pages and tags each paragraph `/P`**, and
`PdfDocument::from_text` is that operation applied to an empty document whose blank page
is then removed.

- **A blank line ends a paragraph. A line break is kept**: a plain text file's lines are
  its writer's, and joining them would be inferring the prose they are part of. **A form
  feed starts a page.** A tab is four spaces, and other control characters are dropped.
- **Lines break greedily at UAX #14's opportunities**, as far as the face's advances
  reach. A run wider than the line with no opportunity in it is broken between
  characters, the last resort UAX #14 leaves to an implementation. Japanese breaks
  between characters and never before 「、」 or 「。」, because UAX #14 says so; no other
  kinsoku is added.
- **One face sets the whole text**: the one the face ladder finds for all its characters
  (ADR-0089), embedded once. Text no face here draws is refused.
- **A page left unsaid is A4**, with an inch of margin, 10.5 points, and baselines one
  and a half sizes apart. A blank line's height is left between paragraphs, though not at
  the top of a page.
- **A paragraph is one `/P`**, however many pages it runs over. Each page's part is
  marked content with its own `/MCID`, named from the element by a marked-content
  reference to its page. The paragraphs go into the tree's one `/Document` element where
  it has one, and under its root where it has not.
- **A document with no tree is given one**: a `/Document` element, `/MarkInfo`, the
  text's language as `/Lang` where the catalogue states none, and `/ViewerPreferences
  /DisplayDocTitle true` where there were no preferences (PDF/UA-2 07-001). Each page's
  parent tree entry takes the next key, written as `/Nums` or as a leaf after `/Kids`.
- **Text is UTF-8, or UTF-16 with a byte order mark**, and nothing else. A legacy
  encoding such as Shift_JIS or Latin-1 is refused, not guessed at.

## Consequences

- A file of hard-wrapped prose keeps its wrapping, and a narrower page breaks its lines
  again inside it.
- Inserted into an untagged document that has content, the new pages are tagged and the
  old ones are not. The PDF/UA-2 audit says so.
- Not done: justification, hyphenation, kinsoku beyond UAX #14, vertical writing and
  bidirectional text.
