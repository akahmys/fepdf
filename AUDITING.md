# fepdf — auditing

> **Phase: audit.** What is checked mechanically, and by what. The rules being checked are
> in [CODING.md](CODING.md).

One script runs everything: [`scripts/audit/verify_compliance.sh`](scripts/audit/verify_compliance.sh),
or `make audit`. It must end `=== AUDIT PASSED ===`; read that line, not the first.

## 1. Licences (`cargo-deny`)

All workspace crates and third-party dependencies are continuously audited using **`cargo-deny`** against the project's license policy configured in [`deny.toml`](deny.toml).

**Allowed**
- **Primary**: `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`
- **BSD Family**: `BSD-3-Clause`, `BSD-2-Clause`, `BSD-1-Clause`, `0BSD`
- **Public Domain / Permissive**: `CC0-1.0`, `Unlicense`, `ISC`, `BSL-1.0`, `Zlib`, `MIT-0`
- **Fonts & Special**: `OFL-1.1`, `Ubuntu-font-1.0`, `Unicode-3.0`, `Unicode-DFS-2016`, `MPL-2.0`, `NCSA`

Some of these match no dependency in the current tree, and `cargo deny` reports each as
an *unmatched license allowance* on every run. The list is a standing policy, not a
description of the lockfile, so that is expected — **and the warnings are still worth
reading**, because the same wording appears when a dependency that *was* relying on an
allowance is dropped. `MPL-2.0` became unmatched exactly that way when `encoding_rs` left.

**Forbidden**
- Strong copyleft licenses (e.g., `GPL-2.0`, `GPL-3.0`, `AGPL-3.0`) are strictly **denied** (`copyleft = "deny"`).

**Commands**
```bash
# Run Cargo-native license check
cargo deny check licenses

# Via Makefile
make audit-licenses
```

---

## 2. Secrets and PII (`betterleaks`)

To prevent accidental leaks of credentials, private keys, API tokens, and Personally Identifiable Information (PII):

**Pre-commit hook**
Security scanning is automatically enforced before every git commit via `.git/hooks/pre-commit` using **`betterleaks`**.

**Custom rules** ([.betterleaks.toml](.betterleaks.toml))
- **AWS / API Keys**: Standard high-entropy and cloud credential patterns.
- **Private Keys**: RSA, Elliptic Curve, SSH private keys.
- **Personal Name Protection**: Pattern `\b(jun[\s._-]*kato|kato[\s._-]*jun)\b` preventing PII leakage.

**Commands**
```bash
# Scan working directory for secrets
betterleaks dir .

# Run pre-commit staged scan manually
betterleaks git --pre-commit --staged
```

---

## 3. What the script checks

Execute the master audit script:
```bash
./scripts/audit/verify_compliance.sh
```

**Thirty steps**, in the order the script runs them. Derive this list rather than
maintaining it — and derive it from the lines that *are* steps:

```bash
grep -cE '^echo "\[[^]]+\]' scripts/audit/verify_compliance.sh
```

The obvious form, `grep -oE '\[Rule [0-9]+\]'`, was here until 2026-09-06 and reported a
`[Rule 17]` that is not a step: it is a comment recording what the clippy step was called
before Rule 17 was retired. A derivation that reads comments is not a derivation.

**Nor is one that cannot see three of its own subjects.** The form here read
`[A-Za-z0-9 ]+` inside the brackets, so it missed `[Rules A, D]` on the comma and both
`[Rule UI-*]` steps on the hyphen — and the prose above it said sixteen while the table
below listed twenty. Three numbers about one script, no two of them equal. The class the
brackets hold is not a class this file gets to choose, so it does not try to name it.

