---
name: code-audit
description: >-
  RR-15 compliance audit and ISO 32000-2 verification procedure.
  Use when auditing code for safety, compliance, or preparing for merge.
---

# Code Audit Protocol

Audit the codebase from multiple perspectives to ensure compliance with RR-15 (`.agents/rules/code-safety.md`) and ISO 32000-2.

---

## 1. Static & Mechanical Audit

- **Action**: Run `./scripts/audit/verify_compliance.sh`.
- **Purpose**: Detect function length violations, unwrap/expect usage, unsafe blocks, static mut, and non-deterministic collections (HashMap).
- **Note**: If the script yields an error, fixing it must be the top priority.

## 2. Semantic & Structural Audit (via ccc)

- **Action**: Run `ccc status` and `ccc index` to ensure the semantic map is up-to-date.
- **Action**: Use `ccc search "<query>"` to verify consistent application of design patterns.
- **Purpose**: Identify "Implementation Gaps" where architectural changes have been applied to core components but missed in edge crates.

## 3. Logical & Architectural Audit

Leverage the AI's contextual understanding to identify violations difficult for scripts to detect:

- **Rule 4 (Nesting)**: Overlapping complex `if let` or `match` statements? Room for flattening with `?`?
- **Rule 6 (Stack Safety)**: Unbounded recursive calls? Can they be converted to loops with `Vec`?
- **Rule 8 (Invalid State)**: Can logic be expressed using type-safe Enums instead of `Option`/`Result`?
- **Rule 15 (Cloning)**: Is `.clone()` truly necessary? Can it be resolved through `Arc` or ownership transfer?

### 3.1. ISO 32000-2 Compliance Guard

- Does the ingestion pipeline call `perform_pass_0_normalization` before structural work?
- Are binary streams correctly excluded from the text-based refinery pipeline?
- Are workspace-wide clippy lints inherited by all member crates (`lints.workspace = true`)?

## 4. Reporting Audit Results

If violations are found, report them in this format:
1. **Violation Location**: Filename and line number.
2. **Violated Rule**: RR-15 rule number or ISO clause.
3. **Recommended Fix**: Concrete code snippets.

## 5. Completion Criterion

- [ ] `verify_compliance.sh` PASSES.
- [ ] `cargo clippy --pedantic` yields no warnings.
- [ ] Logical audit confirms no further room for improvement.
