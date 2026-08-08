---
description: Automated workflow for bug fixing based on FIX_PROTOCOL.
---

# [Workflow] Fix via Diagnosis

Fixing is not development. Identify the root cause of the bug and resolve it with minimal changes.

## Procedure

1. **Session Initialization** [Project Manager]: Follow the startup instructions in [.agents/rules/constitution.md](../../.agents/rules/constitution.md), loading handoff and lessons_learned.
// turbo
2. **Describe the Phenomenon** [Project Manager]: Observe the phenomenon and accurately record the current status in [.agents/session/task.md](../session/task.md).
// turbo
3. **Verify Past History** [Project Manager]: Read [.agents/session/lessons_learned.md](../session/lessons_learned.md) and list past failure patterns.
// turbo
4. **Enumerate Hypotheses** [Project Manager]: List **at least three** hypotheses in task.md explaining why the issue is occurring.
// turbo
5. **Design & Execute Diagnosis** [Compliance Auditor]: Define the exact "Proof of Failure" by writing a failing reproduction test. Do not change production code.
// turbo
6. **Implement Minimal Change** [Engineer]: Modify only one location in the production code to resolve the reproduction test.
// turbo
7. **Verify Effect** [Engineer]: Perform self-verification to ensure the reproduction test passes.
// turbo
8. **Regression Check** [Compliance Auditor]: Run `scripts/audit/verify_compliance.sh`, cargo tests, and external audits to prove that no new breakage has occurred.
// turbo
9. **Root-Cause Reflection** [Compliance Auditor & Project Manager]: Analyze the bug's cause into a lesson in [docs/conventions/reflections.md](../../docs/conventions/reflections.md) and `lessons_learned.md`. Determine if a new protocol entry is required to prevent recurrence.
// turbo
10. **Record & Finish** [Project Manager]: Record final results in task.md, lessons_learned.md, and handoff.md, then end the session.

## Completion Requirements

- A bug reproduction test has been added to the test suite and is PASSING.
- All existing tests are PASSING.
- Empirical evidence is recorded in [.agents/session/walkthrough.md](../session/walkthrough.md).
- Final approval has been obtained from the user.
