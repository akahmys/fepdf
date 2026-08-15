# 📋 fepdf Planning & Discovery Protocol

This document governs task planning, codebase exploration, feature design, and decision-making workflows within the fepdf project.

---

## 🎯 1. Planning Workflow

Before a significant structural change, write the plan down. For a decision that is
contested, reversed, or rests on a measurement, the record belongs in
[`docs/adr/`](docs/adr/README.md); for sequencing, in [ROADMAP.md](ROADMAP.md).
There is no standing `implementation_plan.md` — the one under `docs/history/archive/`
is a record of an earlier effort and is not maintained.

### Implementation Plan Structure
1. **Goal Description**: Clear scope, rationale, and target outcomes.
2. **User Review Required**: Breaking changes, architectural choices, or design trade-offs.
3. **Open Questions**: Unresolved requirements or ambiguities.
4. **Proposed Changes**: Grouped logically by crate/component with `[NEW]`, `[MODIFY]`, or `[DELETE]` annotations.
5. **Verification Plan**: Automated tests (`cargo test`), compliance audits, and manual verification steps.

---

## 🔍 2. Codebase Discovery Protocol

Never guess implementation details, data schemas, or file locations. Follow this exploration protocol:

1. **Log-First Diagnostics**: Inspect full error tracebacks and empirical logs before forming diagnostic hypotheses.
2. **Complete Symbol Inspection**: View full struct, enum, and trait definitions rather than truncated code snippets.
3. **Registry & Dependency Audit**: Check crate manifests (`Cargo.toml`), workspace dependencies, and module exports (`mod.rs` / `lib.rs`).

---

## 🔄 3. Workflows & Session Management

- Task states and progress are tracked in `.agents/session/` artifacts (`task.md`, `walkthrough.md`).
- After completing work, update `walkthrough.md` with:
  - Concise summary of changes.
  - Verification results (test outputs, audit logs).
