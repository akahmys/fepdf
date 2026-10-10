# ADR-0115: An operation names an annotation by its place on the page

- **Status**: Accepted
- **Date**: 2026-10-10
- **Commit**: (see the commit that adds this file)

## Context

AA-2 adds operations that act on an annotation already on a page: remove it, edit its
words, reply to it, set its state (12.5.6.2, 12.5.6.3). Each has to say which annotation
it means. There were two candidates:

1. **`/NM`**, which Table 166 defines as the annotation's name, "uniquely identifying it
   among all the annotations on its page". This is what the standard provides for the
   job. But `/NM` is optional, and the engine writes none. An annotation without one
   would have to be given a name when the document is read. Every save would then write
   a new `/NM` into every annotation the file had, links and widgets included, which is
   a change the reader did not make.
2. **The annotation's place in the page's `/Annots`**: page 3, annotation 7. This needs
   nothing written at load. It is stable wherever it is used: an operation's index means
   the annotations as they stand when the operation is applied, and replaying the
   history applies the same operations in the same order to the same origin
   ([ADR-0114](0114-the-window-journals-its-acts-and-a-crash-replays-them.md)), so the
   index means the same annotation on every replay.

ROADMAP's first draft of AA-2 named `/NM`. That was written before the annotation code
was read, so it was not a measurement.

## Decision

**An operation names an annotation by its page and its index in that page's `/Annots`.**
The facade's list of a page's annotations gives each one's index, so a caller names an
annotation by reading the list.

An annotation this engine creates gets an `/NM`. The name is unique on its page and
deterministic, so a replay writes the same one. Other software that matches annotations
by name, as FDF import does, then finds them.

## Consequences

- Nothing is added to a document at load. An annotation's identity in an operation is
  the same thing the reader sees: its place on the page.
- An index is valid only against the document it was read from. After an operation that
  removes an annotation, the indices after it shift, and a caller has to read the list
  again. The window rereads it after every act, as it does for the outline.
- An index does not survive into another document. FDF (AA-3) carries `/NM` and the page
  for that reason, and does not carry indices.
