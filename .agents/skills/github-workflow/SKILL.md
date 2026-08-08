---
name: github-workflow
description: >-
  GitHub branch management, PR creation, merge protocol, and CI governance.
  Use when creating branches, submitting PRs, or merging to main.
---

# GitHub Workflow Protocol

> [!IMPORTANT]
> **Strict Governance**: The `main` branch is a protected foundation. All merges must be backed by objective evidence of compliance.

---

## 1. Branch Management

### 1.1. Milestone-per-Branch Lifecycle
- Every new feature or fix must be developed in a dedicated branch (`feat/phaseN-...` or `fix/...`).
- Lifecycle: Branch creation → Development → Local Verification → PR Submission → CI Review → User Review → Squash & Merge → Branch Deletion.

### 1.2. GitHub Operations
- Use the `gh` CLI and `git` to autonomously perform tasks from branch creation to Draft PR.
- Obtain consensus on the approach at the Draft PR stage before commencing main implementation.

## 2. Linear History Policy
- Maintain a strictly linear Git history on `main`.
- Use **Squash and Merge** (default) or **Rebase and Merge**. Merge commits (`--no-ff`) are prohibited.

## 3. Merge Triggers
A merge to `main` must occur ONLY when ALL conditions are met:
1. **Task Finalization**: All items in `task.md` marked as `[x]`.
2. **Zero-Warning Audit**: `scripts/audit/verify_compliance.sh` returns PASS with 0 warnings, including Secret & PII Scan.
3. **User Approval**: User has reviewed `walkthrough.md` and provided explicit approval.
4. **Linear Pre-Check**: Feature branch rebased onto latest `main`.

## 4. Evidence-Based Pull Requests
Every PR must contain:
1. A link to the corresponding `walkthrough.md`.
2. A summary of the `verify_compliance.sh` audit result.
3. Proof of visual validation (screenshots/recordings) if UI/Rendering was affected.

## 5. Mandatory CI Gate
The RR-15 Compliance Audit GitHub Action MUST return "Success" before merge:
- Zero `unwrap`/`expect` violations.
- 100% test pass rate.
- No license conflicts.

## 6. Prototype Isolation
- Logic from legacy prototypes must be migrated through this protocol even if previously "complete."
- Prevent the silent leak of non-compliant code into the main foundation.
