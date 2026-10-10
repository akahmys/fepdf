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
        Operation::ResizePages(..) => "ResizePages",
        Operation::AddLtvInfo { .. } => "AddLtvInfo",
        Operation::Retag => "Retag",
        Operation::Upgrade { .. } => "Upgrade",
        Operation::DeclareConformance { .. } => "DeclareConformance",
        Operation::UpdateStructElem(_) => "UpdateStructElem",
        Operation::DeleteStructElem { .. } => "DeleteStructElem",
        Operation::MoveStructElem(_) => "MoveStructElem",
        Operation::CreatePortfolio(_) => "CreatePortfolio",
        Operation::UpdateOutlines(_) => "UpdateOutlines",
        Operation::UpdateLayers(_) => "UpdateLayers",
        Operation::AttachAssociatedFile(_) => "AttachAssociatedFile",
        Operation::SetOutputIntent(_) => "SetOutputIntent",
        Operation::SetPronunciationLexicon { .. } => "SetPronunciationLexicon",
        Operation::AddPageDecoration { .. } => "AddPageDecoration",
        Operation::ApplyBatesNumbering { .. } => "ApplyBatesNumbering",
        Operation::AddAnnotation(_) => "AddAnnotation",
        Operation::RemoveAnnotation(_) => "RemoveAnnotation",
        Operation::EditAnnotation { .. } => "EditAnnotation",
        Operation::ReplyToAnnotation { .. } => "ReplyToAnnotation",
        Operation::SetAnnotationState { .. } => "SetAnnotationState",
        Operation::EditRun { .. } => "EditRun",
        Operation::SplitRun { .. } => "SplitRun",
        Operation::DeleteRun { .. } => "DeleteRun",
        Operation::MergeRuns { .. } => "MergeRuns",
        Operation::MoveRun { .. } => "MoveRun",
        Operation::RemoveOutside { .. } => "RemoveOutside",
        Operation::Redact(_) => "Redact",
        Operation::ApplyRedactAnnotations(_) => "ApplyRedactAnnotations",
        Operation::CropPages(..) => "CropPages",
        Operation::SplitPage { .. } => "SplitPage",
        Operation::CombinePages(..) => "CombinePages",
        Operation::AddFormField(..) => "AddFormField",
        Operation::SetMeasurementScale(_) => "SetMeasurementScale",
        Operation::SetFormFieldValue(_) => "SetFormFieldValue",
        Operation::SetTabOrder { .. } => "SetTabOrder",
        Operation::SetCalculationOrder(_) => "SetCalculationOrder",
        Operation::AddTextLayer { .. } => "AddTextLayer",
        Operation::EditXObject { .. } => "EditXObject",
        Operation::SetPageLabels(_) => "SetPageLabels",
        Operation::UpdateArticleThreads(_) => "UpdateArticleThreads",
        Operation::AddUserProperties { .. } => "AddUserProperties",
        Operation::SetStructAttribute(_) => "SetStructAttribute",
        Operation::SetStructRefs { .. } => "SetStructRefs",
        Operation::SetStructNamespace { .. } => "SetStructNamespace",
        Operation::AttachStructAssociatedFile { .. } => "AttachStructAssociatedFile",
        Operation::MarkArtifact { .. } => "MarkArtifact",
        Operation::WrapStructElem(_) => "WrapStructElem",
        Operation::MapStructType { .. } => "MapStructType",
        Operation::SetOpenAction(_) => "SetOpenAction",
        Operation::SetGeospatialAnchor(_) => "SetGeospatialAnchor",
        Operation::SetUnencryptedWrapper(_) => "SetUnencryptedWrapper",
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

/// The two orders W-F2-b added, in the shape an MCP caller writes them.
#[test]
fn the_two_orders_are_written_as_a_caller_would_write_them() {
    let op: Operation =
        serde_json::from_str(r#"{"SetTabOrder":{"pages":"All","order":"Structure"}}"#)
            .expect("parse");
    assert_eq!(
        op,
        Operation::SetTabOrder {
            pages: PageSelection::All,
            order: fepdf_doc::operation::TabOrder::Structure
        }
    );
    let op: Operation =
        serde_json::from_str(r#"{"SetCalculationOrder":["total","tax"]}"#).expect("parse");
    assert_eq!(op, Operation::SetCalculationOrder(vec!["total".into(), "tax".into()]));
}

#[test]
fn an_unknown_operation_name_is_refused_rather_than_ignored() {
    // `Redact`, `CreateLayer` and `AddStamp` were in `ARCHITECTURE.md`'s listing for four
    // phases without ever existing. A caller who believed that document should be told.
    assert!(serde_json::from_str::<Operation>(r#"{"Redact":{"zones":[]}}"#).is_err());
}

/// `SetStructAttribute` in the shape an MCP caller writes it.
#[test]
fn a_struct_attribute_is_written_as_a_caller_would_write_it() {
    use fepdf_doc::operation::{AttributeValue, StructAttribute};
    let op: Operation = serde_json::from_str(
        r#"{"SetStructAttribute":{"handle_index":7,"owner":"List","key":"ListNumbering","value":{"Name":"Decimal"}}}"#,
    )
    .expect("parse");
    assert_eq!(
        op,
        Operation::SetStructAttribute(StructAttribute {
            handle_index: 7,
            owner: "List".into(),
            key: "ListNumbering".into(),
            value: AttributeValue::Name("Decimal".into()),
        })
    );
}

/// The four review operations (AA-2a), in the shape an MCP caller writes them, and an
/// `AddAnnotation` written before `by` existed, which still reads.
#[test]
fn the_review_operations_are_written_as_a_caller_would_write_them() {
    use fepdf_doc::operation::{AnnotationAt, AnnotationState, Authorship};
    let at = AnnotationAt { page: 0, index: 2 };
    let op: Operation = serde_json::from_str(
        r#"{"SetAnnotationState":{"at":{"page":0,"index":2},"state":"Accepted","by":{"author":"Bo","when":null}}}"#,
    )
    .expect("parse");
    assert_eq!(
        op,
        Operation::SetAnnotationState {
            at,
            state: AnnotationState::Accepted,
            by: Authorship { author: Some("Bo".into()), when: None },
        }
    );
    let op: Operation =
        serde_json::from_str(r#"{"RemoveAnnotation":{"page":0,"index":2}}"#).expect("parse");
    assert_eq!(op, Operation::RemoveAnnotation(at));

    let old: Operation = serde_json::from_str(
        r#"{"AddAnnotation":{"page":0,"rect":[0,0,10,10],"kind":{"TextComment":{"contents":"x"}}}}"#,
    )
    .expect("an AddAnnotation without `by` still parses");
    let Operation::AddAnnotation(spec) = old else { panic!("{}", variant_name(&old)) };
    assert_eq!(spec.by, Authorship::default());
}
