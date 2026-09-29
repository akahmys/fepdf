# ADR-0103: Two operations that wrote what no reader reaches are removed

- **Status**: Accepted
- **Date**: 2026-09-30
- **Commit**: (see the commit that adds this file)

## Context

Reading `fepdf-mcp`'s tool descriptions against what each tool does (ROADMAP Y-1c) found
two operations whose output a reader never reaches. Neither was new: both predate the
crate-by-crate cleanup, which did not read them.

**`AddPublicKeyRecipient`** put an `/Encrypt` dictionary into the **catalogue**. 7.6.1
puts it in the trailer. It declared `/V 4 /R 4`, which PDF 2.0 deprecates, and which this
engine does not write ([ADR-0015](0015-this-engine-reads-five-encryption-schemes-and-writes-one.md)).
It stored the recipient's certificate as it was given, in `/Recipients`, where 7.6.5
wants a CMS enveloped-data object carrying the file key for that recipient. Nothing was
encrypted. `fepdf-mcp` described the tool as adding a recipient "for certificate-based
document encryption".

Encrypting to certificates works, and is done by the save path:
`SaveOptions::recipients`, which `crosscheck_pubsec.sh` checks against a second
implementation. Encryption is a property of a written file, not of a document being
edited, which is why it is a save option.

**`AddMeshShading`** put a Type 4-7 shading into a `/Resources /Shading` dictionary of the
**catalogue**, always under the name `Sh0`. The catalogue has no `/Resources` entry
(Table 29), so nothing reaches it. It also carried none of the `/BitsPerCoordinate`,
`/BitsPerComponent`, `/BitsPerFlag` and `/Decode` entries a mesh shading requires
(8.7.4.5.5-8). A shading means something when a page paints it with `sh` or a pattern.
This operation named no page.

The tests beside them asserted the defects. `backend_operations_test.rs` checked that the
catalogue held `/Resources` and `/Encrypt` after the two ran, and two more tests built each
operation's value and read a field back out of it.

## Decision

**Both are removed**, with their specs (`PublicKeyRecipientSpec`, `MeshShadingSpec`,
`MeshShadingType`), their `fepdf-mcp` tools, and the tests that asserted what they wrote.

Neither removal takes away a capability. Encrypting to certificates remains a save option.
Painting a mesh shading was never something this operation did, and an operation that
does it would be a new one that names a page.

Keeping them with honest descriptions was the alternative, and it was rejected. An
operation whose honest description says "writes a dictionary no reader reaches" is not a
vocabulary entry. It would also leave non-conforming structures in the output of a PDF
writer that has chosen to conform (ROADMAP, the subsets).

## Consequences

- The `Operation` vocabulary is two shorter, and `fepdf-mcp` loses two tools. Both are
  breaking changes. Nothing outside this workspace depends on them.
- `SetUnencryptedWrapper`, found in the same state, is kept and rebuilt to 7.6.7. That is
  a separate decision, recorded separately.
