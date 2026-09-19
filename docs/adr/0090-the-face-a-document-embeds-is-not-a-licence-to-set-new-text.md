# ADR-0090: The face a document embeds is not a licence to set new text in it

- **Status**: Accepted
- **Date**: 2026-09-19
- **Commit**: (see the commit that adds this file)
- **Amends**: [ADR-0089](0089-a-face-is-embedded-only-where-it-permits-it.md)

## Context

[ADR-0089](0089-a-face-is-embedded-only-where-it-permits-it.md) built a ladder of three
rungs and put the document's own embedded face on the first of them: re-subset what is
already in the file, and reach for a system face only when the glyph is missing. It also
recorded that whether a standard requires the embedding to be permitted had **not** been
checked against the standard's text, and named reading it as the first task of W-E1.

It was read on 2026-09-19, from `docs/specs/ISO_32000-2_sponsored-ec2.pdf` — which is in
this tree and which nobody had opened for this question — with this engine's own
`fepdf inspect text`. **Clause 9.9.1 answers it, and against the first rung.** In summary,
and without reproducing its wording:

- A font program's permissions are recorded in the program itself or in a separate
  licence, and one of them may forbid embedding entirely.
- A program may permit embedding **for viewing and printing only**, and not for creating
  new or modified text set in that font — in that document or in any other.
- Doing the latter requires a licensed copy of the program, and the standard says in so
  many words that a copy taken out of the PDF file is not one.
- **Absent explicit information to the contrary, an embedded program shall be used only to
  view and print the document.** It is a `shall`, and it makes silence a restriction
  rather than an absence of one.

The last of those is the one that reaches furthest. Of the 235 embedded programs in the
nine samples, 171 state nothing at all (ADR-0089 measured it), so for most documents the
face in the file is, by this clause, not available for setting text that was not already
there.

That also explains a behaviour this project had observed without reading a reason for it:
Acrobat asks for the font to be installed on the system before it will let text in that
font be edited. It is not a limitation of its implementation.

## Decision

The ladder loses its first rung for new or modified text, and has two:

1. **A face installed on this machine**, where its own `fsType` permits an embedding that
   may be edited and permits subsetting.
2. **Refuse**, naming the characters that could not be written, recording a `Decision`,
   substituting nothing.

The document's own embedded program keeps exactly the use the clause leaves it: it is
copied through unchanged when the document is rewritten, which is what saving already
does, and is viewing and printing. It is **not** re-subsetted to set text the document did
not have — unless the program's own `fsType` permits an editable embedding, in which case
it is a permitted face like any other and rung 1 is simply rung 1 reached by a shorter
path.

## Consequences

- **Editing text will refuse more often than it succeeds on a strange machine**, and that
  is the correct behaviour rather than a gap. A document whose fonts are not installed
  here cannot have its text reset in them.
- **ADR-0089's reading of silence was right and its first rung was not.** The measurement
  it rests on stands; what changes is which rung the face in the file sits on.
- **The bar being bit 3 rather than bit 2 is no longer this engine's judgement.** 9.9.1
  draws the same line between viewing and printing on one side and new or modified text on
  the other, so the choice is the standard's.
- **The GUI now has something to say.** A refusal that names the face and the characters
  is actionable — install the font, or choose another — where a silent substitution was
  not. What it offers is still open, and is the question ADR-0089 left open too.
- **`docs/specs/ISO_32000-2_sponsored-ec2.pdf` is in the tree and is searchable with this
  engine.** Two records now rest on a question that could have been answered at any point
  by running `fepdf inspect text` over it. The hierarchy in `AGENTS.md` puts the standard
  first; this is the first time in this phase that it was consulted first.
