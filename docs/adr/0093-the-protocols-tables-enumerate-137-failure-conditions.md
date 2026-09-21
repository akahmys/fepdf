# ADR-0093: The Matterhorn Protocol's tables enumerate 137 failure conditions, and its prose says 136

- **Status**: Accepted
- **Date**: 2026-09-21
- **Commit**: (see the commit that adds this file)

## Context

`MatterhornAuditor::IN_PROTOCOL` was 136, taken from the protocol's own sentence and
checked against it by a test: "The Matterhorn Protocol is a set of 31 checkpoints
comprised of **136 failure conditions** encompassing file format requirements specified in
PDF/UA-1. 87 failure conditions can be determined by software alone, 47 failure conditions
usually require human judgment. 2 failure conditions have no specific tests (23-001 and
27-001)." [ADR-0092](0092-the-matterhorn-protocol-measures-ua-1-and-this-engine-declares-ua-2.md)
recorded that sentence as the source of the number, and `docs/specs/README.md` repeated
it.

The number is the denominator of every claim this engine makes about how much of the
protocol it checked — "2 of 136" in the accessibility panel, and the scope that W-21a
added so that "no findings" could not be read as "conforms".

Extracting the document's text and counting the **Index** column gave 137.

The gap was measured on 2026-09-21 rather than argued about:

- **The tables enumerate 137.** Reading every line that begins with a failure-condition
  number gives 137 distinct ones, and every checkpoint is numbered from `001` upward with
  no gap: checkpoint 01 has seven, 13 has eight, 28 has eighteen, 31 has thirty. A gap or
  a stray match would show up as a checkpoint whose highest index does not equal how many
  it has, and none does.
- **The `How` column agrees with 137, not 136.** Counting it gives 87 `M` and **48** `H`
  beside the two with no test — one more `H` than the sentence's 47, and the same `M`.
- **Version 1.1's own Document History says which one.** "1.1 2020-11-10 ■ Failure
  condition 13-008 added". 13-008 — "ActualText not present when a `<Figure>` is intended
  to be consumed primarily as text" — is marked `H`, which is exactly the one the `How`
  count is over by.

So the sentence is version 1.02's count, carried into 1.1 with the condition 1.1 added
but without the arithmetic. It is the only place in the document that states a total, and
nothing in the document's own tables supports it.

## Decision

**`IN_PROTOCOL` is 137**, and what it counts is the conditions the protocol enumerates
rather than the sentence that totals them. `AGENTS.md`'s hierarchy puts measurement above
documentation; here both are the same document, and the measurement is of the part a
number can be looked up in.

**The test derives the count instead of asserting it.**
`every_failure_condition_in_the_protocol_is_counted` reads the Index column out of
`docs/specs/Matterhorn-Protocol-1-1.pdf`, requires each checkpoint to be numbered from
`001` without a gap, and compares the total with `IN_PROTOCOL`. A number that is derived
before it is quoted is the third writing rule, and this is what it looks like when the
source is a PDF.

**The prose is asserted too.** The same test requires the document to still say "136
failure conditions" and to still record "Failure condition 13-008 added". Both are the
reason this constant disagrees with the sentence, so an edition that corrects the sentence
fails the test and brings someone back to this record rather than quietly agreeing with
it.

## Consequences

- **Every "N of M" this engine prints moves by one**, in the panel, in the report's scope
  and in the summary. The denominator was too small, so every claim about coverage was
  very slightly too generous.
- **The split to work against is 87 `M`, 48 `H`, 2 with no test.** W-21's target — "136
  minus the two with no test" — is 135, and W-21d's "47 the protocol marks `H`" is 48.
- **This is not a defect in the protocol worth routing around.** One stale sentence in a
  document whose tables are consistent is a normal erratum, and the tables are what the
  rest of this engine cites: a finding names `13-004`, not "one of the 136".
- **`docs/specs/README.md` repeated the sentence** and now states the count with the
  discrepancy beside it, because that file exists to stop this document being got wrong
  from memory.
