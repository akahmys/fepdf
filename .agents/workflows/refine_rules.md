---
description: Workflow for refining self-rules.
---

# [Workflow] Refine Rules

Autonomously improve (Self-Refinement) the conventions to increase predictability.

## Procedure

// turbo
1. **Analysis**: Read the latest insights from [docs/conventions/reflections.md](../../docs/conventions/reflections.md) and execute the [analyze_friction](../skills/analyze_friction.md) skill to identify causes of friction.
2. **Verification**: Verify whether the proposed improvements align with the philosophy of [RR-15](../rules/hardening.md) and can resolve the current issues.
3. **Operational Optimization**: Determine which layer (Protocol / Skill / Workflow) is most efficient for operating the improved rule.
4. **Proposal**: Present the proposed improvements and changes in operation to the user for consensus.
// turbo
5. **Reflection**: After approval, modify the convention files (Charter/Rules/Skills) and solidify the changes using [sync_docs](sync_docs.md).

> [!CAUTION]
> Relaxing fundamental principles (safety constraints) for the sake of "ease of development" is strictly prohibited. Always improve in a direction that is "safer and more mechanical."
