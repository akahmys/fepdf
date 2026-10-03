# ADR-0108: A page keeps what Annex F gives it, where qpdf counts otherwise

- **Status**: Accepted
- **Date**: 2026-10-03
- **Commit**: (see the commit that adds this file)

## Context

ROADMAP Y-F31 held every sample's linearised save against `qpdf --check-linearization`
(qpdf 12.4.2). Two faults in the writer were fixed, and eight samples of ten read clean.
The other two report one kind of warning: a page's object count in the page offset hint
table is higher than qpdf computes.

- `intel_sdm.pdf`, 4,962 pages, mostly by one: each page carries its article bead, which
  the page reaches through `/B` and the catalogue reaches through `/Threads`, the thread's
  `/F` and the beads' `/N` and `/V`.
- `sample_02c.pdf`, page 0, 122 against 36: its thirty widgets and what they draw with,
  which the page reaches through `/Annots` and the catalogue through `/AcroForm /Fields`.

The writer writes these with their page and counts them as the page's. Annex F F.3.10 says
of threads that "the bead dictionaries" are located "with the individual pages", and a
widget is an annotation the page draws. qpdf counts an object some catalogue entry also
reaches as not the page's own, so its count is lower.

## Decision

**The writer follows the standard's text, and the warning is kept.** The owner chose it on
2026-10-03, over counting such objects as shared to match qpdf.

The two warnings are listed in `scripts/test/linearization_known.tsv`, by file and warning,
with the reason. `scripts/test/check_linearization.sh` reports a listed warning as known
and does not count it; any other warning, any error, or the list removed fails it.

## Consequences

- A document with article threads or a form reads with this warning in qpdf. The file is
  linearised as Annex F places its objects; a reader taking qpdf's count reads a page as
  one or a few objects longer than it is.
- The list names files, not documents in general: a new file with the same shape fails the
  script until it is added, which is the point at which someone reads this record.
