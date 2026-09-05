# ADR-0050: Ruby is bound to the base it reads, because where it sits relative to it is not fixed

- **Status**: Accepted
- **Date**: 2026-09-01
- **Commit**: (see the commit that adds this file)

## Context

[ADR-0049](0049-the-extraction-backend-was-not-tracking-the-ctm.md) left two files below
the reading they had before ADR-0047's sort: `bokutokitan.pdf` at 4 agreeing pages against
93, and `fy05.pdf` at 11 against 45. Both are vertical Japanese, and the cause is ruby.

Measured on `bokutokitan.pdf` page 11: the body is set at 10.6pt in columns 17.71 apart,
and each gloss sits **7.97 to the right of the column it reads, at 5.3pt** — three quarters
of a body em across, half the size. The column-clustering tolerance is
`(size * scale * 0.4).clamp(2.0, 6.0)`, at most 6, so every gloss formed a column of its
own. Columns are emitted right to left, so **every gloss on the page came out ahead of the
prose it annotates** and `まもり` left the `守` it belongs to.

**Folding the gloss back into its column is not enough, because `y` does not order them.**
All three arrangements occur on that one page:

| gloss | base | where it sits |
| :--- | :--- | :--- |
| `まもり` | `守` | 2.65 **above** |
| `はと` | `鳩` | at **exactly** the same `y` |
| `や` | `谷` | 2.66 **below** |

Sorting by descending `y` gets the first right and the other two backwards, and no
tie-break rescues a gloss that is genuinely lower than its base.

**And a gloss does not always arrive whole.** The same page emits `どうぬき` as one run and
`ひとえ` as three — `ひ`, `と`, `え`, 6.64 apart. Bound one at a time, each went to its own
nearest base and the page read `ひ単衣とえと` where the file says `ひとえ単衣と`.

## Decision

**A ruby column is folded into the column it annotates, and each gloss takes the `y` of the
base it reads.** Detection is by proportion rather than by any absolute: a column set at
0.6 or less of the size of the column to its left, no more than 1.2 body em to its right.
The smaller-size-first tie-break in `format_vertical_column` then places the gloss ahead of
its base, all three arrangements having become the same case.

**A gloss is bound whole.** Runs closer together than twice their own size are one gloss
and take the base nearest the first of them. On that page the next gloss along is 17.28
away against a within-gloss step of 6.64, so the two populations do not meet.

## Consequences

`bokutokitan.pdf` page 11 agrees with PDFKit for **332 of its 389 characters**, from 59.

**The check could not see any of that, and now can.** The identical / order-only / content
columns are per page, and one misplaced running head keeps a page out of the identical
column however much of it is right — so the whole of this work moved
`crosscheck_reading_order.sh` by exactly nothing. It carries a **prefix column** now: how
far the two readers agree before they first part, over all pages of a file.
`bokutokitan.pdf` reads **8.7% against 1.2%**, and that figure has a floor like the others.
A measurement too coarse to see the work is a measurement that would have let it regress.

**What is left there is the running head, and PDFKit is not obviously right about it.** The
remaining divergence on page 11 is where `濹東綺譚` lands, and PDFKit puts it *inside a
word* — `勘定をす濹東綺譚るついで`. Chasing exact agreement past this point would be
chasing the second reader rather than the document.

**`fy05.pdf` is unmoved at 3.6%**, because its vertical pages carry no ruby; whatever holds
it below its own best of 45 is something else, and unmeasured — **measured since, in
[ADR-0059](0059-what-holds-fy05-below-its-old-reading.md)**: a running head in the outer
margin, split across body lines.

**Each of the four parts has a check that fails when it is removed**, verified by removing
each in turn — the fold, the size tie-break, the binding, and the grouping. Two of the
tests are corpus-free, which matters here: `.gitignore` excludes `/samples/`, so the
crosscheck cannot run on a fresh clone at all.
