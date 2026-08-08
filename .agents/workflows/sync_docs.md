---
description: Workflow for synchronizing documents (ROADMAP, task, README, design specs).
---

# [Workflow] Sync Docs

Maintain the SSoT (Single Source of Truth) based on the actual implementation state.

## Procedure

1. **Information Gathering**: Verify parsing code, test results, Arlington validation status, and the build status of each crate under `crates/`.
// turbo
2. **ROADMAP Sync**: Update the progress in [ROADMAP.md](../../ROADMAP.md) to reflect the latest milestone status.
// turbo
3. **task Sync**: Check off completed items in [.agents/session/task.md](../session/task.md) and clearly document upcoming tasks.
// turbo
4. **Session State Sync**: Update [.agents/session/handoff.md](../session/handoff.md) and [.agents/session/lessons_learned.md](../session/lessons_learned.md) based on the lessons learned from this session.
// turbo
5. **Spec Update**: Reflect changes in parsing/rendering logic and progress in UI integration in the respective design documents under [specs/](../../docs/specs/).
// turbo
6. **README Sync**: Reflect the current phase and RR-15 compliance status in [README.md](../../README.md).

## Completion Requirements

- All documents accurately describe the capabilities of the current codebase.
- No broken links.
