# ADR-0012: Saving produces a new document, not an edited one

- **Status**: Accepted
- **Date**: 2026-08-16
- **Commit**: the provenance work

## Context

Three questions arrived together and turned out to be one.

*Can a signature survive being written?* No. A signature covers a byte range, and this
engine does not reproduce those bytes.

*Can `/P` survive?* No. Decryption is what makes the objects readable, and `/Encrypt`
cannot remain over plain objects — Acrobat reports error 135 for a file that tries.

*Is the history preserved?* No, and this is the one that settles it. Measured on
`samples/fy05.pdf`:

| | Source | Output |
| :--- | ---: | ---: |
| Cross-reference sections | 3 | 1 |
| Object numbers defined more than once | 13 | 0 |

The revision chain is merged at load — newest definition wins, per ADR-0006 — and the
older versions never enter the arena. By the time a `Document` exists there is one
state, not a history. Nothing later can put it back.

Normalisation compounds it. `samples/fy05.pdf` differs from its own output in 378 of
4,574 objects **with refinement turned off**, and there is no code path that writes a
faithful copy. `write_incremental_update` exists in the writer with no caller, and
wiring it would need a writer that keeps the source bytes and appends — a different
mode from the one that exists, because normalisation leaves no notion of *which*
objects changed.

So the engine cannot preserve anything that depends on the exact bytes, and its output
is not a revision of the input.

## Decision

**Saving produces a new document derived from the input.** Not an edit of it.

This is not a change of behaviour. It is a name for what the engine already does, and
naming it settles what to do about the rest:

- **Signatures are not carried.** A new document does not bear someone else's
  signature, and carrying an invalid one forward would be worse than dropping it.
- **`/Encrypt` is not carried.** Protecting the output is a new decision about a new
  document, not an inheritance.
- **Neither is refused.** `/P` is a declaration and 7.6.4.1 puts obeying it at
  `should`; a signature's absence is a fact about the output, not a violation the
  engine commits.

What replaces the loss is a record of origin. `xmpMM:DerivedFrom` and
`xmpMM:OriginalDocumentID` carry the source's `xmpMM:DocumentID`, or its trailer
`/ID[0]` when it had no XMP. A message on a terminal is gone when the terminal is; a
reader of the output can ask it where it came from without having watched it being
made.

The write path also reports what the source carried and the output does not — its
permissions (7.6.4.2) and its signatures (12.8) — through the `Vec<Decision>` that
`save_*` returns.

## Consequences

- **The engine's capabilities and its claims now agree.** Before, the output silently
  presented itself as the same document with things missing. It is a different
  document, and says so.
- **`fepdf edit rotate a.pdf -o b.pdf` means "make b from a", not "rotate a".** That is
  a real shift in how the commands read, and it is the honest one. An `--in-place`
  operation would contradict this policy directly and should not be added without
  revisiting it.
- **A faithful-copy path remains worth building**, and this decision does not close it.
  It would be a second save mode — keep the source bytes, append an incremental update —
  and with it a signature could survive. Recorded in `ROADMAP.md`; nothing here
  forecloses it.
- **Signing a document fepdf produced is still sound.** There is no prior signature to
  preserve and the bytes are the engine's own. What is not sound is signing a document
  derived from a signed one without saying so — the new signature would stand where
  another's used to, and `xmpMM:DerivedFrom` is what keeps that visible.
