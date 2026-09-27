# ADR-0098: An `H` condition is decided only where the document answers it, and W-21 closes with thirteen for a person

- **Status**: Accepted
- **Date**: 2026-09-28
- **Commit**: (see the commit that adds this file)
- **Amends**: W-21's target in `ROADMAP.md`, which [ADR-0092](0092-the-matterhorn-protocol-measures-ua-1-and-this-engine-declares-ua-2.md) framed

## Context

W-21 set its target as the protocol's 137 failure conditions less the two with no test,
reading the protocol's `How` column as advice: an `H` is "not determinative", so software
may decide one, provided the finding says a machine decided it.

By W-21u the auditor decided every `M` condition ingestion does not answer: seventy-seven.
W-21v to W-21x then took thirty-five of the forty-eight `H` conditions, all by one test.
Each is a person's question *about a thing* — whether an action flickers, whether a figure's
`/ActualText` would serve better as `/Alt`, whether a table's header cells are tagged as
headers — and a document with none of the thing has no such question. Where a document has
some, the auditor reports how many, for a reader. One, 31-010, the document settles itself:
a font program's `OS/2.fsType` states whether it may be embedded.

The thirteen left — 04-001, 06-004, 09-001, 13-003, 13-007, 14-001, 14-005, 16-003,
17-001, 18-001, 19-001, 19-002 and 24-001 — ask whether *content* is something (a
heading, a list, a formula, a caption, a note, a reference, a header, a non-interactive
form, information carried by colour) or is in the right order. Whether a document has any
of the thing cannot be known without reading the content, so the test has nothing to
decide. 06-004 asks of a `dc:title` that ingestion rewrites
([ADR-0094](0094-the-auditor-reads-the-ingested-document-so-ingestion-answers-checkpoint-06.md)).

## Decision

**An `H` condition is decided by a machine only where the document gives the answer** —
it has none of what the condition is about, or states a fact that settles it — and the
finding says "Decided by the machine" and why. Where the document has the thing, the
finding is for a reader and names how much of it there is.

**W-21 closes at a hundred and twelve decided, thirteen for a person, ten answered by
ingestion, and two the protocol gives no test.** A heuristic could point at candidates for
the thirteen — a large bold line not tagged as a heading — but it could not make a finding
sound, and a finding for a reader that every document with content receives says nothing
the scope's list does not.

## Consequences

- **The target in `ROADMAP.md` is restated**, and W-21's box is checked; W-22 no longer
  waits on it.
- **No whole document reports nothing for a reader.** A document stating a language leaves
  11-007, whether the language is the right one; one stating none leaves 11-006. That is
  the protocol's shape, and `found_nothing` says so.
- **A heuristic for the thirteen is a new item**, if a use case asks for one: it would add
  pointers for a reader and decide nothing.
