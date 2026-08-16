# 🤖 fepdf Agentic Governance & System Architecture

Welcome to **fepdf**, an experimental, high-fidelity PDF 2.0 processing platform built with Rust. This project operates under an AI-native autonomous engineering model adhering to strict safety and determinism guarantees.

---

## 🏛️ Governance Architecture & Document Structure

The project rules, architecture specs, and operational protocols are modularised into
these documents. [Which document owns what](#-which-document-owns-what) below is the
part to read when deciding where to write something.

| Document | Focus & Scope | Description |
| :--- | :--- | :--- |
| 📘 **[AGENTS.md](AGENTS.md)** | **Constitution & Governance** | System vision, truth hierarchy, decision framework, and entry point. |
| 🏛️ **[ARCHITECTURE.md](ARCHITECTURE.md)** | **System Design & Layering Rules** | The rules that decide where code goes, target crate topology with per-crate migration status, `PdfArena` invariants, the Sublimation Pipeline and the two layers it produces, Vello renderer. |
| 📋 **[PLANNING.md](PLANNING.md)** | **Planning & Discovery** | Implementation plans, architecture design, exploration protocols, and task breakdown. |
| 💻 **[CODING.md](CODING.md)** | **Coding Rules & Architecture** | **RR-15 Protocol**, ISO 32000-2 pipeline, Vello rendering, and Rust 2024 coding standards. |
| 🛡️ **[AUDITING.md](AUDITING.md)** | **Security, Compliance & Audit** | Static auditing, **`cargo-deny`** license checks, **`betterleaks`** PII protection, and Clippy lints. |
| 📜 **[docs/adr/](docs/adr/README.md)** | **Decision Records** | Decisions that were contested, reversed, or rest on a measurement — and what the measurement was. |
| 🧪 **[TESTING.md](TESTING.md)** | **Testing & Validation** | Workspace unit/integration tests, Vello visual regression, and MSRV compatibility. |

---

## ⚖️ Hierarchy of Truth

When directives conflict, resolve in this order:

```
1. ISO 32000-2:2020, the standard itself
   └── 2. Measurement of the code as it is
        └── 3. RR-15 (CODING.md) and the layering rules (ARCHITECTURE.md)
             └── 4. The remaining governance documents
                  └── 5. docs/specs/ and other background material
```

**Measurement outranks documentation.** This is not a platitude: four decisions in
`docs/adr/` were reversed because a document asserted something the code did not do.
A document that disagrees with a verified measurement is wrong and gets corrected, not
argued from.

---

## 📚 Which Document Owns What

Each document answers one question. Writing something in the wrong one is how two
documents come to disagree.

| Document | Answers | Does **not** contain |
| :--- | :--- | :--- |
| **[README.md](README.md)** | What is this, how do I build it? | Design or rules |
| **[AGENTS.md](AGENTS.md)** | How is the project governed? Where does everything live? | The rules themselves |
| **[ARCHITECTURE.md](ARCHITECTURE.md)** | What is the design **now**, and why this shape? | History, plans, coding rules |
| **[docs/adr/](docs/adr/README.md)** | How did it come to be? What was tried and dropped? | The present design |
| **[CODING.md](CODING.md)** | What must code satisfy? | Design rationale |
| **[AUDITING.md](AUDITING.md)** | How is compliance checked? | The rules being checked |
| **[TESTING.md](TESTING.md)** | What must be verified before merging? | Test results |
| **[ROADMAP.md](ROADMAP.md)** | What is next, and what does done mean? | Completed history |
| **[PLANNING.md](PLANNING.md)** | How is a change planned before it is written? | Any specific plan |
| **docs/specs/** | Background on a subsystem. | Anything authoritative |
| **docs/retrospectives/**, **docs/history/** | What happened, as it was then. | Anything current |
| **.agents/** | Agent operating protocols: rules, skills, workflows. | The enforced form of any rule — that is `CODING.md` |

### Rules

1. **One fact, one home.** If it belongs in two places, one of them links instead of
   restating. `ARCHITECTURE.md` §4 links to ADRs rather than repeating them.
2. **Present tense is `ARCHITECTURE.md`; past tense is `docs/adr/`.** A decision that
   was reversed is recorded, not deleted, so the reasoning is not repeated.
3. **Historical documents are never updated.** `docs/retrospectives/` and
   `docs/history/` keep the names and facts that were true when written. They were
   deliberately excluded from the `ferruginous`→`fepdf` rename.
4. **A quoted figure carries its date.** Measurements go stale. Either re-verify
   before quoting, or write "at the time" — an ADR that silently rots is worse than
   none.
5. **A rule that is not checked is a comment.** Every entry in `CODING.md` names what
   enforces it. If nothing does, say so rather than implying enforcement.
6. **`docs/specs/` is background, not truth.** It predates most of the current design
   and sits at the bottom of the hierarchy. Where it disagrees with
   `ARCHITECTURE.md`, `ARCHITECTURE.md` wins.

---

## 🎯 Core Operating Principles

1. **Safety Over Speed**: Memory safety, determinism, and ISO compliance take precedence over premature optimization or prototyping.
2. **Zero Unsafe**: `unsafe_code = "forbid"` is enforced across all workspace crates.
3. **Automated Verification**: Every code change must be verifiable via `./scripts/audit/verify_compliance.sh`, `cargo deny`, `betterleaks`, and `cargo test`.
4. **Measure, do not assume**: A claim about this codebase is established by running
   something, not by reading a document or a function name — see the hierarchy above.
   Note that this is *not* "log-first": the engine holds exactly one `log::warn!` by
   design, because a warning on stderr cannot tell a caller *this loaded* from *this was
   conforming*. What the engine finds in a document it records as a `Decision`
   (`ARCHITECTURE.md` §5.3); what you want to know about the engine you measure.

---

## 🚀 Quick Verification Commands

```bash
# Where the project stands, with the documents' own figures re-measured
./scripts/dev/status.sh --full

# Run full compliance audit (RR-15 rules, Clippy, cargo-deny, betterleaks)
./scripts/audit/verify_compliance.sh

# Run Cargo-native license audit
cargo deny check licenses

# Run full workspace unit tests
cargo test --workspace

# Run GPU visual regression tests
python3 scripts/visual_regression.py
```
