# ADR-0019: Semantic understanding is measured against what a corpus presents

- **Status**: Accepted
- **Date**: 2026-08-20
- **Commit**: this record

## Context

`ROADMAP.md` opens with a goal: *an engine that understands ISO 32000-2 semantically —
not merely one that round-trips it.* Every phase beneath it states what *done* means in
terms a run can check. The sentence they all sit under never did, and `status.sh` said
so in its own closing paragraph: no run of it can report the goal true or false, so
"every box is ticked" and "the goal is met" had no relationship to each other.

That is not a small gap. The previous roadmap marked twenty-seven phases complete
against a goal of "the world's most robust and ISO-compliant PDF 2.0 toolkit", and
measurement then found `open_repair` returning without repairing, `ColorPolicy` never
read, and five `edit` subcommands reporting success while writing nothing. A goal
nothing can check is one that ticked boxes will be mistaken for.

Two ways to give it a completion condition.

**Measure against the standard.** Enumerate every construct ISO 32000-2 defines —
Arlington's model is 613 object definitions — and report the fraction the engine reads.
It has a fixed denominator and it rewards building for constructs no file contains,
which is the container-before-contents shape this project has already had to correct
twice (ADR-0017). It also makes the figure move when the *standard* is re-read rather
than when the engine changes.

**Measure against what files actually contain.** The denominator is the set of
constructs the corpora present. Nothing that never arrives can raise or lower the
figure.

## Decision

**Semantic coverage is the proportion of the constructs a corpus presents whose
*contents* this engine reads.** `fepdf-model::coverage` computes it, `fepdf inspect
coverage` reports it, and `status.sh --full` prints it against `samples/` — or against
both corpora when the external one has been fetched, naming which.

Three axes have a denominator the engine can enumerate from a file without judgement:

| Axis | Presented | Read when |
| :--- | :--- | :--- |
| Catalogue entries (7.7.2) | the file carries the key | `Support::Modelled` — the field's type says what it holds |
| Annotation entries (12.5) | an annotation of that subtype writes the key | the subtype's readers name it |
| Stream filters (7.4) | a stream names the filter | `filters::is_decoded` |

Annotation entries are counted **per subtype**, because `/Parent` is read on a `/Popup`
and on nothing else; folding `/Circle /BS` and `/Movie /BS` into one construct would let
a reader for either claim both.

Actions (12.6) are the obvious fourth axis and are deliberately absent: "reads an
action" has no settled meaning here — a `/GoTo`'s destination resolves through the name
tree while a `/URI`'s target is never looked at — and an axis whose numerator is a
judgement call is one the figure can be argued into.

**The property that makes it worth having is that it cannot be raised by building
containers.** `a_construct_no_file_carries_counts_in_neither_direction` asserts it:
`/DPartRoot` has a field, occurs in none of the 251 files of both corpora, and appears
in neither the numerator nor the denominator. The test exists because the failure it
prevents has happened — the catalogue's typed count went 15 → 32 in one session while
the entries whose contents the engine could read moved by one.

## Consequences

The goal line has a number. It is **61% over `samples/`** — 17 of 28 constructs — and
**82% over both corpora**, 190 of 231. Those are the figures the day this was decided;
`fepdf inspect coverage` re-derives them, and Phase K moved them within the week. A phase that reads a new entry moves it without
anyone editing a paragraph, and adding an axis can only lower it.

That the figure went *up* when the corpus grew is the caveat below made concrete, and it
was not the prediction. The external corpus adds 194 annotation constructs, of which
Phase J reads 171, while the axis this engine is weakest on gains four constructs and no
readers at all — catalogue entries go from 5 of 16 to 5 of 20. A weighted total is a
real fact about a real corpus and it is not "how well does this engine do on hard
files". Per axis is where that question is answered, which is why the rows are printed
above the total rather than under it.

**What the number is not**, stated here so it travels with the figure:

- It is a **proxy** for understanding, not a measure of it. It says nothing about
  whether what was read was read *correctly*. Clause 7.6 was ticked while every
  password handler was broken (ADR-0009), and a coverage figure would not have caught
  it; cross-checking against another implementation did.
- It is **bounded by the corpus**. 251 files present 20 of Table 29's 32 catalogue keys,
  16 of ~28 annotation subtypes and 7 filter names. A corpus that presents little
  flatters an engine that does little, which is the argument for fetching more — and the
  reason Phase J wrote down what its corpus could *not* exercise before deciding
  whether to.
- It **weights an axis by the number of constructs it presents**, not by importance.
  Twenty catalogue keys count for more than seven filters because there are more of
  them. Averaging the three percentages instead would let an axis with two constructs
  swing the total as far as one with two hundred.

Reading this figure as the goal itself would be the same mistake as reading "32 of 32"
as the catalogue being understood. It is a floor under the claim, not the claim.
