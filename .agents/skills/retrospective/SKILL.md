---
name: retrospective
description: >-
  Post-development retrospective and friction analysis protocol.
  Use after development cycles to identify systemic improvements
  and evolve project rules and workflows.
---

# Retrospective & Friction Analysis Protocol

> [!IMPORTANT]
> **Continuous Improvement**: This protocol defines how fepdf learns from its own development friction to harden rules and workflows.

---

## 1. Structured Retrospective
- **Rule**: Every development cycle (Phase, Epic, or major Feature) MUST conclude with a post-mortem analysis.
- **Purpose**: Bridge the gap between "momentary mistakes" and "systemic safeguards."
- **Compliance Criterion**: A `retrospective` entry must be added to `docs/conventions/reflections.md` citing specific tool/workflow failures.

## 2. Friction Analysis

### 2.1. History Scanning
- Identify compilation errors, Clippy warnings, logical contradictions, or user feedback.
- Delve deep into the causes of trial and error.
- The specific occurrence points of friction and the underlying "hesitation" must be verbalized.

### 2.2. Categorization & Revision
- Categorize by type (e.g., Charter/Quality/Process) and revise conventions to be "more concrete and mechanical" without compromising existing philosophy.
- Files to be modified are identified, and draft revisions including specific "Criteria" are created.
- Consider whether the rule revision can be automated through "skillization" or "workflowization."

### 2.3. Reflection (Apply)
- After obtaining user consensus, modify the convention files and execute sync_docs to bring them up to date.
- `scripts/audit/verify_compliance.sh` must pass after convention modification.

## 3. Rule Distillation (R-P-C Format)
- **Rule**: Identified friction points must be evaluated for "Rule Potential." If structural, convert into a rule using the **Rule, Purpose, Criterion** format.
- **Purpose**: Systematically eliminate classes of errors rather than just patching them.
- **Compliance Criterion**: New protocols must be validated against existing RR-15 and HDD standards.

## 4. Workflow Feedback Loop
- **Rule**: If a workflow consistently leads to downstream failures, the workflow itself MUST be modified.
- **Purpose**: Optimize AI-developer collaboration for maximum reliability.
- **Compliance Criterion**: Updates to workflows must be verified through the next execution cycle.

## 5. Architectural Validation (Phase Closure)
- **Rule**: Every phase MUST include a step for validating core data structures and data flows.
- **Purpose**: Prevent architectural drift and technical debt accumulation.
- **Compliance Criterion**: A visual check (e.g., Mermaid diagrams) or static analysis report must be reviewed and documented.
