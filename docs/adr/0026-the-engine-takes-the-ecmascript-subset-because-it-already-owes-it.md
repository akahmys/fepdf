# ADR-0026: The ECMAScript subset is taken, because work already undertaken depends on it

- **Status**: Accepted
- **Date**: 2026-08-22
- **Commit**: *(the commit that takes the subset)*

## Context

[ADR-0022](0022-what-a-document-does-is-a-settled-question-where-reads-an-action-is-not.md)
declined ECMAScript, and the sentence it rested on was:

> declining ECMAScript is **a choice the conformance model provides for**, not a gap

That is true, and it is **permission rather than judgement**. 6.3.2.1 establishes that a
processor may decline a subset; it says nothing about whether this processor should. The
record established that not-choosing was legitimate and was then read, here and in
`ROADMAP.md`'s subset table, as though the choice had been made. **It had not.** A subset
that was never chosen was written down as a subset that was declined.

The other refusals in that table do carry reasons: multimedia because 13.4 is deprecated
in 2.0, XFA because it is deprecated *and* is a second form model beside the one that
works. ECMAScript's reason was "the language is ISO/DIS 21757-1, not this document" — the
eighty-one-normative-references argument, which is sound for PRC and U3D and weaker here,
because 21757-1 specifies the **object model**, not the language. The language is
ECMA-262. And where a 3D format cannot be usefully implemented in part, a scripting object
model can: PDF.js implements the part its documents touch.

**What filled the gap instead was demand, and demand is circular.** "Two of 524 files run
code on open", "`/AA /C` occurs zero times" — a capability that does not exist has no
users, because the people who need it use something else. `ROADMAP.md` has said since
Phase L that a corpus is grounds for building and never grounds for declining; the
argument was made anyway, relabelled as sequencing, which is the same argument wearing a
different coat.

**"Do not build what is not needed" is a discipline that applies after the decision, not a
method for taking it.** `fepdf-resource`, an `Operation` vocabulary of which 19 of 24 were
stubs, ingestion options nothing read, and two HTTP dependencies used by no line of code
are all failures of that discipline — things built without a judgement. Using it *as* the
judgement is a different error and produces the opposite failure: things not built without
a judgement either.

## Decision

**The criterion.** 6.3.2.1 says a processor shall comply with the provisions of the
subsets it chose. Read as a test rather than as permission, that gives one:

> **A subset is required when the engine has already undertaken work whose correctness
> depends on it.**

It appeals to no user, no corpus and no forecast — only to what this engine has already
committed to doing.

**Applied, it decides the question, and the engine had already written down the answer.**
`SetFormFieldValue` sets a value in an AcroForm. When the form declares a calculation
order, `apply/annotations.rs` records:

```
[VIOLATION] ISO 12.6.3 : the form declares N field(s) in its calculation order, and
  setting <field> would have run their ECMAScript
  -> wrote the value and did not run the scripts; fields computed from it are now stale
```

`Violation` is the severest of the three levels and means something was dropped or
substituted. The engine performs form editing and reports, in its own decision log, that
it cannot finish it. That is not a missing feature; it is **an undertaking with a hole in
it**, and the hole is exactly this subset.

So: **the ECMAScript subset is taken**, for document and field scripting — 12.6.4.17's
execution, and of ISO/DIS 21757-1 the objects that form scripts reach for (`app`, `this`
as `Doc`, `Field`, `event`, `util`, `color`). Not `Collab`, `security`, `SOAP`, media, or
the XFA bindings, none of which any undertaking of this engine depends on.

**The engine is boa**, settled by
[ADR-0024](0024-pure-rust-is-a-rule-and-therefore-has-a-check.md): RR-15 Rule 9 forbids
compiling C and QuickJS cannot satisfy it. **The shape is a fifth frontend**, settled by
[ADR-0025](0025-a-script-processor-is-a-frontend-not-a-subsystem.md): translation into the
same `Operation` vocabulary, with no path of its own into the document.

**The alternative was real and is recorded because it was rejected on merit, not
overlooked.** The engine could have *narrowed the writer subset* instead — refused
`SetFormFieldValue` on a form carrying `/CO`, and declared that. It is cheap and honest.
It was rejected because the resulting statement is worse than the gap: this engine would
not be able to set a field value on a third of the forms it meets, in an engine whose
`/AcroForm` support is otherwise complete enough to build appearances from values
(12.7.4.3). Narrowing a chosen subset to fit an unbuilt one is a decision too, and this is
not the place for it.

**What was decided is the subset, not the schedule.** Phase P holds measured rendering
defects that are broader and far cheaper — one PDF function evaluator (7.10) fixes a spot
colour rendering white and a stitching gradient rendering black-to-white — and Phase Q
holds Rule D, which this design leans on and which does not currently hold. Those come
first, on cost and on dependency. Neither is a reason the subset is not needed.

## Consequences

- **`ROADMAP.md`'s subset table gains its second "chosen and not met" row.** That table's
  own distinction is that "not implemented" is not a defect and "chosen and not complied
  with" is. Taking this subset therefore *creates* a declared non-compliance where there
  was a conforming refusal, and it will stand until Phase R lands. That is the honest
  consequence of deciding, and the same position `fepdf-gui` has held against 6.3.2.3
  since Phase P named it.
- **The 12.6.3 `Violation` stays until scripts run, and it is correct.** It fires on every
  `SetFormFieldValue` against a form with a non-empty `/CO`, whichever field is set, and
  that is not [ADR-0008](0008-an-indirect-length-is-not-an-ambiguity.md)'s "fires on
  conforming input" — which field a calculation reads cannot be known without running it,
  so the output may be stale in every one of those cases. Conservative is the only
  available answer, and Phase R removes the condition rather than the report.
- **ADR-0022 is amended, not superseded.** Everything it decided about *reading* actions —
  classifying by consequence, `Trigger::without_interaction()`, refusing to fold an
  unknown `/S` into the harmless group, walking every place an action can hang — stands
  unchanged and becomes the input to execution. Only the refusal is reversed, and its
  conformance argument remains true: declining *was* permitted. It was simply never
  chosen.
- **This is the first requirement decision this project has taken on a stated criterion**,
  and the criterion is now the thing to attack. If it is wrong, it is wrong in a way that
  can be argued with, which "no one is asking for it" was not.
