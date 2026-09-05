# ADR-0056: A clone that was never finished, because the step that completes it was a separate method

- **Status**: Accepted
- **Date**: 2026-09-05
- **Commit**: (see the commit that adds this file)

## Context

Duplicating a page in the viewer produced a page the engine then refused to draw:

```
Failed to render page 36: Filter error (None): expected a stream, found null
```

**`ObjectCloner::clone_object` does not clone references. It queues them.** Each one becomes
an `Object::Null` placeholder in the target arena and a task on a stack, and
`process_queue` is what later fills the placeholder in. Only `clone_handle` called it.

Every other caller stopped at `clone_object`, so every clone came out with each of its
references still `Null`. For a page dictionary that is `/Contents`, `/Resources` and
`/Annots` — everything a page is made of. **Six call sites across two crates** had it:
duplicating pages, inserting a document, merging, extracting pages to a new document,
cloning AcroForm fields, and cloning an outline.

**`process_queue` ends with a validation pass written to catch exactly this**, refusing any
target object left `Null` where the source was not. It could not fire: it lives inside the
step that was never taken.

**The test that covered duplication checked page widths.** `/MediaBox` is a direct value —
the one part of a page dictionary that survives a clone which drops every reference it
holds. The fixture it used, `get_distinguishable_pdf`, has no `/Contents` at all.

## Decision

**`clone_complete` clones and then drains**, and `clone_object` is private. The pair is the
point: a caller can no longer take the first half by itself, because the half that does not
finish is not reachable from outside.

**The tests are about what a page is made of.** A new fixture carries a content stream, a
resource dictionary and a font, and the tests read the *text* of a clone back: a duplicated
page must read the same as the page it came from, and a page inserted from another document
must not come through empty.

## Consequences

Duplicating, inserting, merging and extracting produce pages that draw. Form fields and
outlines carried across a merge keep what they refer to.

**This was found by a reader duplicating a page in the viewer, not by the suite.** The suite
had a test for the operation and a fixture that could not express the failure — which is
the more useful half of the report: a green test over a document with no content streams
says nothing about content streams.

**Verified by putting the defect back**: with `clone_complete` reduced to `clone_object`
again, both new tests fail and the width test still passes.
