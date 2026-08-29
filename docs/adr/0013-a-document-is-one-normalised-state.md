# ADR-0013: A document is one normalised state, settled at load

- **Status**: Accepted
- **Date**: 2026-08-16
- **Commit**: `3930b99`, on top of `2e2b642` and `b773252`

## Context

The question was whether to move the `/Info` dictionary into XMP when a file is read.
The first answer given was no — that reading should report the file as it is, and a
migration of that kind belongs to refinement.

That answer was reasoned from `ARCHITECTURE.md` §4.4, which describes the **Reading**
stage as placing each object in the arena as written with every tolerance recorded. All
of which is true of that stage, and none of which is true of the pipeline it opens.
Nothing in the document said what the composition amounts to, so it was possible to read
one stage's property as the whole:

| | Where it happens | How that was established |
| :--- | :--- | :--- |
| Object refinement | inside `Ingestor::ingest` | `perform_active_refinement` is called there |
| Text decoding | inside ingest | the arena holds `Text("…")` before anything is saved |
| Revision merging | as the file is read | ADR-0006 |
| Decryption | Pass 0 | `/Encrypt` is gone once a `Document` exists |
| **Metadata** | **at save** | `update_document_metadata` had two callers, both in `save_*` |

So the engine already normalised at load in every case but one, and the objection
applied to the exception rather than to the proposal.

The fidelity concern behind the objection also already had an answer in the code.
`FileStructure::survey`, `CatalogReport::survey`, `InteractiveReport::survey` and
`EncryptionReport::survey` all take `&[u8]` and run `reader::load_document` with Pass 0
and nothing else. They never see a refined arena. There were two layers; neither was
named, so the layering rules described one of them and the code kept the other.

Leaving metadata at the save boundary had a cost that had gone unmeasured. `/Info` was
read and then every XMP field overwrote it with no comparison, so a file that says two
things was resolved in silence — `samples/fy05.pdf` dates itself six days apart in its
two places and nothing reported it.

## Decision

**A `Document` is one normalised state, produced at load. What the file said is
answered from its bytes, not from the arena.**

The two layers are named rather than invented:

- **The byte layer** — `reader::load_document` and Pass 0. Reports the file: revisions,
  cross-reference form, catalogue entries as written, the encryption that was in force.
  `inspect structure`, `catalog`, `interactive`, `encryption`.
- **The document layer** — `Document::open`. Reports the document the engine made of it.
  `inspect info`, `text`, `tree`, and everything under `edit` and `publish`.

Everything normalisation chooses or discards is recorded as a `Decision`, which is what
makes the arrangement honest: the byte layer is not the only way to find out what was
lost, it is the way to see the file again.

Metadata joins the rest. `metadata::settle` runs at ingest, records disagreements
between `/Info` and the metadata stream, and moves the entries 14.3.3 deprecates into
the stream where that clause puts them.

## Consequences

- **`inspect catalog` and `inspect info` can disagree about the same file, and that is
  correct.** They answer different questions. Before, which one a command answered was
  an accident of how it had been written.
- **A document that had no metadata stream has one after being loaded.** Six of the nine
  corpus files are in that position. This follows from the policy rather than being a
  defect to fix.
- **`extract_metadata` is a derivation, not a decision.** After settling it returns the
  same answer however often it is asked, which is what lets ADR-0011's fixed point
  extend to metadata.
- **ADR-0012 follows from this** rather than standing beside it. Saving produces a new
  document *because* loading already produced one.
- **`sublime_metadata` is read for the first time**, leaving `color_policy` as the last
  of the two options ADR-0007 hid for being inert.
- **What the model cannot hold is lost at load, with no later stage to recover it.**
  This is the price, and it is not hypothetical: until `2e2b642` the text decoder
  corrupted a conforming `/Title` at load, and the only reason the output was right was
  that the save path happened to overwrite it from XMP. Under this policy that accident
  is gone. Decoding defects are now unrecoverable rather than merely wrong, which raises
  what a change to the reader has to prove.
- **`ARCHITECTURE.md` was accurate stage by stage and silent about the whole.** Every
  bullet in §5.4 was correct; what was missing was the sentence that follows from them
  together, and any mention of the byte layer — four report types that take `&[u8]` and
  appear nowhere in a document whose subject is layering. A document can mislead without
  containing a false statement, and this one did: it was persuasive enough to nearly
  settle this question the other way. §5.4 says both things now, and §5.3 gained the
  second instance of its own rule about decisions that fire on conforming input, because
  settling broke it on all nine samples before the rule was read again.
