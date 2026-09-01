# ADR-0051: Nothing declares a binding direction, so removing the guess did not restore the default — it removed the feature

- **Status**: Accepted
- **Date**: 2026-09-02
- **Commit**: (see the commit that adds this file)

## Context

The viewer decided which end to open a book at from `/ViewerPreferences /Direction`
(12.2, Table 30), and where the document said nothing it guessed: a font whose name carries
the `-V` writing-mode suffix, or a `/Lang` beginning `ja`, meant right-to-left. That guess
was deleted in the GUI work of 2026-09-01, leaving `doc.viewer_direction()` as the only
source.

**`/Direction` is declared by 0 of the 524 files in both corpora.** So the declared source
is empty for every document available, and after the deletion right-to-left binding was not
merely rarer — it was unreachable. Table 30 gives the entry a default of `L2R`, so the
deletion reads as "obey the standard's default", and the effect is that a vertically set
Japanese novel opens at the wrong end with nothing said.

Which documents this touches is small and known: `samples/bokutokitan.pdf` carries four
fonts with a `-V` suffix and `samples/fy05.pdf` six. The other seven samples carry none.

**The deletion also runs against the shape
[ADR-0041](0041-a-character-collection-is-declared-not-guessed.md) settled**, which is the
same question one clause over. There the engine was guessing a character collection from a
font's name while the file declared one, and the fix was: *obey the declaration where there
is one, and keep the name heuristic where the file declares nothing, because that is the
case it was written for and where it is the only thing to go on.* `/Direction` is the
second half of that sentence, and nothing declares it at all.

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

A vertically set document opens right to left again, and says why. A document that declares
a direction is unaffected — the guess is not reached. A document that declares nothing and
is not vertical CJK gets Table 30's default, silently, because there is nothing to say.

**`-V` is a real signal rather than a spelling coincidence**: it is the writing-mode suffix
Adobe's CMap names carry for vertical forms (9.7.5.2). The test carries a control for the
opposite case — `NotoSerif-Vietnamese` ends in no `-V` and a name merely containing a `V`
is not a mode — because "vertical CJK binds right to left" and "everything binds right to
left" are otherwise the same green test. Verified by breaking it three ways: always
answering `R2L`, never answering, and dropping the `/Lang` half.

**What is still not measured is whether the heuristic is any good.** It fires on two files
and no second implementation was consulted about either. It is a guess that says it is a
guess, which is the most that can be claimed for it until a document declares `/Direction`
and disagrees.
