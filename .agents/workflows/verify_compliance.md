---
description: Automated verification workflow for RR-15 compliance.
---

# [Workflow] Verify Compliance & Structural Audit

This workflow serves as the **Standard Procedure for Codebase Audit and Refactoring**. It ensures both structural integrity via semantic analysis and logical compliance via the RR-15 hardening standards.

## Procedure

// turbo
1. **Semantic Audit (ccc)**: Run `ccc status` and `ccc index` to update the map, then use `ccc search` to detect architectural inconsistencies or implementation gaps (e.g., missed variant updates).
2. **AI Logic Audit (RR-15)**: Follow the [verify_rr15](../skills/verify_rr15.md) skill to self-check for logical violations of the [Hardening Rules](../rules/hardening.md).
3. **Automated Mechanical Audit**: Execute the following and confirm that all items PASS.
// turbo
   - `./scripts/audit/verify_compliance.sh` (Convention compliance)
// turbo
   - `cargo clippy --pedantic` (Zero warnings)
// turbo
   - `./scripts/audit/msrv_check.sh` (Maintenance of compatibility)
4. **Artifact Recording**: Record audit results, discovered issues, and fix details in `walkthrough.md`.

## Completion Requirements
- Not a single warning or inconsistency remains.
- The AI's logical audit yields a judgment of "no room for improvement."
- If fixes are necessary, feature additions are suspended and fixes are prioritized.
