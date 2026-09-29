# ADR-0104: The writer makes unencrypted wrapper documents, to 7.6.7

- **Status**: Accepted
- **Date**: 2026-09-30
- **Commit**: (see the commit that adds this file)
- **Rests on**: [ADR-0103](0103-two-operations-that-wrote-what-no-reader-reaches-are-removed.md)

## Context

`SetUnencryptedWrapper` was found in the same state as the two operations ADR-0103
removes. It embedded the caller's bytes as `EncryptedPayload.pdf` with `/AFRelationship
/Unspecified`, and nothing else. 7.6.7 describes a wrapper document as one whose producer
shall include:

- a `/Collection` making the payload the initial document, with `/View /H`;
- the payload's file specification in the `/EmbeddedFiles` name tree and in the
  catalogue's `/AF`;
- `/AFRelationship /EncryptedPayload` on that file specification;
- an encrypted payload dictionary (Table 28), whose `/Subtype` names the cryptographic
  filter.

It also requires that `/EmbeddedFiles` hold exactly one entry.

The operation did none of these, so it produced a PDF with an attachment, not a wrapper.

Whether to make wrapper documents at all was a question about which subsets this
processor chooses (6.3.1). The ROADMAP table is where those choices are recorded, and the
table did not say. Removing the operation and building it properly both produce
conforming output. Only keeping it as it was did not. The choice is the project owner's.

## Decision

**Making unencrypted wrapper documents is part of the PDF writer subset**, the project
owner's decision on 2026-09-30, and the ROADMAP table says so.

`SetUnencryptedWrapper` meets every `shall` above. The spec gains the three things the
clause needs from the caller: the payload's name, the filter's name (Table 28's `/Subtype`),
and optionally its version. The operation refuses, naming the clause, when:

- the payload has no `%PDF-` header;
- the filter is not a name;
- the version is not integers with periods between them;
- the document already embeds a file or is already a collection.

The notice for a reader without the filter goes in the payload's `/Desc`. 7.6.7 says a
wrapper *should* guide the user, and drawing that notice onto the wrapper's pages is left
to the caller's own content.

The payload is not decrypted or checked beyond its header. It is encrypted by a handler
this standard does not define, and that is the reason a wrapper exists.

## Consequences

- A test reads every one of the clause's requirements back from a saved and reopened
  file, and fails against the old operation.
- `AFRelationship` gains `EncryptedPayload`, a value of Table 43 it lacked.
- The spec's shape changes, and so does the `set_unencrypted_wrapper` tool, which now
  takes `crypto_filter` and `filter_version`. These are breaking changes, confined to this
  workspace.
- Reading a wrapper, which means opening its payload when the filter is at hand, is not
  part of this decision.
