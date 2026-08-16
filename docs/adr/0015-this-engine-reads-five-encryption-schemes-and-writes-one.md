# ADR-0015: This engine reads five encryption schemes and writes one

- **Status**: Accepted
- **Date**: 2026-08-17
- **Commit**: this record

## Context

Clause 7.6's standard security handler has accumulated five password-based schemes
across the editions: RC4 at 40 and 128 bits (`/V` 1 and 2), RC4 under a crypt filter
(`/V 4 /CFM /V2`), AES-128 (`/V 4 /CFM /AESV2`), and AES-256 at revisions 5 and 6
(`/V 5 /CFM /AESV3`). This engine reads all of them, and has a fixture for each built by
an independent Python implementation.

Writing was a different question, and it arrived with the flag that claimed to do it.
`--encrypt-password` produced a plaintext file: nothing called `set_security_handler`,
so `encrypt_stream` was unreachable. Implementing it meant choosing which of the five to
produce, and the obvious answer — "whichever the input used" — is wrong for a reason
worth writing down.

## Decision

**Output is AES-256 at revision 6, and nothing else.**

Two clauses settle it between them. Output is always PDF 2.0 ([ROADMAP](../../ROADMAP.md)
states this as the engine's premise), and 7.6.4.1 in that same edition deprecates RC4
and the Algorithm 2 key derivation that the pre-R6 revisions rest on. Writing a scheme
the standard tells readers to stop trusting, into a file that declares itself conformant
to the edition that deprecated it, puts a weaker guarantee behind a newer number.

Revision 5 is excluded on narrower grounds: it is Adobe's original extension, and its
Algorithm 2.A hashes the password once where revision 6's Algorithm 2.B is deliberately
serial and therefore expensive to attack in parallel. Both are readable here. Only one
is worth producing.

Preserving the input's scheme is not an option for a further reason. [ADR-0012] settled
that saving produces a new document, and [ADR-0013] that a `Document` is one normalised
state: by the time anything is written, the ciphertext is gone and the file key with it.
There is nothing to preserve, only a scheme to choose.

## Consequences

- **Reading and writing are deliberately asymmetric**, and the asymmetry is the point.
  A reader that refuses old files is useless; a writer that produces them is harmful.
  This is the same shape as "read 1.7, write 2.0", one clause further down.
- **A document encrypted by this engine cannot be opened by a reader that predates
  PDF 1.7 extension level 3.** That is the cost, and it is the cost of every AES-256
  file, not something this choice adds.
- **`--permissions` became implementable**, because `/P` needs an `/Encrypt` to live in.
  It had been hidden under a comment naming exactly that blocker.
- **The `/Encrypt` dictionary is written outside the object serialiser**, as the
  signature `/Contents` already was. 7.6.2 exempts its strings from encryption and it
  has to: `/U` is what a password is checked against, so encrypting it under the key
  that password unlocks would leave nothing able to open the file.
- **Public-key handlers (7.6.5) inherit less than the shared word suggests.** They wrap
  a seed in CMS `EnvelopedData`; nothing here builds or reads that structure. What they
  inherit is the dependency and `SigningIdentity`.

[ADR-0012]: 0012-saving-produces-a-new-document.md
[ADR-0013]: 0013-a-document-is-one-normalised-state.md
