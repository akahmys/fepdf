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

Four of these — `BSD-1-Clause`, `MIT-0`, `NCSA` and `Unicode-DFS-2016` — match no
dependency in the current tree, and `cargo deny` says so on every run as an *unmatched
license allowance*. The list is a standing policy rather than a description of the
lockfile, so that is expected; it is worth reading the warnings rather than tuning them
out, because the same wording appears when a dependency that *was* relying on an
allowance is dropped.

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

Fourteen checks, in the order the script runs them. This list is derived from the
script's own `[Rule N]` headings, which is the only way to keep the two in step — it
named nine and omitted 11, 13, 14, 15 and 19, so five enforced rules read as unenforced.

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
| 11 | `cargo deny check licenses` | 16 |
| 12 | `cargo clippy --workspace --all-targets -- -D warnings` | 17 |
| 13 | `betterleaks dir .` | 18 |
| 14 | `cargo fmt --all --check` | 19 |

`CODING.md` states each rule; this table states only which of them the script enforces.
Rules 4, 6, 8 and 20 are in `CODING.md` and are **not** here, because nothing automated
checks them — `CODING.md` names code or architecture review for each, per the rule that
a rule which is not checked is a comment.
