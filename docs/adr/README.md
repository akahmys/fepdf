# Architecture Decision Records

One file per decision that was **contested, reversed, or rests on a measurement**.
Not every choice: a decision whose alternative is obviously worse does not need a
record, and a log padded with those stops being read.

## When to write one

- A decision was made, then **measurement contradicted it**. Record both, so the
  reasoning that led there is visible and not repeated.
- Two defensible options exist and one was chosen. Record why, so the question is
  settled rather than relitigated.
- A constraint is being accepted deliberately — a dependency, a tolerance, a gap.

Ordinary implementation choices belong in code comments and commit messages.

## Format

```
# ADR-NNNN: <the decision, as a statement>

- **Status**: Accepted | Amended by ADR-NNNN | Superseded by ADR-NNNN
- **Date**: YYYY-MM-DD
- **Commit**: <sha of the change that implemented it>

## Context
What was true, and what question had to be answered.

## Decision
What was decided.

## Consequences
What follows, including what is now harder.
```

Keep each under a page. If it needs more, the design belongs in `ARCHITECTURE.md`
and the ADR should point at it.

## Relationship to other documents

`ARCHITECTURE.md` describes the architecture **as it is now**. These records describe
**how it came to be**, including paths not taken. When the two disagree,
`ARCHITECTURE.md` is authoritative for the present and the ADR is authoritative for
the history.

Note that `Decision` in `fepdf-model` is a different thing entirely: it records what
the *engine* decided about a non-conforming input file at run time
(`ARCHITECTURE.md` §5.3).

## A note on the first five

ADR-0001 through ADR-0005 were written **retroactively**, reconstructed from the
commits that implemented them. They were not written at the time the decisions were
taken — which is the reason this log exists, since in four of the five the original
reasoning had to be recovered from a diff rather than read.

ADR-0006 is the first written as the decision was taken, with the measurement that
forced it still to hand.
