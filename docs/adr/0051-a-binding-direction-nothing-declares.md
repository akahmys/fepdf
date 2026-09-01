# ADR-0051: Almost nothing declares a binding direction, so a vertical book that is silent about it opens the wrong way round

- **Status**: Accepted
- **Date**: 2026-09-02
- **Commit**: (see the commit that adds this file)

## Context

The viewer decided which end to open a book at from `/ViewerPreferences /Direction`
(12.2, Table 30), and where the document said nothing it guessed: a font whose name carries
the `-V` writing-mode suffix, or a `/Lang` beginning `ja`, meant right-to-left. That guess
was deleted in the GUI work of 2026-09-01, leaving `doc.viewer_direction()` as the only
source.

**`/Direction` is declared by 2 of the 524 files in both corpora** — `bokutokitan.pdf`
says `R2L` and one external file says `L2R`. So for almost every document Table 30's
default of `L2R` is what applies, and a vertically set book opens at the wrong end unless
something looks at it.

**Which document this actually touches is one.** `samples/fy05.pdf` is set vertically —
six fonts in a `-V` writing mode — and declares nothing, so only the guess reaches it.
`samples/bokutokitan.pdf` carries four such fonts and would seem to be the second case, and
is not: **it declares `R2L` itself**, so the guess is never reached for it and the deletion
never affected it. The other seven samples carry no vertical font at all.

**The deletion also runs against the shape
[ADR-0041](0041-a-character-collection-is-declared-not-guessed.md) settled**, which is the
same question one clause over. There the engine was guessing a character collection from a
font's name while the file declared one, and the fix was: *obey the declaration where there
is one, and keep the name heuristic where the file declares nothing, because that is the
case it was written for and where it is the only thing to go on.* `/Direction` is the
second half of that sentence: `fy05.pdf` declares nothing, and the guess is the only thing
that reaches it.

## Decision

**The guess comes back, and it is recorded where a reader can see it.**
`infer_binding` is reached only when the document declares no direction, and when it fires
the viewer appends an `Ambiguity` of 12.2 to the same list of decisions the sidebar already
shows — naming what it found and that it departed from Table 30's default.

It is the *frontend* that records it, not the engine. Binding is a viewer question
(6.3.2.3), `fepdf::Decision` is public, and the GUI already carries `doc.decisions()` into
its sidebar, so the guess appears beside the engine's own reading decisions without the
engine growing an opinion about how a book is held (ADR-0025, ADR-0031).

## Consequences

A vertically set document that declares nothing opens right to left again, and says why. A
document that declares a direction is unaffected — the guess is not reached, which is why
`bokutokitan.pdf` was never broken and never fixed by any of this. A document that declares
nothing and is not vertical CJK gets Table 30's default, silently, because there is nothing
to say.

**This record said 0 of 524 and the number was 2.** It came from grepping
`inspect catalog` for the word, which does not print `/ViewerPreferences` entries; asking
`PdfDocument::viewer_direction` instead — the same call the viewer makes — finds two. The
correction matters to the argument and not only to the figure: "the deletion made
right-to-left unreachable" was false, since any file that declares it still gets it, and the
case for the heuristic rests on the one document that is vertical and silent rather than on
two. It was caught by running the viewer and finding the decision log empty on
`bokutokitan.pdf`, which is what a guess that is never reached looks like.

**`-V` is a real signal rather than a spelling coincidence**: it is the writing-mode suffix
Adobe's CMap names carry for vertical forms (9.7.5.2). The test carries a control for the
opposite case — `NotoSerif-Vietnamese` ends in no `-V` and a name merely containing a `V`
is not a mode — because "vertical CJK binds right to left" and "everything binds right to
left" are otherwise the same green test. Verified by breaking it three ways: always
answering `R2L`, never answering, and dropping the `/Lang` half.

**What is still not measured is whether the heuristic is any good.** It fires on one file
of 524 and no second implementation was consulted about it. It is a guess that says it is a
guess, which is the most that can be claimed for it until a document declares `/Direction`
and disagrees.
