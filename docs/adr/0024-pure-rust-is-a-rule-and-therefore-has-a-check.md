# ADR-0024: Pure Rust is a rule, and therefore has a check

- **Status**: Accepted
- **Date**: 2026-08-22
- **Commit**: a2c225e

## Context

Choosing an ECMAScript engine put the question in front of everything else. QuickJS is
the obvious embedding choice — complete ES2023, small, no JIT — and boa is "some of the
language" with 528 `unsafe` occurrences. The argument against QuickJS looked like a
preference until it was measured, and then it looked like nothing at all:

```
fepdf-model → zstd → zstd-safe → zstd-sys → cc
```

The engine already compiled C. "Do not introduce a C dependency" was not a principle this
project held; it was a sentence nobody had checked.

Measuring what that C actually was made the question smaller than expected. Three
dependencies produced the whole of it, and **two of them were referenced by no line of
code**:

| | In the source | What it dragged in |
| :--- | :--- | :--- |
| `reqwest` | **nothing** | `ring` (C crypto), `rustls`, `hyper`, `encoding_rs` |
| `rustls-native-certs` | **nothing** | `security-framework` and three macOS `-sys` crates |
| `zstd` | two call sites | `zstd-sys`, the only vendored C in the build |

An HTTP client and an OS trust store, in a crate that reads PDF files. Both were almost
certainly added for long-term signature validation — which is `/DSS` and `/Perms`,
ROADMAP's P3, carried by **none** of the 524 corpus files. A container before its
contents, in the dependency manifest rather than in a struct, where nothing was looking.

`zstd` was the only one doing work, and both jobs survive without it. The
`/ZstandardDecode` filter is **not in ISO 32000-2** — zero occurrences across 1020 pages —
the writer never emits it, and **no file of the 530 in any corpus carries the Zstd magic
number anywhere**, including the "lie filter" heuristic that tested every stream's first
four bytes for it. Its doc comment said "a producer in the wild writes it"; nothing
measured said so. The other job is compressing large streams in memory, and that form
never reaches a file — `flate2`, already present and pure Rust, does it.

## Decision

**Pure Rust is a rule.** It is RR-15 Rule 9, and like every other rule in `CODING.md` it
names what enforces it, because a rule that is not checked is a comment.

**The line is compiled foreign source, not FFI.** "No FFI" would forbid the language:
`std` links libc, and every program reaches its operating system through a C ABI. What
*is* both meaningful and checkable is whether the build compiles vendored C — because
that is code no Rust tool audits. `cargo clippy`, the `unsafe` ban, the function-length
limit and the determinism rules all stop at the language boundary.

| | Example | |
| :--- | :--- | :--- |
| **Forbidden** | `zstd-sys`, `libz-sys`, `openssl-sys`, `ring`, a QuickJS binding | Compiles vendored C |
| **Allowed** | `core-foundation-sys`, `windows-sys`, `libc` | The platform's own API, which `std` already needs |

The check is one line of logic: **no crate named `cc` in any workspace member's dependency
tree.** `cc` is how a Rust build compiles C, and nothing compiles C without it. It was
verified by putting `zstd` back and watching the audit fail, naming all nine affected
crates and printing the chain.

**This settles the engine choice: boa.** Not because boa is better at being a JavaScript
engine — QuickJS is more complete and says so — but because the rule now exists and
QuickJS cannot satisfy it. What remains is a question the rule does not answer, and it
should be asked separately: whether 124 crates and 528 `unsafe` occurrences are a trade
worth making for a capability the corpus has not yet asked for.

## Consequences

- **`fepdf-model` drops from 234 transitive crates to 149**, and the whole workspace
  compiles zero C — including `fepdf-gui`, because wgpu is pure Rust now too. The rule was
  reachable across every member, not only the engine.
- **`/ZstandardDecode` is gone**, and with it a heuristic that examined the first four
  bytes of every stream in every document for a format no standard defines and no file
  carries. `ROADMAP.md`'s filter row loses a sentence it should not have had.
- **Two dependencies vanished that nothing was using**, which nothing in this project's
  checking would have caught. `status.sh` reports "ingestion options nothing reads:
  none" because ADR-0007 made that visible; there is no equivalent row for dependencies,
  and there should be.
- **The rule was written after the fact.** Three dependencies had to go before it could
  pass, and admitting that is the point: a principle that is announced and not checked is
  how `docs/specs/` came to hold twelve false claims.
