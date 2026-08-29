# 📋 fepdf Planning & Discovery Protocol

> **Phase: planning.** What to decide before code is written. The rules for writing it
> are in [CODING.md](CODING.md).

This document governs task planning, codebase exploration, feature design, and decision-making workflows within the fepdf project.

---

## 🎯 1. Planning Workflow

Before a significant structural change, write the plan down. For a decision that is
contested, reversed, or rests on a measurement, the record belongs in
[`docs/adr/`](docs/adr/README.md); for sequencing, in [ROADMAP.md](ROADMAP.md).
There is no standing `implementation_plan.md`.

### Implementation Plan Structure
1. **Goal Description**: Clear scope, rationale, and target outcomes.
2. **User Review Required**: Breaking changes, architectural choices, or design trade-offs.
3. **Open Questions**: Unresolved requirements or ambiguities.
4. **Proposed Changes**: Grouped logically by crate/component with `[NEW]`, `[MODIFY]`, or `[DELETE]` annotations.
5. **Verification Plan**: `cargo test --workspace`, `./scripts/audit/verify_compliance.sh`,
   and the checks in [TESTING.md](TESTING.md) that the release-mode suite cannot make —
   `cli_smoke.sh` in a debug build, and `crosscheck_roundtrip.sh` against a second
   implementation. State which one would fail if the change were wrong.

---

## 🔍 2. Codebase Discovery Protocol

Never guess implementation details, data schemas, or file locations. Follow this exploration protocol:

1. **Measure, do not read.** Run something. A function's name, a doc comment and a
   governance document are all claims about the code, and this project has reversed
   decisions taken from each of them (`AGENTS.md`, Hierarchy of Truth). Note that
   "log-first" is not available here even when it sounds right: the engine holds one
   `log::warn!` by design, and what it finds in a document it records as a `Decision`
   (`ARCHITECTURE.md` §4.3).
2. **Establish the search finds nothing by making it find something.** An absent call
   site, an unfired gate and a broken grep look identical. Put the thing back and watch
   the check fail before believing it passes.
3. **Complete symbol inspection.** View whole struct, enum and trait definitions rather
   than truncated snippets.
4. **Registry and dependency audit.** Check crate manifests (`Cargo.toml`), workspace
   dependencies, and module exports (`mod.rs` / `lib.rs`).

---

