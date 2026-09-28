# ADR-0101: A PDF Declaration is the caller's statement, and W-22 closes

- **Status**: Accepted
- **Date**: 2026-09-28
- **Commit**: (see the commit that adds this file)
- **Rests on**: [ADR-0099](0099-the-xmp-packet-carries-what-the-engine-does-not-write.md)

## Context

W-22 listed eight things a well-tagged file needs that no operation wrote. The claim
itself was left for last: a PDF Declaration whose `pdfd:conformsTo` names WTPDF's reuse or
accessibility level (WTPDF 6.1). The reason given was that a conformance claim is worth
what the checking behind it is worth.

This engine has no WTPDF checker. Its auditor measures the Matterhorn Protocol, which is
PDF/UA-1's ([ADR-0092](0092-the-matterhorn-protocol-measures-ua-1-and-this-engine-declares-ua-2.md)).
Much of what WTPDF adds is a person's judgement about content, in the same way as the
thirteen `H` conditions ADR-0098 leaves to a person: whether a heading is a heading, and
whether the nesting follows the document.

ISO 14289-2 7.2.2 describes a PDF Declaration as the author's statement attesting to
conformity.

## Decision

**`DeclareConformance` writes the caller's statement and checks nothing.** It adds the
URI it is given to the document's `pdfd:declarations`, beside those already there, which
keep their `pdfd:claimData`. Declaring one twice declares it once. The operation, the
`declare_conformance` tool and `fepdf_model::declarations` all say that the statement is
the caller's.

The alternative was to gate the accessibility level on the UA-2 audit. That would have
made a Matterhorn result stand for a WTPDF claim, which is a different standard's
question, and gated the reuse level on nothing.

**W-22 closes.** Each of the eight is an operation, and the declaration a file carries
survives a save (ADR-0099). A WTPDF checker is not part of W-22, and nothing here claims
one.

## Consequences

- A caller can make a well-tagged file with this engine and say that it is one. Whether
  the statement is true is the caller's to know.
- `fepdf_model::declarations::declared` reads what a document declares, so a later check
  can ask what is claimed before judging it.
