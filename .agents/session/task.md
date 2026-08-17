# Session Task (WAL)

## Phase D Operation Stubs Resolution (Tier 1 & Tier 2/3)

- [x] **Task 1**: Implement `Operation::SetPageLabels` (Clause 12.4.2 Number Tree under `/Catalog -> /PageLabels`)
- [x] **Task 2**: Implement `Operation::CreatePortfolio` (Clause 12.3.5 `/Collection` & `/Names -> /EmbeddedFiles`)
- [x] **Task 3**: Implement `Operation::AttachAssociatedFile` (Clause 14.13 `/AF` & `/Type /Filespec`)
- [x] **Task 4**: Implement `Operation::UpdateOutlines` (Clause 12.3.3 Outline Tree `/Outlines`)
- [x] **Task 5**: Implement `Operation::SetOutputIntent` (Clause 14.11.2 `/OutputIntents`)
- [x] **Task 6**: Implement `Operation::UpdateLayers` (Clause 8.11 `/OCProperties`)
- [x] **Task 7**: Un-hide verified CLI subcommands in `fepdf-cli` (`page-label`, `portfolio`, `attach`)
- [x] **Task 8**: Add tests in `crates/fepdf-sdk/tests/` and update `scripts/dev/status.sh`
- [x] **Task 9**: Run `verify_compliance.sh` and workspace tests

## Completed Milestones: Tier 1, Tier 2 & Tier 3 Operations

- [x] **Task 10**: Implement `Operation::AddAnnotation` (Clause 12.5 Annotation on target Page `/Annots`)
- [x] **Task 11**: Implement `Operation::SetFormFieldValue` (Clause 12.7 AcroForm field `/V` value update)
- [x] **Task 12**: Implement `Operation::SetGeospatialAnchor` (Clause 13.10 / 14.12.2 Viewport `/VP` & `/Measure`)
- [x] **Task 13**: Implement `Operation::ApplyBatesNumbering` (Clause 14.11.7 Bates numbering text synthesis)
- [x] **Task 14**: Implement `Operation::AddPageDecoration` (Header/Footer/Watermark text overlay)
- [x] **Task 15**: Un-hide verified CLI subcommands in `fepdf-cli` (`geo`, `bates`)
- [x] **Task 16**: Add tests in `crates/fepdf-sdk/tests/backend_operations_test.rs` and update `scripts/dev/status.sh` (expect 8)
- [x] **Task 17**: Run `verify_compliance.sh` and workspace tests




