# ADR-0029: Halftones and transfer functions are declined on their own clauses, not on the corpus

- **Status**: Accepted
- **Date**: 2026-08-22
- **Commit**: (this change)

## Context

`ROADMAP.md` Phase P held halftones open as a question about evidence:

> **Halftones (10.6) have no code at all.** Whether that matters is a question for a
> corpus that has not been asked: no file has been surveyed for `/HT` in an `ExtGState`,
> and that survey is the first step rather than the implementation.

That framing has a trap in it, and the same list names the trap two hundred lines earlier:
**a corpus is grounds for building and never grounds for declining**, because a capability
that does not exist has no users. Phase L exists as the record of learning that. So the
survey could tell us to build and could not, on its own, tell us not to.

The survey was run anyway, because "few files carry it" and "the survey is broken" print
identically and one of them is worth knowing. `crates/fepdf/examples/survey_extgstate.rs`
walks every dictionary the arena holds — not `grep`, because an `ExtGState` is usually
inside an object stream — across all 524 files of both corpora.

| key | files | what they are |
| :--- | ---: | :--- |
| `/HT` | 2 | both `isartor-6-5-3-t04-fail-*`, each a 60 lpi / 45° / round-dot printing screen |
| `/TR` | 4 | all `isartor-6-2-8-t01-fail-*` |
| `/TR2` | 5 | three isartor, two real documents — both `/Default` |

Every isartor file carries the key **because the PDF/A-1 clause it is named for forbids
it**; they are conformance-failure fixtures. The only two real documents set `/TR2` to
`/Default`, which is the device default and nothing to apply. Not one file in either
corpus specifies a transfer function or a halftone that would change a pixel.

**That is corroboration and it is not the argument.** The argument is in the clauses.

## Decision

**10.6 is declined because 10.6.1 exempts this class of device.** Its second paragraph:

> Some output devices can reproduce continuous-tone colours directly. Halftoning is not
> required for such devices; after gamma correction by the transfer functions, the colour
> components shall be transmitted directly to the device.

This engine renders to an 8-bit RGBA raster through Vello. It is such a device. Halftoning
is a process for approximating continuous tone where the device cannot produce it, and
there is nothing here for it to approximate.

**10.5 is declined because 3.15 says to.** The same sentence establishes that transfer
functions *do* apply to a continuous-tone device, so 10.5 does not fall with 10.6. It
falls on its own: `/TR` and `/TR2` are "deprecated in PDF 2.0", and the standard defines
the term in its definitions clause —

> **deprecated**: a part of ISO 32000 that should not be written into a PDF 2.0 document,
> and **should be ignored by a PDF processor**

— so ignoring them is conformance rather than a gap. The only other route to a transfer
function is an entry inside a halftone dictionary, reached through `/HT`, which is moot
for the reason above.

**No `Decision` is recorded on encountering either.** A file carrying `/HT` is conforming
and ignoring it is correct here, so a departure recorded against it would be exactly the
false positive [ADR-0008](0008-an-indirect-length-is-not-an-ambiguity.md) and
[ADR-0028](0028-four-of-the-thirteen-logs-were-not-decisions.md) were both written about.

## Consequences

Two clauses close without code, and the reason is quotable from the standard rather than
from a file count — which is the distinction Phase L's rule turns on. If this engine ever
acquires a genuinely halftoning output path, 10.6.1's exemption stops applying and the
decision reopens on the same sentence that closed it.

**Phase P said the function evaluator would be "the floor 10.5's transfer functions and
10.6's halftones would stand on". That was wrong about both.** The evaluator earned itself
on 8.6.6 tint transforms and 8.7.4 shadings, which are the two things that were actually
broken; the two clauses it was justified by turn out not to need it. The justification was
weaker than the thing it justified, which is only visible now because the clauses were
read after the code was written rather than before.

**The survey found a defect in surveys.** An arena walk double-counts: `commit_to_arena`
allocates a *new* dictionary for every refined one while the parsed original stays, so a
live dictionary is typically held twice — verified against a control file built with
exactly one `ExtGState`, which reports two. Per-file answers are unaffected, which is why
the survey reports files and labels its dictionary totals an upper bound. The first
predicate was also wrong in the other direction, matching 834 image XObjects through
`/SMask` and a run of annotations through `/CA`; the tell was `/Width` and `/Height`
appearing 834 times in a tally of graphics-state keys.
