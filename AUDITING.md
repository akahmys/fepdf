# 🛡️ Ferruginous Security, License & Compliance Auditing Protocol

This document defines the automated audit checks, license policy, security vulnerability management, and secret protection standards.

---

## 🔍 1. Audit Framework Overview

Ferruginous enforces a 4-tier automated compliance pipeline:

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
1. Function line limits (Rule 1).
2. Absence of `unwrap`/`expect` (Rule 2).
3. Zero `unsafe` blocks (Rule 3).
4. Match exhaustiveness & no wildcard `=> _` (Rule 5).
5. Zero `static mut` (Rule 7).
6. No `HashMap`/`HashSet` in core crates (Rule 10).
7. Zero Clippy warnings (`cargo clippy --workspace -- -D warnings`) (Rule 17).
8. Pass `cargo deny check licenses` (Rule 16).
9. Pass `betterleaks dir .` (Rule 18).
