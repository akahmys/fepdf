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

**Fifteen checks**, in the order the script runs them. Derive this list rather than
maintaining it:

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

`CODING.md` states each rule; this table states only which the script enforces. Rules 4,
6, 8 and 20 are in `CODING.md` and **not** here, because nothing automated checks them —
each names review instead, per the rule that an unchecked rule is a comment.

**A rule that is checked but not stated is harder to catch than one stated but not
checked, because nothing goes red.** Both directions have happened here: five enforced
rules once read as unenforced, and the script enforced Rules 16 and 18 under numbers
`CODING.md` did not define. Deriving the list is what keeps the two in step.
