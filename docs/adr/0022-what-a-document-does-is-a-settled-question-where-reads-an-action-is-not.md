# ADR-0022: "What can this document do" is a settled question where "reads an action" is not

- **Status**: Accepted
- **Date**: 2026-08-22
- **Commit**: 465ccb2

## Context

[ADR-0019](0019-semantic-understanding-is-measured-against-what-a-corpus-presents.md) kept
clause 12.6 out of the coverage index, and the reason was sound: *"reads an action"* has
no agreed meaning. A `/GoTo`'s destination resolves through the name tree; a `/URI`'s
target is never looked at; a `/Launch` names a program nobody will start. An axis whose
numerator nobody can define is a number that looks like a measurement and is not.

`inspect interactive` therefore counted actions **by `/S`** and stopped there: a file
carries one `/JavaScript`, and nothing about what the script is or when it runs.

Phase O-1 fetched a corpus that presents the constructs. 105 of 524 files carry at least
one action; six are `/JavaScript`, three are `/Launch`, and six name an `/S` no edition
of the standard defines. One of them writes
`/Win << /F (TextPad.exe) /P (status.txt) /D (C:/Programme/TextPad 4) >>` on a `/Launch`
behind a link. Two more file a script under `/Names /JavaScript`, which **nothing points
at** and which runs the moment the document is opened — and the `/S` census does not see
those at all, because they hang off no annotation and no catalogue action entry.

So the question was no longer hypothetical, and it needed a definition that is not
"reads an action".

## Decision

**The settled question is "what can this document do, and does the reader have to do
anything first".** Both halves are answerable from the file without interpretation:

- **What it can do** is the action's *consequence*, not its name. `Capability` has six
  values — runs code, launches another program, reaches outside the document, plays
  media, stays inside the document, and **undefined**. Classifying by consequence means
  two `/S` values this engine has never seen land in the right group if they do the same
  thing.
- **What has to happen first** is the `Trigger`, and the line that matters is
  `Trigger::without_interaction()`: `/OpenAction` and the `/Names /JavaScript` tree fire
  when the file opens; everything else waits for a click, a page turn, a keystroke or a
  print.

**An `/S` the standard does not define is not folded into the harmless group.** It gets
`Capability::Undefined`, and a test asserts it is not `StaysInside`. The corpus carries
`/SetState` and `/NOP`; reporting an unknown as safe is the exact failure a screen exists
to prevent, and it is the cheapest mistake to make.

**Every place an action can hang is walked**, because a screen that misses one is worse
than none: `/OpenAction`, the catalogue's `/AA`, the `/Names /JavaScript` tree, each
page's `/AA`, each annotation's `/A` and `/AA`, each form field's `/AA`, and the `/Next`
chain off any of them. The `/JS` payload is read from a string **or a stream**, and a
`/Launch` target from `/F` **or** from the `/Win`, `/Mac` and `/Unix` dictionaries 12.6.4.6
deprecates — the only `/Launch` in the corpus writes no `/F` at all, so reading the
undeprecated entry alone reports it as launching nothing.

**Reading without running is conforming, and the standard says which sentence makes it
so.** 12.6.4.17 has a `shall`: on invocation a processor executes the script. 6.3.2.1 has
the answer — each PDF processor chooses which subsets of PDF functionality to support and
shall comply for the ones it chose, and PDF 2.0 deliberately abandoned the notion of a
"conforming reader" that the subset standards keep. So declining ECMAScript is a choice
the conformance model provides for, not a gap; what would not be conforming is claiming
the subset and not doing it. `/Requirements` with `EnableJavaScripts` (12.10) is how a
document declares it needs the subset, and reading *that* is the honest interface.

**This does not become a coverage axis.** ADR-0019's argument survives intact: the
denominator would still be undefinable, and adding an axis whose numerator is "we read
`/S` and one payload key" would let the index rise for work that understood nothing more.
`inspect actions` is a *report*, and the index stays at three axes.

## Consequences

- **`inspect interactive`'s action tally is now knowingly narrower than the truth**, and
  says so where it is declared. It is a census of what hangs off the catalogue and the
  annotations; `ActionReport` is the complete walk. Two facts, two homes, with the
  narrower one pointing at the wider.
- **A document-level script is visible for the first time.** The two files that run code
  with no interaction at all were reported by the old census as doing nothing of the kind.
- **The report says what an action *says***, not only that it exists — the script, the
  program, the URL, the named action. That is what makes it screening rather than
  counting, and it is why the payload is truncated for display and complete in the JSON.
- **P4 of "Read broadly, write 2.0" is answered.** P1 (`/Ch`) and P3 (`/DSS`, `/Perms`)
  are not, and cannot be from a corpus: neither occurs in any of the 524 files.
