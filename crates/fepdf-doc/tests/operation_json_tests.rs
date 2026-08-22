//! The `Operation` vocabulary as JSON.
//!
//! This is not a formality. `fepdf-mcp`'s `apply_operation` tool deserialises a
//! caller-supplied JSON string into an `Operation` and applies it, so the JSON *is* a
//! public interface — and [ADR-0025] rests on that path existing, because a script
//! processor is a fifth frontend producing the same values from JavaScript.
//!
//! Placed under `tests/` per RR-15 Rule 14. `fepdf-doc` had no `tests/` directory at all
//! until Rule D moved six operations into it.
//!
//! [ADR-0025]: ../../../docs/adr/0025-a-script-processor-is-a-frontend-not-a-subsystem.md

use fepdf_doc::operation::{Operation, PageSelection, PdfStandard};

/// Names every variant, exhaustively.
///
/// The value of this function is entirely in what it refuses to compile. RR-15 Rule 5
/// forbids a wildcard arm over a domain enum and `verify_compliance.sh` checks it, so a
/// variant added to `Operation` breaks the build here until someone decides what its JSON
/// should look like. Rule D added six variants in one change; without this the tests
/// below would have gone on passing while covering 80% of the vocabulary.
fn variant_name(op: &Operation) -> &'static str {
    match op {
        Operation::Rotate { .. } => "Rotate",
        Operation::Reorder { .. } => "Reorder",
        Operation::RemovePages(_) => "RemovePages",
        Operation::ReorderBatch { .. } => "ReorderBatch",
        Operation::DuplicatePages(_) => "DuplicatePages",
        Operation::InsertFrom { .. } => "InsertFrom",
        Operation::AddLtvInfo { .. } => "AddLtvInfo",
        Operation::Retag => "Retag",
        Operation::Upgrade { .. } => "Upgrade",
        Operation::UpdateStructElem(_) => "UpdateStructElem",
        Operation::DeleteStructElem { .. } => "DeleteStructElem",
        Operation::CreatePortfolio(_) => "CreatePortfolio",
        Operation::UpdateOutlines(_) => "UpdateOutlines",
        Operation::UpdateLayers(_) => "UpdateLayers",
        Operation::AttachAssociatedFile(_) => "AttachAssociatedFile",
        Operation::SetOutputIntent(_) => "SetOutputIntent",
        Operation::SetPronunciationLexicon { .. } => "SetPronunciationLexicon",
        Operation::AddPageDecoration { .. } => "AddPageDecoration",
        Operation::ApplyBatesNumbering { .. } => "ApplyBatesNumbering",
        Operation::AddAnnotation(_) => "AddAnnotation",
        Operation::SetMeasurementScale(_) => "SetMeasurementScale",
        Operation::SetFormFieldValue(_) => "SetFormFieldValue",
        Operation::SetPageLabels(_) => "SetPageLabels",
        Operation::UpdateArticleThreads(_) => "UpdateArticleThreads",
        Operation::AddUserProperties { .. } => "AddUserProperties",
        Operation::ExecuteAction(_) => "ExecuteAction",
        Operation::SetGeospatialAnchor(_) => "SetGeospatialAnchor",
        Operation::AddMeshShading(_) => "AddMeshShading",
        Operation::SetUnencryptedWrapper(_) => "SetUnencryptedWrapper",
        Operation::AddPublicKeyRecipient(_) => "AddPublicKeyRecipient",
    }
}

/// The six operations Rule D produced, which is the set nothing had exercised as JSON.
fn operations_rule_d_added() -> Vec<Operation> {
    vec![
        Operation::ReorderBatch { sources: vec![3, 1], target: 0 },
        Operation::DuplicatePages(PageSelection::Indices(vec![0, 2])),
        Operation::InsertFrom { source: b"%PDF-2.0\n".to_vec(), at: 1 },
        Operation::AddLtvInfo { certificates: vec![vec![0x30, 0x82]] },
        Operation::Retag,
        Operation::Upgrade { standard: PdfStandard::A4 },
    ]
}

#[test]
fn every_operation_rule_d_added_survives_a_json_round_trip() {
    for op in operations_rule_d_added() {
        let json = serde_json::to_string(&op).expect("serialise");
        let back: Operation = serde_json::from_str(&json).unwrap_or_else(|e| {
            panic!("{} did not deserialise from {json}: {e}", variant_name(&op))
        });
        assert_eq!(back, op, "{} changed across the round trip", variant_name(&op));
    }
}

#[test]
fn a_hand_written_json_string_reaches_the_right_variant() {
    // The shape an MCP caller — or a script frontend — actually sends. Written by hand
    // rather than produced by `to_string`, because a round trip agrees with itself even
    // when the format is not what a caller would guess.
    let op: Operation = serde_json::from_str(r#"{"Upgrade":{"standard":"A4"}}"#).expect("parse");
    assert_eq!(op, Operation::Upgrade { standard: PdfStandard::A4 });

    let op: Operation =
        serde_json::from_str(r#"{"ReorderBatch":{"sources":[3,1],"target":0}}"#).expect("parse");
    assert_eq!(op, Operation::ReorderBatch { sources: vec![3, 1], target: 0 });

    let op: Operation = serde_json::from_str(r#""Retag""#).expect("parse");
    assert_eq!(variant_name(&op), "Retag");
}

#[test]
fn an_unknown_operation_name_is_refused_rather_than_ignored() {
    // `Redact`, `CreateLayer` and `AddStamp` were in `ARCHITECTURE.md`'s listing for four
    // phases without ever existing. A caller who believed that document should be told.
    assert!(serde_json::from_str::<Operation>(r#"{"Redact":{"zones":[]}}"#).is_err());
}