| | Step | Rule |
| ---: | :--- | :--- |
| 1 | Function line limits | 1 |
| 2 | No `unwrap`/`expect` in production code | 2 |
| 3 | No wildcard match arms over domain enums | 5 |
| 4 | **Wildcard arms over a file's numeric value — counted; a stale exemption fails** | **20** |
| 5 | No non-deterministic collections in core crates | 10 |
| 6 | No `String`/`anyhow` errors in a `Result` | 11 |
| 7 | No `filter_map(Result::ok)` | 13 |
| 8 | **No `Result` discarded by `let _ =` without a reason** | **13** |
| 9 | Test code separation — no standalone test file in `src/` | 14 |
| 10 | Excessive cloning (warns; does not fail) | 15 |
| 11 | MSRV stated as one version across `Cargo.toml`, `.rust-toolchain.toml` and `README.md` | — |
| 12 | `cargo check --workspace` | — |
| 13 | `cargo clippy --workspace --all-targets -- -D warnings` | 4, 5 |
| 14 | **No dependency that compiles C** | **9** |
| 15 | **No unbounded recursion over a document's graph** | **6** |
| 16 | **Document tense, links, and the ADR index** | **`AGENTS.md` 1, 2** |
| 17 | **What stands above the facade, what it declares, and what the facade lets in** | **A, D** |
| 18 | **Every icon codepoint resolves to a glyph that draws, and is written in one file** | **UI-1** |
| 19 | **Colours are written in the palette; three exemptions, named** | **UI-9** |
| 20 | **Spacing, type size and corner radius come from the declared scales** | **UI-11** |
| 21 | **User-facing strings are locale keys; four structure names exempt** | **UI-5** |
| 22 | **Icon controls carry a name a screen reader can read** | **UI-2** |
| 23 | **Every change to the document takes the one recorded path** | **UI-6** |
| 24 | **The palette's contrast, against every surface a colour can meet** | **UI-8** |
| 25 | **One visible door per feature, and no second one** | **UI-4, UI-12** |
| 26 | **The accent names a selection and nothing else** | **UI-10** |
| 27 | **Work the reader waits for says that it is happening** | **UI-7** |
| 28 | `cargo fmt --all --check` | 19 |
| 29 | `cargo deny check licenses` | 16 |
| 30 | `betterleaks dir .` | 18 |

**Rules 3 and 7 are not here and are not unenforced.** `unsafe_code = "forbid"` fails the
build on an `unsafe` block, and a `static mut` cannot be read without one, so `rustc`
holds both — verified by adding each to `fepdf-model` and watching `cargo build` fail. The
greps that used to sit here matched `unsafe {`, missed `unsafe(`, and ran after a build
that had already succeeded.

`CODING.md` states each rule; this table states only which the script enforces. Rules 4
and 8 are in `CODING.md` and **not** here, because nothing automated checks them — each
names review instead, per the rule that an unchecked rule is a comment. **Rule 20 is here
for part of itself**, which the next-but-one paragraph is about.

**Rule 6 joined the table on 2026-09-06**, and what it took to get there is the argument
for the rule that put it in `CODING.md` with "Code review" in the first place. Review
missed five unbounded walks in one week, three of which aborted the process on files of
four or five objects, and `scripts/audit/unbounded_recursion.py` found a sixth on its
first run. `CODING.md`'s "Rule 6 in detail" says what it sees and what it does not.

**Rule 20 has a counter, which is not the same as a check.**
`scripts/audit/silent_branches.py` lists the wildcard arms where a value read out of a
file gets neither a `Decision` nor an error — the ground Rule 5's lint cannot reach,
because `/LC`, `/LJ`, `/ShadingType` and `/V` arrive as integers rather than as enums.
**The count still does not gate a commit** and is not meant to: some of what it lists is
defensible — an unknown `/V` makes the document fail to open, which is loud enough — and
the number is there so a new one is visible.

**What joined the script on 2026-09-08 is the verdict the tool already had.** It exits
non-zero when an exemption names a site that no longer matches a silent arm, which reads
as a check still being made and is not one. Only `status.sh` ran the tool until then, and
`status.sh` reports rather than gates, so that exit code had never been read by anything.
Proved by adding an exemption for a function that does not exist and watching step 4 fail.

It reads **0**: the count
went 11 → 8 on 2026-08-30, when three enumerants gained recording callers, and 8 → 0 on
2026-08-31, when the remaining eight were audited and each registered against the caller
that records for it. All 11 are printed with that caller named, so the list stays readable
as *why* each is not silent rather than as an absence.

**This document and `CODING.md` both said 8 until 2026-09-06** — a figure stated twice,
which is the case `AGENTS.md`'s third writing rule names, and it drifted in both places at
once because neither is derived. `status.sh` prints the live number beside them.

It exits non-zero on one thing only: an exemption naming a site that no longer exists,
which reads as a check still being made.

**A rule that is checked but not stated is harder to catch than one stated but not
checked, because nothing goes red.** Both directions have happened here: five enforced
rules once read as unenforced, and the script enforced Rules 16 and 18 under numbers
`CODING.md` did not define. Deriving the list is what keeps the two in step.
