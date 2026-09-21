# ADR-0094: The auditor reads the ingested document, so ingestion answers checkpoint 06

- **Status**: Accepted
- **Date**: 2026-09-21
- **Commit**: (see the commit that adds this file)

## Context

W-21b adds the failure conditions that need no new machinery, and checkpoint 06 looked
like the easiest of them. Two of its four are `Doc M` and are properties of the catalogue
this engine already models:

- **06-001** — "Document does not contain an XMP metadata stream."
- **06-003** — "XMP metadata stream does not contain `dc:title`."

Both were written, and both were wrong in the same way. `MatterhornAuditor` was given the
document, asked `catalog().metadata`, and reported. The test fixture states no
`/Metadata` at all, and 06-001 came back **sound**.

`metadata::settle` runs at ingest. It reads `/Info`, reads any XMP packet, overlays the
two, and then **writes the packet back into the catalogue** — `update_xmp_metadata(doc,
&settled)`, after which `migrate_deprecated_info` may empty `/Info` of what has moved.
That is the right thing for the engine to do and the reason a document is one normalised
state ([ADR-0013](0013-a-document-is-one-normalised-state.md)); it is also the reason
neither condition can be asked here.

Measured on 2026-09-21, through `audit_ua2_report`:

| The file on disk | What the auditor said |
| :--- | :--- |
| No `/Metadata`, no `/Info` | 06-001 **sound** — ingestion had written the packet |
| No `/Metadata`, `/Info` with a `/Title` | 06-003 **sound** — ingestion had promoted the title to `dc:title` |

The second is the worse of the two. A file whose only title is in the dictionary PDF 2.0
deprecates is a file that breaks 06-003, and this engine repaired it on the way in and
then reported it as conforming.

## Decision

**Checkpoint 06 is not in `CHECKED`.** A condition whose answer was written by this
engine's own ingestion is a check that cannot fail, and a check that cannot fail is what
Phase W-21 has spent itself removing — from the "100% Compliant" label, from
`unwrap_or_default()` in the worker, and from `found_nothing()`. Claiming it would be the
same defect in a new place.

**The reason is held to the code by a test.**
`checkpoint_06_is_left_out_because_ingestion_answers_it` asserts that the two numbers are
absent from `CHECKED`, that the fixture's bytes state no `/Metadata` — read through
`CatalogReport::survey`, which reads the file rather than the ingested document — and that
the ingested document has one anyway. The day ingestion stops writing the packet, the test
fails and the two conditions become checkable.

**What would make them checkable is naming what is being audited.** A Matterhorn finding
is a statement about a *file*; this auditor's subject is a *document*, and the two differ
by everything ingestion repairs. Auditing the file means reading it the way
`CatalogReport::survey` does — before `settle` — which is machinery W-21b does not have
and does not pretend to.

## Consequences

- **W-21b reports ten failure conditions, not twelve.** 01-007, 07-001, 07-002, 11-002,
  13-004, 14-002, 14-003, 14-007, 17-002 and 28-005.
- **Checkpoint 06 belongs to a later item**, along with any other condition about what a
  file states rather than what a document holds. 26-001 and 26-002 are the same shape:
  decryption drops `/Encrypt`, so the encryption dictionary is gone from the document
  before anything could ask about its `/P`.
- **This is a question to ask of every condition added from here.** "Can this engine
  change the answer between reading the file and running the audit?" — and where it can,
  the condition is about the file and needs the file.
- **`XmpMetadata` did not gain a `parsed` flag.** One was written, to tell "the packet
  says no `dc:title`" from "the packet would not parse", and it went with 06-003: an
  entry nothing reads is a comment with a type on it.
