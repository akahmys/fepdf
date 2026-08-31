# ADR-0048: Choice fields (`/FT /Ch`) are read and updated with appearance regeneration

- **Status**: Accepted
- **Date**: 2026-08-31
- **Commit**: TBD

## Context

ISO 32000-2:2020 Clause 12.7.4.4 defines Choice fields (`/FT /Ch`), representing scrollable list boxes and drop-down combo boxes.

Prior to this decision, the AcroForm walker in `interactive.rs` walked terminal fields and recognized `/FT /Btn`, `/FT /Tx`, `/FT /Sig`, and `/FT /Ch`, but did not parse choice-specific structures:
1. `/Opt`: the option list (array of strings or `[export display]` pairs).
2. `/I`: the array of selected option indices.
3. `/TI`: the top visible item index for list boxes.
4. Setting a choice value via `Operation::SetFormFieldValue` wrote `/V` as a `PdfName` rather than a text string and did not update `/I` or regenerate widget appearance streams (`/AP /N`).

Across the 524-file corpus, terminal `/Ch` fields occurred zero times (ROADMAP §"Read broadly, write 2.0" item P1). However, per fepdf Principle 3 (*"A corpus can justify building something. Only a use case can justify not building it"*), choice fields are standard PDF form components necessary for any interactive form containing dropdowns or list boxes.

## Decision

1. **Domain Model**:
   - Added `ChoiceOption { export_value: String, display_value: String }` representing single strings or export/display pairs.
   - Extended `FormField` with `options: Vec<ChoiceOption>`, `selected_indices: Vec<usize>`, and `top_index: Option<usize>`.
   - Added `is_combo()`, `is_editable_combo()`, and `is_multiselect()` predicates inspecting flags (Table 232).

2. **Inheritance & Reading (12.7.4.2 & 12.7.4.4)**:
   - `/Opt`, `/TI`, `/DA`, `/FT`, and `/Ff` are inherited down the field hierarchy.
   - `/I` (selected indices) is parsed from the dictionary or computed by matching `/V` against `/Opt`.

3. **Mutation & Appearance Regeneration**:
   - `apply_set_form_field_value` writes `/V` as `Object::String`.
   - When `/Opt` is present, `/I` is updated with the selected item index.
   - `refresh_appearance` resolves display text (from paired options) and builds the `/AP /N` stream using `/DA` font and quadding.

4. **Inspection & Verification**:
   - `fepdf inspect interactive` formats choice options and selection status.
   - Added integration test suite `crates/fepdf/tests/choice_field_test.rs` validating combo boxes, paired options, list boxes, multi-select, inheritance, and appearance generation.

## Consequences

- Item P1 in `ROADMAP.md` is resolved and marked as Built.
- Choice fields can be read, inspected, filled, and rendered across all frontends.
