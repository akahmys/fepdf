---
description: Overall reorganization and cleanup of project conventions, skills, and workflows.
---

# [Workflow] Refactor Conventions

Eliminate redundancy in conventions as the project grows and rebuild the SSoT (Single Source of Truth).

## Procedure

// turbo
1. **Global Audit**: Scan [rules.md](../rules.md), [rules/](../rules/), [skills/](../skills/), and [workflows/](./) to identify redundant descriptions, broken links, or stale references.
// turbo
2. **Infrastructure Renewal**: Ensure a clear separation between the Charter ([rules.md](../rules.md)) and operational governance ([skills/planning.md](../skills/planning.md)), clarifying the division of responsibilities.
// turbo
3. **Unification of Descriptions**: Standardize each convention (e.g., [hardening.md](../rules/hardening.md)) into the "Rule, Purpose, Criterion" format to eliminate ambiguity.
// turbo
4. **Extension of Automation**: Add `// turbo` annotations to all workflows and fix link paths to reflect the latest layout.
// turbo
5. **Organization**: Delete unnecessary temporary files, logs, and integrate misplaced specifications into the `docs/specs/` directory.
// turbo
6. **Hardening Audit**: Verify compliance with [RR-15](../rules/hardening.md) (XRef support, caching patterns).

// turbo
7. **Sync Verification**: Execute [sync_docs](sync_docs.md) to perform a final consistency check across all documents.

## Completion Requirements

- Maintenance of the hierarchical structure (Constitution -> Rules -> Skills -> Workflows)
- Equivalent quality assurance via `scripts/audit/verify_compliance.sh`
- All links are functional
