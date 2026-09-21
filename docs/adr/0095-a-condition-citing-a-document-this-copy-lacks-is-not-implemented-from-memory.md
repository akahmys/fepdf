# ADR-0095: A failure condition citing a document this working copy lacks is not implemented from memory of it

- **Status**: Accepted
- **Date**: 2026-09-22
- **Commit**: (see the commit that adds this file)

## Context

W-21c names three things: whether a marked-content sequence has a structure element,
whether content outside one is an artefact, and **whether a table's cells are in a row**.
The first two are checkpoint 01's 01-003, 01-004 and 01-005, and they are decided by
reading a content stream. The third is 09-004, and its text is:

> A table-related structure element is used in a way that does not conform to the syntax
> defined in ISO 32000-1, Table 337.

`docs/specs/` holds ISO 32000-2 twice, PDF/UA-1, PDF/UA-2, WTPDF, the four extensions and
the Matterhorn Protocol. It does not hold **ISO 32000-1**, and `docs/specs/README.md` —
the file whose job is to say which documents a working copy needs — did not say that it
should.

ISO 32000-2's own table for these types is Table 371, "Table standard structure types",
and it is prose rather than a grammar: `TR` is "a row of table header cells (`TH`) or
table data cells (`TD`) in a table", `THead` is "a group of `TR` structure elements", and
a `Caption` "shall be either the first or last child of the `Table` structure element".
Those sentences support catching a `<TD>` outside a `<TR>`. They are not Table 337, and
09-004 is a condition about Table 337.

09-004 is not alone. Read out of the protocol on 2026-09-22, the failure conditions whose
text cites a table or clause of ISO 32000-1 are **09-004** (Table 337), **09-005**
(Table 336), **09-006** (Table 333), **09-007** and **09-008** (Table 338), **31-006** and
**31-008** (Table 118), and **31-027** (Annex D); **02-001** and **10-001** cite it in
their notes. All of the first eight are marked `M`.

## Decision

**09-004 is not implemented in W-21c**, and neither is any other condition whose text
cites a document this working copy does not hold. Writing one from ISO 32000-2's prose
and reporting it under a number that names Table 337 would be a finding filed against a
requirement nobody here has read — which is
[ADR-0092](0092-the-matterhorn-protocol-measures-ua-1-and-this-engine-declares-ua-2.md)'s
defect with a different surface. The three numbers there were wrong because they were
cited from memory; a rule can be cited from memory just as easily as a number.

**`docs/specs/README.md` says ISO 32000-1 is missing, and which conditions want it.** That
file exists so this document set is not got wrong from memory, and a needed document it
does not list is the same gap one step earlier.

**Reporting broken without being able to report sound is not the way round it.** The
auditor can express a condition that is reported but never called sound — an unreadable
page does exactly that for 01-005. Using that shape here would put a permanent
half-check in the scope: a `<TD>` outside a `<TR>` caught, and no answer at all for a
document that has none. The conditions stay out until the syntax they name can be read.

## Consequences

- **W-21c lands two thirds of what it names.** 01-003, 01-004 and 01-005 are checked;
  09-004 moves to an item of its own with the other seven, and that item's first step is
  obtaining ISO 32000-1.
- **ISO 32000-1 is not free the way the rest of `docs/specs/` is.** The PDF Association's
  sponsored access covers the ISO 32000-2 bundle and the PDF/UA bundle; ISO 32000-1:2008
  is superseded and is not in either. Where it comes from is the open question this item
  starts with, and it is why this is a decision rather than a task.
- **The question generalises.** Before a condition is implemented, its text is read for
  what it cites, and a citation this copy cannot follow is a reason to stop — alongside
  ADR-0094's question, of whether this engine has already changed the answer.
