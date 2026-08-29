# 🛡️ fepdf Security, License & Compliance Auditing Protocol

This document defines the automated audit checks, license policy, security vulnerability management, and secret protection standards.

---

## 🔍 1. Audit Framework Overview

fepdf enforces a 4-tier automated compliance pipeline:

```
                          ┌──────────────────────────┐
                          │ verify_compliance.sh     │
                          └────────────┬─────────────┘
                                       │
     ┌──────────────────┬──────────────┴──────────────┬──────────────────┐
     ▼                  ▼                             ▼                  ▼
1. RR-15 Rules     2. Clippy Lints            3. License Audit    4. Secret Scan
(Line limits,      (-D warnings,              (cargo-deny via     (betterleaks via
 panic, unsafe)     pedantic/nursery)          deny.toml)          pre-commit)
```

---

## 📜 2. License Compliance Protocol (`cargo-deny`)

All workspace crates and third-party dependencies are continuously audited using **`cargo-deny`** against the project's license policy configured in [`deny.toml`](deny.toml).

### Allowed License List (Permissive & Weak Copyleft)
- **Primary**: `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`
- **BSD Family**: `BSD-3-Clause`, `BSD-2-Clause`, `BSD-1-Clause`, `0BSD`
- **Public Domain / Permissive**: `CC0-1.0`, `Unlicense`, `ISC`, `BSL-1.0`, `Zlib`, `MIT-0`
- **Fonts & Special**: `OFL-1.1`, `Ubuntu-font-1.0`, `Unicode-3.0`, `Unicode-DFS-2016`, `MPL-2.0`, `NCSA`

**Five** of these — `BSD-1-Clause`, `MIT-0`, `NCSA`, `Unicode-DFS-2016` and, since
2026-08-22, `MPL-2.0` — match no dependency in the current tree, and `cargo deny` says so
on every run as an *unmatched license allowance*. The list is a standing policy rather
than a description of the lockfile, so that is expected.

**`MPL-2.0` is the case this paragraph warned about, and it happened.** It said to read
the warnings rather than tune them out, "because the same wording appears when a
dependency that *was* relying on an allowance is dropped" — and one was: `encoding_rs`,
the only MPL-2.0 crate in the tree, which came in through `reqwest` and left with it under
Rule 9 ([ADR-0024](docs/adr/0024-pure-rust-is-a-rule-and-therefore-has-a-check.md)). It had
also been named in `docs/specs/refinery_engine.md` as doing text recovery, which it was
not. A licence allowance going quiet is a dependency-graph change reported by the one tool
that always notices.

Re-derive the list rather than trusting this paragraph:

```bash
cargo deny check licenses 2>&1 | grep -B1 'unmatched license allowance'
```

### Forbidden Licenses
- Strong copyleft licenses (e.g., `GPL-2.0`, `GPL-3.0`, `AGPL-3.0`) are strictly **denied** (`copyleft = "deny"`).

### License Audit Commands
```bash
# Run Cargo-native license check
cargo deny check licenses

# Via Makefile
make audit-licenses
```

---

## 🔐 3. Secret & PII Protection Protocol (`betterleaks`)

To prevent accidental leaks of credentials, private keys, API tokens, and Personally Identifiable Information (PII):

### Git Pre-commit Hook
Security scanning is automatically enforced before every git commit via `.git/hooks/pre-commit` using **`betterleaks`**.

### Custom Leak Prevention Rules ([.betterleaks.toml](.betterleaks.toml))
- **AWS / API Keys**: Standard high-entropy and cloud credential patterns.
- **Private Keys**: RSA, Elliptic Curve, SSH private keys.
- **Personal Name Protection**: Pattern `\b(jun[\s._-]*kato|kato[\s._-]*jun)\b` preventing PII leakage.

### Secret Audit Commands
```bash
# Scan working directory for secrets
betterleaks dir .

# Run pre-commit staged scan manually
betterleaks git --pre-commit --staged
```

---

## 🛠️ 4. Static Compliance Script (`verify_compliance.sh`)

Execute the master audit script:
```bash
./scripts/audit/verify_compliance.sh
```

### Script Execution Criteria

**Fifteen checks**, in the order the script runs them. Derive the list rather than
maintaining it — that is the only way to keep the two in step, and it has now failed
twice in opposite directions:

```bash
grep -oE '\[Rule [0-9]+\]' scripts/audit/verify_compliance.sh
```

| | Check | Rule |
| ---: | :--- | :--- |
| 1 | Function line limits | 1 |
| 2 | No `unwrap`/`expect` in production code | 2 |
| 3 | No `unsafe` blocks | 3 |
| 4 | No wildcard match arms over domain enums | 5 |
| 5 | No `static mut` | 7 |
| 6 | No non-deterministic collections in core crates | 10 |
| 7 | No `String`/`anyhow` errors in a `Result` | 11 |
| 8 | No `filter_map(Result::ok)` | 13 |
| 9 | Test code separation — no standalone test files in `src/` | 14 |
| 10 | No excessive cloning | 15 |
| 11 | `cargo clippy --workspace --all-targets -- -D warnings` | 17 |
| 12 | **No dependency that compiles C** | **9** |
| 13 | `cargo fmt --all --check` | 19 |
| 14 | `cargo deny check licenses` | 16 |
| 15 | `betterleaks dir .` | 18 |

**Both failure directions have now happened.** This table once named nine checks and
omitted 11, 13, 14, 15 and 19, so five enforced rules read as unenforced. Then Rule 9 was
added to the script and not to the table, so a fourteenth check ran unlisted — and, worse,
checks 14 and 15 were mapped to **rules `CODING.md` did not state at all**: it had no
Rule 16 and no Rule 18, while the script had been enforcing both under those numbers for
longer than either document. This table cited them as though `CODING.md` defined them.

A rule that is checked but not stated is harder to catch than one stated but not checked,
because nothing goes red. Both are now in `CODING.md`.

`CODING.md` states each rule; this table states only which of them the script enforces.
Rules 4, 6, 8 and 20 are in `CODING.md` and are **not** here, because nothing automated
checks them — `CODING.md` names code or architecture review for each, per the rule that
a rule which is not checked is a comment.
