# ADR-0009: `/P` is thirty-two bits, and reading it as a positive integer destroyed the content

- **Status**: Accepted
- **Date**: 2026-08-16
- **Commit**: the start of Phase C

## Context

Phase C opens on clause 7.6, the area `ROADMAP.md` calls the weakest. Measuring it
before designing anything found something worse than the documented gap.

`samples/unicode_16.pdf` is the only encrypted file in the corpus: V4/R4, `/CFM
/AESV2`, empty user password. Everything the engine could check about it passed. It
opened. It reported 1,140 pages — the same count PDFKit reports. Its object counts
matched. `publish upgrade` succeeded on it, as it does on the other eight.

It extracted **no text at all**, from any page.

| Reader | Characters over the first 200 pages |
| :--- | ---: |
| macOS PDFKit | 460,648 |
| fepdf | **0** |

The engine was not reading a mostly-correct document. It was decrypting every string
and stream to noise, then reporting the noise as 30,343 font problems — a
`Violation` per stream that failed to inflate, which read as "this file has bad
fonts" rather than "nothing was decrypted".

Worse, `publish upgrade` wrote that noise out. The result was a structurally valid,
unencrypted PDF whose content was destroyed. PDFKit opens it, reports 1,140 pages, and
finds zero characters. Every internal check the project has said this was a success.

## The defect

`/P` in that file is written `4294966260`. Those are the 32 bits `0xFFFFFBF4`, which
7.6.4.2 defines as a signed value: `-1036`. Producers write it either way, because
Table 22 sets every reserved bit to 1 and the sign follows.

```rust
fn permissions(arena: &PdfArena, encrypt: &Dict) -> Option<i32> {
    let value = integer(arena, encrypt, "P")?;
    i32::try_from(value).ok()          // 4294966260 does not fit; None
}
```

and, at the one call site that matters:

```rust
permissions(arena, encrypt).unwrap_or(0)
```

Algorithm 2 hashes `/P` into the file encryption key. A `/P` of `0` where the file
says `-1036` is a different key:

| `/P` passed to Algorithm 2 | File encryption key |
| :--- | :--- |
| `-1036`, as the file means it | `d889527373ba8d339c29e3d0d0f7a3c9` |
| `0`, as `unwrap_or` supplied | `abb690033ba2ad5d99632a67d60ba5c6` |

The cryptography was never wrong. Deriving the key by hand with CommonCrypto against
Algorithm 2 produced `d889…`, and so did `SecurityHandler::new_v4` when `-1036` was
passed to it. Decrypting 200 streams with that handler inflated 200 of them. Only the
value reaching it was wrong, and only because a conversion failure had a default.

## Decision

Reinterpret the low 32 bits rather than rejecting them, and stop defaulting:

```rust
i32::try_from(value).ok().or_else(|| u32::try_from(value).ok().map(|bits| bits as i32))
```

`build_handler` now propagates the `None` instead of substituting `0`, so a `/P` that
is neither form refuses to build a handler rather than building a wrong one.

**Validate the password.** Algorithm 6 — recompute `/U` from the derived key by
Algorithm 4 or 5 and compare — was absent, which is why the wrong key went unnoticed.
RC4 is written out in `security.rs` for it; no crate here provided one, and it is
twenty lines.

## Consequences

- The encrypted sample now reads. `publish upgrade` on it produces a file PDFKit reads
  as 460,648 characters — **byte-for-byte the same count as the source**. Decisions
  recorded while reading it went from 30,343 to **zero**.
- A wrong password is now refused: `build_handler` returns `None`, `unlock` records a
  `Violation` under 7.6.1, and the document is left encrypted with its structure
  readable, which is the behaviour `unlock` already documented.
- **No internal measurement could have found this.** Object counts matched, page counts
  matched, the round trip was self-consistent, and `examples/compare_documents.rs`
  compares two fepdf reads — both would have been the same noise. It took an
  independent reader, which is the second time (ADR-0006 was the first) and the
  stronger case: there the disagreement was one page, here it was the entire content.
- A tolerance that reports nothing is worse than a failure. `unwrap_or(0)` on an
  unparseable permissions field turned an unreadable document into a silently wrong
  one. Where the reader cannot determine a value that feeds a key, it must decline.
- `scripts/dev/status.sh` now asserts that text comes out of the encrypted sample.
  Checking that it *opens* would have passed throughout.
