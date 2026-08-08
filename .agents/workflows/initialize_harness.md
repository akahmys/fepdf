---
description: Fastest initialization of a verification harness based on the HDD protocol.
---

# [Workflow] Initialize Harness

Define the passing conditions (HDD) prior to implementation.

## Procedure

1. **Specification Identification**: Identify the relevant object and Clause from the [HDD_PROTOCOL](../skills/harness.md) and ISO 32000-2.
2. **Model Reference**: Check expected field definitions, types, and constraints from the Arlington PDF Model.
// turbo
3. **Harness Construction**: Execute the `scaffold_harness` skill to create a failing test under `tests/`.
// turbo
4. **FAIL Confirmation**: Run `cargo test` and confirm that it fails as intended.

## Completion Requirements
- Links to specifications are included in the code comments.
- The test correctly proves the current "deficiency."
