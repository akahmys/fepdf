# fepdf — auditing

> **Phase: audit.** What is checked mechanically, and by what. The rules being checked are
> in [CODING.md](CODING.md).

One script runs everything: [`scripts/audit/verify_compliance.sh`](scripts/audit/verify_compliance.sh),
or `make audit`. It must end `=== AUDIT PASSED ===`; read that line, not the first.

## 1. Licences (`cargo-deny`)

Every dependency's licence must be on the allow-list in [`deny.toml`](deny.toml), which is
the one place the list is written; strong copyleft is denied (`copyleft = "deny"`).

Some of these match no dependency in the current tree, and `cargo deny` reports each as
an *unmatched license allowance* on every run. The list is a standing policy, not a
description of the lockfile, so that is expected — **and the warnings are still worth
reading**, because the same wording appears when a dependency that *was* relying on an
allowance is dropped. `MPL-2.0` became unmatched exactly that way when `encoding_rs` left.

**Commands**
```bash
# Run Cargo-native license check
cargo deny check licenses

# Via Makefile
make audit-licenses
```

---

## 2. Secrets and PII (`betterleaks`)

Credentials, private keys, tokens and personal data are scanned for by **`betterleaks`**,
before every commit through `.git/hooks/pre-commit` and in the audit. The patterns,
including the project's own, are in [`.betterleaks.toml`](.betterleaks.toml).

**Commands**
```bash
# Scan working directory for secrets
betterleaks dir .

# Run pre-commit staged scan manually
betterleaks git --pre-commit --staged
```

---

## 3. What the script checks

**Thirty-four steps**, in the order the script runs them. Derive this list rather than
maintaining it — and derive it from the lines that *are* steps:

```bash
grep -cE '^echo "\[[^]]+\]' scripts/audit/verify_compliance.sh
```

It counts `echo "[…]"` lines and nothing narrower: a pattern that names the bracket's
contents misses `[Rules A, D]` and `[Rule UI-*]`, and one that reads comments counts a
retired rule.

| | Step | Rule |
| ---: | :--- | :--- |
| 1 | Function line limits | 1 |
| 2 | Impl block line limits | 1 |
| 3 | **No panic in production code — `unwrap`, `expect`, the panicking macros, an index or a slice — by clippy** | **2** |
| 4 | No wildcard match arms over domain enums | 5 |
| 5 | **Wildcard arms over a file's numeric value — counted; a stale exemption fails** | **20** |
| 6 | No non-deterministic collections in core crates | 10 |
| 7 | No `String`/`anyhow` errors in a `Result` | 11 |
| 8 | No `filter_map(Result::ok)` | 13 |
| 9 | **No `Result` discarded by `let _ =` without a reason** | **13** |
| 10 | Test code separation — no standalone test file in `src/`; **a test is `#[ignore]`d only as needing a file the repository does not hold** | 14 |
| 11 | Excessive cloning (warns; does not fail) | 15 |
| 12 | `rust-version` is one version in every `Cargo.toml` and `README.md`; `rust-toolchain.toml` pins at or above it, and `rustc` runs what it pins | — |
| 13 | The stated minimum builds the workspace (`msrv_build.sh`) | — |
| 14 | `cargo check --workspace` | — |
| 15 | `cargo clippy --workspace --all-targets -- -D warnings` | 4, 5 |
| 16 | **No dependency that compiles C** | **9** |
| 17 | **No unbounded recursion over a document's graph** | **6** |
| 18 | **Document tense, links — relative ones, and intra-doc ones by `cargo doc` with warnings denied — and the ADR index** | **`AGENTS.md` 1, 2** |
| 19 | **Every row `status.sh` labels "(expect 0)" reads 0** | **`AGENTS.md` 4** |
| 20 | **What stands above the facade, what it declares, and what the facade lets in** | **A, D** |
| 21 | **Every icon codepoint resolves to a glyph that draws, and is written in one file** | **UI-1** |
| 22 | **Colours are written in the palette; three exemptions, named** | **UI-9** |
| 23 | **Spacing, type size and corner radius come from the declared scales** | **UI-11** |
| 24 | **User-facing strings are locale keys: at a sink, and anywhere a literal reads as prose** | **UI-5** |
| 25 | **Icon controls carry a name a screen reader can read** | **UI-2** |
| 26 | **Every change to the document takes the one recorded path** | **UI-6** |
| 27 | **The palette's contrast, against every surface a colour can meet** | **UI-8** |
| 28 | **One visible door per feature, and no second one** | **UI-4, UI-12** |
| 29 | **The accent names a selection and nothing else** | **UI-10** |
| 30 | **Work the reader waits for says that it is happening** | **UI-7** |
| 31 | **Each act names the view that answers it, in one table** | **UI-14** |
| 32 | `cargo fmt --all --check` | 19 |
| 33 | `cargo deny check licenses` | 16 |
| 34 | `betterleaks dir .` | 18 |

**Rules 3 and 7 are not here and are not unenforced.** `unsafe_code = "forbid"` fails the
build on an `unsafe` block, and a `static mut` cannot be read without one, so `rustc`
holds both — verified by adding each to `fepdf-model` and watching `cargo build` fail. The
greps that used to sit here matched `unsafe {`, missed `unsafe(`, and ran after a build
that had already succeeded.

`CODING.md` states each rule; this table states only which the script enforces. Rules 4
and 8 are in `CODING.md` and **not** here, because nothing automated checks them — each
names review instead, per the rule that an unchecked rule is a comment. **Rule 20 is here
for part of itself**, below.

**Rules 6 and 20 are checked by a script each**, and what each sees and does not is in
`CODING.md`'s "in detail" sections. Rule 20's script counts silent arms without gating on
the count; what fails the audit is an exemption naming a site that no longer exists.

**A rule that is checked but not stated is harder to catch than one stated but not
checked, because nothing goes red.** Both directions have happened here: five enforced
rules once read as unenforced, and the script enforced Rules 16 and 18 under numbers
`CODING.md` did not define. Deriving the list is what keeps the two in step.
