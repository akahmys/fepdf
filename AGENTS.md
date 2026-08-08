# 🤖 Ferruginous Agentic Governance & System Architecture

Welcome to **Ferruginous**, an experimental, high-fidelity PDF 2.0 processing platform built with Rust. This project operates under an AI-native autonomous engineering model adhering to strict safety and determinism guarantees.

---

## 🏛️ Governance Architecture & Document Structure

The project rules, architecture specs, and operational protocols are modularized into five core documents:

| Document | Focus & Scope | Description |
| :--- | :--- | :--- |
| 📘 **[AGENTS.md](AGENTS.md)** | **Constitution & Governance** | System vision, truth hierarchy, decision framework, and entry point. |
| 🏛️ **[ARCHITECTURE.md](ARCHITECTURE.md)** | **System Design & Pipeline** | Crate topology, `PdfArena`, Sublimation Pipeline Pass 0/1/2, Vello renderer. |
| 📋 **[PLANNING.md](PLANNING.md)** | **Planning & Discovery** | Implementation plans, architecture design, exploration protocols, and task breakdown. |
| 💻 **[CODING.md](CODING.md)** | **Coding Rules & Architecture** | **RR-15 Protocol**, ISO 32000-2 pipeline, Vello rendering, and Rust 2024 coding standards. |
| 🛡️ **[AUDITING.md](AUDITING.md)** | **Security, Compliance & Audit** | Static auditing, **`cargo-deny`** license checks, **`betterleaks`** PII protection, and Clippy lints. |
| 🧪 **[TESTING.md](TESTING.md)** | **Testing & Validation** | Workspace unit/integration tests, Vello visual regression, and MSRV compatibility. |

---

## ⚖️ Hierarchy of Truth

When conflicting directives arise, agents and contributors MUST resolve ambiguities using the following strict hierarchy:

```
1. ISO 32000-2:2020 Standard Specification
   └── 2. RR-15 (Reliable Rust-15) Safety & Hardening Protocol
        └── 3. Core Architecture Specs (docs/specs/)
             └── 4. Primary Governance Docs (AGENTS, PLANNING, CODING, AUDITING, TESTING)
                  └── 5. Codebase Implementation & Workspace Crates
```

---

## 🎯 Core Operating Principles

1. **Safety Over Speed**: Memory safety, determinism, and ISO compliance take precedence over premature optimization or prototyping.
2. **Zero Unsafe**: `unsafe_code = "forbid"` is enforced across all workspace crates.
3. **Automated Verification**: Every code change must be verifiable via `./scripts/audit/verify_compliance.sh`, `cargo deny`, `betterleaks`, and `cargo test`.
4. **Log-First Diagnostics**: Diagnostics must be driven by empirical log evidence rather than assumptions.

---

## 🚀 Quick Verification Commands

```bash
# Run full compliance audit (RR-15 rules, Clippy, cargo-deny, betterleaks)
./scripts/audit/verify_compliance.sh

# Run Cargo-native license audit
cargo deny check licenses

# Run full workspace unit tests
cargo test --workspace

# Run GPU visual regression tests
python3 scripts/visual_regression.py
```
