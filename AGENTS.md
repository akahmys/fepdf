# 🤖 fepdf Agentic Governance & System Architecture

Welcome to **fepdf**, an experimental, high-fidelity PDF 2.0 processing platform built with Rust. This project operates under an AI-native autonomous engineering model adhering to strict safety and determinism guarantees.

---

## ⚖️ Hierarchy of Truth

When directives conflict, resolve in this order:

```
1. ISO 32000-2:2020, the standard itself
   └── 2. Measurement of the code as it is
        └── 3. AGENTS.md — the principles below
             └── 4. The rules of each phase, in the order work happens:
                    PLANNING.md → CODING.md → TESTING.md → AUDITING.md
                 └── 5. docs/specs/ and other background material
```

`ARCHITECTURE.md` is the design, `ROADMAP.md` is the work, `README.md` is the
introduction, and `docs/adr/` records how a rule or a design came to be. None of them
states a rule; when one appears to, the rule belongs in a phase document and the ADR
records why.

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
| **[ARCHITECTURE.md](ARCHITECTURE.md)** | What is the design **now**? | Why it came to be, history, plans, rules |
| **[docs/adr/](docs/adr/README.md)** | How did it come to be? What was tried and dropped? | The present design |
| **[CODING.md](CODING.md)** | What must code satisfy — RR-15 and the layering rules? | Design rationale |
| **[AUDITING.md](AUDITING.md)** | How is compliance checked? | The rules being checked |
| **[TESTING.md](TESTING.md)** | What must be verified before merging? | Test results |
| **[ROADMAP.md](ROADMAP.md)** | What is next, and what does done mean? | Completed history |
| **[PLANNING.md](PLANNING.md)** | How is a change planned before it is written? | Any specific plan |
| **docs/specs/** | Background on a subsystem. | Anything authoritative |
### Rules

1. **One fact, one home.** If it belongs in two places, one of them links instead of
   restating. `ARCHITECTURE.md` §4 links to ADRs rather than repeating them.
2. **Present tense is `ARCHITECTURE.md`; past tense is `docs/adr/`.** A decision that
   was reversed is recorded, not deleted, so the reasoning is not repeated.
3. **A quoted figure carries its date.** Measurements go stale. Either re-verify
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
   (`ARCHITECTURE.md` §4.3); what you want to know about the engine you measure.

---

## 🚀 Where the commands are

Each phase document carries the commands for its phase: planning in
[PLANNING.md](PLANNING.md), the rules and what checks them in [CODING.md](CODING.md),
what to run before merging in [TESTING.md](TESTING.md), and the audits in
[AUDITING.md](AUDITING.md). They were listed here too until 2026-08-29, in a fifth
place that had to be kept in step with four others.

