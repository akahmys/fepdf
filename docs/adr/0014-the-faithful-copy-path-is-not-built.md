# ADR-0014: The faithful-copy path is not built, and signing is limited to output this engine wrote

- **Status**: Accepted; amends ADR-0012
- **Date**: 2026-08-16
- **Commit**: this record

## Context

[ADR-0012](0012-saving-produces-a-new-document.md) settled that saving produces a new
document, and left one thing open on purpose:

> **A faithful-copy path remains worth building**, and this decision does not close it.
> It would be a second save mode — keep the source bytes, append an incremental update —
> and with it a signature could survive.

Asked directly whether that path is needed, three things came out of measuring it.

**It cannot be a second save mode.** `write_incremental_update` exists in the writer
with no caller and takes `changed_handles: &[(u32, Handle<Object>)]` — it requires the
caller to know which objects changed. [ADR-0013](0013-a-document-is-one-normalised-state.md)
is the reason nobody can: by the time a `Document` exists the revision chain is merged,
the ciphertext is gone and the metadata has one answer, so there is no "changed" to
compute against. A path that starts from a `Document` has already lost the file. It
would have to start from the bytes, which makes it a second implementation of the whole
read–modify–write cycle rather than a mode of the existing one.

**Almost nothing this engine does could use it.** Of the five `edit` operations, only
`rotate` is expressible as a minimal incremental update; `merge`, `split`, `tag` and
`repair` are whole-document transformations by construction.

**What byte preservation actually buys is narrower than it looks.** Appending to a
signed document leaves the signed byte range intact, but readers still report the
document as changed since signing, so editing a signed file gains nothing. The one case
it genuinely serves is *adding a first signature to a document this engine did not
produce* — where normalising at load would mean signing something the user never saw.

No corpus file carries a signature: zero `/Sig`, zero `/ByteRange` across all nine.

## Decision

**The faithful-copy path is not built. fepdf signs only documents it wrote itself.**

Signing a file this engine did not produce is a job for a tool that never rewrites it,
and there are such tools. Doing it here would mean carrying a second engine to serve one
operation that is better served elsewhere.

What this does *not* change: signing fepdf's own output remains sound and remains
wanted, because there the bytes are the engine's own and it can compute a byte range
over them without preserving anyone else's. That still needs a CMS layer, as do
public-key security handlers (7.6.5) — but neither needs byte fidelity.

## Consequences

- **ADR-0012's open item is closed rather than carried.** An item nobody can start is
  worse than an item declined with a reason; `ROADMAP.md` moves it to *Not planned*.
- **`write_incremental_update` has no caller and no prospect of one.** It should be
  deleted when someone is confident nothing else wants it — a function kept for a plan
  that has been abandoned is the container-before-contents shape this codebase keeps
  paying for.
- **Existing signatures still do not survive a save**, exactly as ADR-0012 recorded, and
  the write path still says so through its `Vec<Decision>`.
- **The scope question was the useful one.** "Do we need a faithful-copy path" has no
  answer on its own; "can fepdf sign a document it did not produce" has one, and it
  settles the first. The engineering question was downstream of a product question that
  had not been asked.
