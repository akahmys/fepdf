# ADR-0118: A reply carries an appearance that draws nothing

- **Status**: Accepted. Amends AA-2a's replies and state changes.
- **Date**: 2026-10-10
- **Commit**: (see the commit that adds this file)

## Context

AA-2a wrote a reply (`ReplyToAnnotation`) and a state change (`SetAnnotationState`) as
text annotations with no `/AP`. They had the `/Rect` of the annotation they answer, on
the reasoning that 12.5.6.2 says a reply is shown with what it answers and not on its
own.

Reading ISO 32000-2 for AA-4 found that this breaks Table 166. Its `/AP` entry says a
PDF writer "shall include an appearance dictionary when writing or updating the PDF file"
for every annotation except two cases. One is a Popup, Projection or Link. The other is
a `/Rect` whose two corners are the same point. A reply with its parent's rectangle is
neither. So every reply and state this engine wrote made the file non-conforming, and
nothing checked it: the Arlington test runs on saved samples, and no sample carries one.

## Decision

**A reply and a state change carry an appearance that draws nothing.** It is a form
XObject with an empty content stream. The annotation keeps its parent's `/Rect`.

The other way out of the rule was to give the reply a point `/Rect`. It was not taken,
because a reader that does place a reply, against the clause, would then place it
nowhere.

## Consequences

- What this engine writes for review conforms to Table 166.
- Files written before this change still carry replies without `/AP`. Reading one is
  unaffected. Saving one again leaves it as it is, since nothing regenerates appearances
  on load. [ADR-0119](0119-an-annotation-without-an-appearance-is-given-one-from-its-own-entries.md)
  is the mechanism that could.
