# ADR-0059: What holds fy05.pdf below its old reading is a running head in the margin

- **Status**: Accepted (a measurement, not a fix)
- **Date**: 2026-09-05
- **Commit**: (see the commit that adds this file)

## Context

[ADR-0049](0049-the-extraction-backend-was-not-tracking-the-ctm.md) and
[ADR-0050](0050-ruby-is-bound-to-the-base-it-reads.md) left `samples/fy05.pdf` at **11
agreeing pages of 846 and 3.6% prefix agreement**, against a best of 45 before ADR-0047's
sort, and recorded the cause as unmeasured. This measures it.

**585 of its 846 pages hold the same characters in a different order.** The pages that
diverge earliest are every odd page in a long run — 5, 7, 9, 11, 13 — and they agree on
**nothing at all**: the first character is already wrong.

**It is the running head.** `目次` is two characters, and this engine puts them 75
characters apart with a sentence between them:

| page | our `目` / `次` | PDFKit |
| ---: | :--- | :--- |
| 5 | 76 and 151 | 0 and 1 |
| 6 | 112 and 193 | 1465 and 1466 |
| 7 | 73 and 146 | 0 and 1 |

**The head is a column in the outer margin, and the margin is mirrored:**

| page | body `x` | head `x` |
| ---: | :--- | ---: |
| 5, 7 (odd) | 88–498 | **68**, outside to the left |
| 6, 8 (even) | 108–500 | **520**, outside to the right |

`目` sits at `y=715.48` and `次` at `683.48` — one column, 32 units apart at 8pt — while
body lines are about 19 apart. Each character therefore falls between two body lines and is
clustered as a line of its own, and the two are emitted a body line apart. PDFKit reads the
head first on the odd pages and last on the even ones, which is what a mirrored margin
means; the two readers would agree without either copying the other.

## Decision

**Nothing is changed.** The obvious fix was tried and reverted, and the reason it failed is
the point of this record.

**The head is not vertically set.** The first attempt partitioned the runs by
`ExtractedRun::is_vertical` — keeping vertical runs out of the body's `y` sort and placing
each column before or after the body by which side of it they sit — on the reading that a
column of characters is vertical text. It is not: only **25 of 846 pages** carry a run whose
font declares writing mode 1, and pages 5 to 8 are not among them. The head is a horizontal
font placed a character at a time down the margin.

So the partition never saw the head, and on the 25 pages it did reach it moved prefix
agreement from **3.6% to 3.4%**. Reverted.

## Consequences

**What would find it is geometry, not writing mode**: a line holding one short run whose `x`
lies outside the range the rest of the page occupies is a running head or a folio, wherever
its characters sit in `y`. That rule has to be written carefully — the marginal line must
not be counted when working out what "the rest of the page" spans — and it would apply to
every one of the 7,727 pages the crosscheck covers, where the current numbers are the
floor.

**The 250 pages that differ in *content* are a separate matter** and are not extraction
order at all. They belong to the loss the roadmap tracks.

**The measurement is the deliverable.** ADR-0050 said fy05's cause was "something else, and
unmeasured". It is now measured, with the mechanism named and the failed fix recorded so
that the next attempt does not begin by making it again.
