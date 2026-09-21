//! A form field created here, reopened, and reported as what it was given.
//!
//! **The check is the round trip, not the screen**
//! ([ADR-0087](../../../docs/adr/0087-a-form-field-is-created-here-not-only-filled.md)):
//! a form created here, written out and opened again, reports every field through the
//! same reader that reports somebody else's document.
//!
//! The engine read forms well and wrote them barely. `inspect interactive` reported every
//! field a document carried and `SetFormFieldValue` changed values in a form that already
//! existed; no operation created one, so a document this engine declares PDF/UA-2
//! conforming could have fields it could only complain about.

use fepdf::{FieldKind, IngestionOptions, NewField, Operation, PdfDocument, SaveOptions};

/// A one-page document with no form at all, so that what a field needs is created too.
fn blank() -> PdfDocument {
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] >>",
        ])
        .into_iter()
        .collect::<Vec<u8>>()
        .into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// The document through a file and back, which is where a created field has to survive.
fn round_trip(doc: &PdfDocument, name: &str) -> PdfDocument {
    let path =
        std::env::temp_dir().join(format!("fepdf_create_field_{name}_{}.pdf", std::process::id()));
    doc.save_with_options(&path, "2.0", &SaveOptions::default()).expect("it writes");
    let written = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);
    PdfDocument::open_with_options(written.into(), &IngestionOptions::default())
        .expect("what this engine wrote, this engine opens")
}

fn field(name: &str, kind: FieldKind) -> NewField {
    NewField {
        page: 0,
        rect: (20.0, 300.0, 220.0, 330.0),
        name: name.to_string(),
        tooltip: format!("what {name} is for"),
        kind,
    }
}

/// **Every one of the nine kinds is created and read back as the type it was given.**
///
/// The types are `/FT` and the kinds are nine, because `/Ff` decides the rest: bit 13 is
/// `Multiline` on a text field and nothing on any other.
#[test]
fn all_nine_kinds_are_created_and_reported() {
    let kinds = [
        ("plain", FieldKind::Text { value: "x".to_string() }, "Tx"),
        ("many", FieldKind::TextArea { value: "y".to_string() }, "Tx"),
        ("secret", FieldKind::Password, "Tx"),
        ("tick", FieldKind::CheckBox { on: true }, "Btn"),
        ("one_of", FieldKind::RadioButton { group: "g".to_string(), on: false }, "Btn"),
        ("press", FieldKind::PushButton { caption: "Go".to_string() }, "Btn"),
        (
            "drop",
            FieldKind::ComboBox { options: vec!["a".into(), "b".into()], value: "b".into() },
            "Ch",
        ),
        (
            "list",
            FieldKind::ListBox { options: vec!["a".into(), "b".into()], value: "a".into() },
            "Ch",
        ),
        ("sign", FieldKind::Signature, "Sig"),
    ];

    let mut doc = blank();
    for (name, kind, _) in &kinds {
        doc.apply(Operation::AddFormField(field(name, kind.clone())))
            .unwrap_or_else(|why| panic!("creating {name} failed: {why}"));
    }
    let reopened = round_trip(&doc, "nine");
    let form = fepdf::form_of(reopened.inner());

    assert!(form.declared, "the document this engine wrote declares no form");
    assert_eq!(form.terminal.len(), kinds.len(), "not every field came back");
    for (field, (name, _, wanted_type)) in form.terminal.iter().zip(kinds.iter()) {
        assert_eq!(
            field.qualified_name.as_deref(),
            Some(*name),
            "the fields came back in a different order"
        );
        assert_eq!(
            field.field_type.as_deref(),
            Some(*wanted_type),
            "{name} came back as a different type"
        );
        // The check ADR-0087 named: `inspect audit` finding a field without a `/TU` is
        // the failure, and a creator that wrote the defect it reports would be an
        // auditor writing its own findings.
        assert_eq!(
            field.tooltip.as_deref(),
            Some(format!("what {name} is for").as_str()),
            "{name} came back without the /TU it was given"
        );
    }
}

/// **A field is created holding what it was given to hold.**
///
/// A password is the exception and is exempt by name: writing one into a document puts it
/// in the document, and this engine will not.
#[test]
fn a_created_field_holds_the_value_it_was_given() {
    let mut doc = blank();
    doc.apply(Operation::AddFormField(field(
        "plain",
        FieldKind::Text { value: "typed".to_string() },
    )))
    .expect("the field is created");
    doc.apply(Operation::AddFormField(field(
        "drop",
        FieldKind::ComboBox { options: vec!["a".into(), "b".into()], value: "b".into() },
    )))
    .expect("the field is created");
    doc.apply(Operation::AddFormField(field("secret", FieldKind::Password)))
        .expect("the field is created");

    let form = fepdf::form_of(round_trip(&doc, "values").inner());
    let held = |name: &str| {
        form.terminal
            .iter()
            .find(|f| f.qualified_name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("{name} is not in the form"))
            .value
            .clone()
    };
    assert_eq!(held("plain").as_deref(), Some("typed"), "the text field lost its value");
    assert_eq!(held("drop").as_deref(), Some("b"), "the choice field lost its value");
    assert_eq!(held("secret"), None, "a password was written into the document");
}

/// **Every created field carries a `/TU`**, because a field without one is a Matterhorn
/// failure this engine already reports — and an auditor that created the defect it names
/// would be writing its own findings.
#[test]
fn a_field_with_no_tooltip_is_refused() {
    let mut doc = blank();
    let mut asked = field("anon", FieldKind::Text { value: String::new() });
    asked.tooltip = String::new();
    let error = doc.apply(Operation::AddFormField(asked)).expect_err("it refuses");
    assert!(error.to_string().contains("/TU"), "the refusal does not say what is missing: {error}");
}

/// A field with no name cannot be filled, and saying so beats writing one that cannot.
#[test]
fn a_field_with_no_name_is_refused() {
    let mut doc = blank();
    let error = doc
        .apply(Operation::AddFormField(field("  ", FieldKind::Text { value: String::new() })))
        .expect_err("it refuses");
    assert!(error.to_string().contains("no name"), "the refusal does not say why: {error}");
}

/// **A field this engine creates can be filled by the part of it that fills fields.**
///
/// Creating and filling are two halves of one thing, and a field created in a shape
/// `SetFormFieldValue` cannot reach would be a form only its maker can use.
#[test]
fn a_created_field_can_then_be_filled() {
    let mut doc = blank();
    doc.apply(Operation::AddFormField(field("who", FieldKind::Text { value: String::new() })))
        .expect("the field is created");
    doc.apply(Operation::SetFormFieldValue(fepdf::FormFieldSpec {
        name: "who".to_string(),
        value: fepdf::FormValue::Text("someone".to_string()),
    }))
    .expect("the value applies");

    let form = fepdf::form_of(round_trip(&doc, "filled").inner());
    assert_eq!(
        form.terminal[0].value.as_deref(),
        Some("someone"),
        "the field this engine created did not keep what was put in it"
    );
}
