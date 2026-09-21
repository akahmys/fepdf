# ADR-0092: The Matterhorn Protocol measures PDF/UA-1, and this engine declares PDF/UA-2

- **Status**: Accepted
- **Date**: 2026-09-21
- **Commit**: (see the commit that adds this file)

## Context

`MatterhornAuditor` is 168 lines reporting three failure conditions, behind an
`audit_ua2` whose doc comment said "Performs a full UA-2 structural audit".
`PdfStandard::UA2` writes a conformance claim for ISO 14289-2 into the catalogue.

Three things were measured on 2026-09-21, after the protocol was read rather than
remembered.

**The Matterhorn Protocol is a PDF/UA-1 document.** Its own text: "The Matterhorn
Protocol is a set of 31 checkpoints comprised of 136 failure conditions encompassing file
format requirements specified in **PDF/UA-1**." `PDF/UA-1` appears twelve times in it and
`ISO 14289-1` twice; `PDF/UA-2` and `ISO 14289-2` appear **not at all**. There is no
Matterhorn 2.0.

**PDF/UA-2 is not a revision of PDF/UA-1.** ISO 14289-1:2014 is 25 pages on PDF 1.7;
ISO 14289-2:2024 is 51 pages on PDF 2.0, and is not backward compatible with it.

**All three numbers were wrong.** They were written from memory, and each named a
different defect:

| What the code checks | What it claimed | What that number means |
| :--- | :--- | :--- |
| Numbered heading levels are skipped | 14-001 | Headings are not tagged — and `Doc H`, needing human judgment |
| A `Figure` has no `/Alt` | 13-001 | Graphics objects are not tagged with a `<Figure>` tag |
| A structure element names a page that is not there | 01-002 | Real content is marked as artifact |

A wrong number is worse than a missing check. A missing check is silence; a wrong number
is a finding filed against a different defect, and a reader or a tool that looks it up is
told something untrue.

## Decision

**The numbers are the protocol's, checked against the protocol.** The two conditions this
engine does check are 13-004 and 14-003, and a test reads
`docs/specs/Matterhorn-Protocol-1-1.pdf` and requires each number to be followed, in the
document's own words, by the condition it is used for. The same test requires the
protocol to still say "136 failure conditions" and "specified in PDF/UA-1".

**A check with no failure condition is not given one.** The dangling-page check matched
nothing in the protocol, because a broken reference is not a way to fail PDF/UA-1. It was
removed rather than renumbered to whatever was nearest.

**Matterhorn does not measure a UA-2 claim.** What it measures is UA-1, and the report
says how much of it was looked at (2 of 136) rather than answering "no findings".

**For PDF/UA-2 the source is ISO 14289-2 itself**, which is in `docs/specs/` at no cost
through the PDF Association's sponsored access. veraPDF's validation profiles (CC BY 4.0)
formalise each of its "shall" statements as a rule named by clause — `PDFUA-2.xml` carries
91 — and are a second reading to check against, the way `fepdf-render` is a second reading
of where a run is drawn. They are not the source.

## Consequences

- **`audit_ua2` is measuring UA-1 conditions** until the UA-2 work lands, and the scope
  in its report is what says so. Naming it honestly is W-21.
- **Adding a condition means reading the protocol**, because the test compares the number
  to the document. That is the cost, and it is the point: the numbers were wrong for as
  long as nothing compared them.
- **The protocol is untracked**, like every other `.pdf` here, so the test says where to
  get it rather than passing when it is absent.
- **87 of the 136 can be determined by software, 47 usually require human judgment, and
  2 have no specific test** (23-001 and 27-001).

  **The protocol is careful about what that split is**, and it is weaker than it looks:
  "While **not determinative** the value of *How* generally indicates the realistic
  best-practice approach **at the present time**. Some checkpoints may always be decided
  by M (Machine), some usually or probably require H (Human) interaction." It is advice
  with a date on it, not a statement about what software can do — so 87 is not a ceiling,
  and an `H` is not a prohibition.

  What it does mean is that reporting an `H` condition **as decided** is a claim the
  protocol does not support. 14-001 was the wrong number to use because it names a
  different defect, not because it is `Doc H`.
