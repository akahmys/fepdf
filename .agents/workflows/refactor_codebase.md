---
description: Standard procedure for codebase refactoring with semantic analysis and compliance checks.
---

# [Workflow] Refactor Codebase

This workflow defines the high-reliability procedure for restructuring existing code without changing its external behavior. It ensures that every structural change is backed by semantic understanding and mechanical proof.

## Procedure

### Phase 1: Semantic Mapping & Discovery
// turbo
1. **Index Synchronization**: Run `ccc status` and `ccc index` to ensure the semantic map is current.
2. **Structural Analysis**: Use `ccc search` to map the logical structure and dependencies of the target components.
3. **Data Flow Verification**: Identify all incoming and outgoing data flows. Ensure `RefCell` usage and ownership patterns are understood.
4. **Specification Alignment**: Cite the relevant ISO 32000-2 Clauses that govern the logic being refactored.

### Phase 2: Baseline Stabilization (HDD)
// turbo
5. **Harness Construction**: Execute the `scaffold_harness` skill to create a verification environment that covers the current behavior.
6. **Reference Capture**: Capture reference outputs (Visual snapshots, Binary dumps, or Log traces) to establish an objective "Truth" baseline.
// turbo
7. **Baseline Check**: Run `cargo test` to confirm the workspace is stable before any changes.

### Phase 3: Logical Refinement (RR-15)
8. **Implementation**: Execute the refactoring in small, atomic steps.
    - Resolve borrowing errors by redesigning ownership (Rule 15).
    - Eliminate invalid states using type-safe Enums (Rule 8).
    - Ensure all functions stay under the size limit (Rule 1).
// turbo
9. **Continuous Check**: Run `cargo check` frequently during implementation to catch type-level regressions early.

### Phase 4: Differential Verification
// turbo
10. **Harness Execution**: Run the verification harness constructed in Phase 2.
11. **Differential Comparison**: Compare the new output against the reference baseline. Any deviation not explicitly intended is a regression.
12. **Logic Proof**: Confirm that the refactored data flow remains consistent with the original intent.

### Phase 5: Final Compliance & Quality Gate
// turbo
13. **Compliance Audit**: Execute the [verify_compliance](verify_compliance.md) workflow.
14. **Logical Audit**: Run the [verify_rr15](../skills/verify_rr15.md) skill for a final logical check.
15. **Artifact Recording**: Document the refactoring results, performance impact (if any), and proof of compliance in `walkthrough.md`.

## Completion Requirements
- The behavior is proven identical to the baseline (Differential Proof).
- Zero Clippy (Pedantic) warnings.
- 100% compliance with RR-15 and ISO 32000-2.
- The user has approved the `walkthrough.md`.
