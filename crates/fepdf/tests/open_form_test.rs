//! The form an open document carries, rather than the one a file on disk does.
//!
//! **`InteractiveReport::survey` reads bytes**, which is what an audit of a file wants and
//! what a window cannot use: the document a reader is filling in has been changed since it
//! was opened, and serialising it again to ask what is in it would answer about a file
//! nobody has.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_doc::operation::Operation;
use fepdf_fixtures::{FormField, acroform};
use fepdf_model::document::extensions::{FormFieldSpec, FormValue};

fn opened(bytes: Vec<u8>) -> PdfDocument {
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the fixture opens")
}

/// **The fields are the ones the document has now.**
///
/// A window listing a form has to list the form as it stands, not as it was saved. This
/// sets a value and asks again.
#[test]
fn the_form_read_from_an_open_document_is_the_one_it_has_now() {
    let mut doc = opened(acroform(&[FormField::new("who", "before")], &[]));

    let form = fepdf::form_of(doc.inner());
    assert!(form.declared, "the fixture carries no /AcroForm");
    assert_eq!(form.terminal.len(), 1, "the form does not have the one field it was given");
    assert_eq!(
        form.terminal[0].value.as_deref(),
        Some("before"),
        "the field does not read what the fixture put in it"
    );

    doc.apply(Operation::SetFormFieldValue(FormFieldSpec {
        name: "who".to_string(),
        value: FormValue::Text("after".to_string()),
    }))
    .expect("the value applies");

    let form = fepdf::form_of(doc.inner());
    assert_eq!(
        form.terminal[0].value.as_deref(),
        Some("after"),
        "the form still reads the value the document was opened with"
    );
}

/// A document with no form says so, rather than failing to read one that is not there.
#[test]
fn a_document_with_no_form_declares_none() {
    let doc = opened(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
        ])
        .into_iter()
        .collect(),
    );
    let form = fepdf::form_of(doc.inner());
    assert!(!form.declared, "a document with no /AcroForm declared one");
    assert!(form.terminal.is_empty(), "it listed fields it does not have");
}

/// The field's type and name come with it, which is what a window needs to know what to
/// draw for it.
#[test]
fn a_field_says_what_it_is_and_what_it_is_called() {
    let doc = opened(acroform(&[FormField::new("who", "x"), FormField::new("what", "y")], &[]));
    let form = fepdf::form_of(doc.inner());

    let named: Vec<&str> =
        form.terminal.iter().filter_map(|f| f.qualified_name.as_deref()).collect();
    assert_eq!(named, vec!["who", "what"], "the fields are not the ones the fixture named");
    for field in &form.terminal {
        assert!(
            field.field_type.is_some(),
            "field {:?} says nothing about what type it is",
            field.qualified_name
        );
    }
}

/// **A real form, read through the same entry point the window uses.**
///
/// Every form test until this one was a fixture: a document built by hand with the fields
/// the test wanted. `sample_02c.pdf` is a form somebody else made, and what it says is
/// what the window will be shown.
///
/// Measured 2026-09-21: 30 fields, 19 text, 7 button, 4 choice — and **not one of them
/// carries a `/TU`**. That is the Matterhorn failure
/// [ADR-0087](../../../docs/adr/0087-a-form-field-is-created-here-not-only-filled.md) was
/// taken over: a defect this engine could name and not repair. It can name which field
/// now, rather than how many.
#[test]
fn a_real_form_reports_its_fields_and_what_they_are_missing() {
    let bytes = std::fs::read("../../samples/sample_02c.pdf").expect("the sample is there");
    let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens");
    let form = fepdf::form_of(doc.inner());

    assert!(form.declared, "the sample no longer carries the form this was written about");
    assert_eq!(form.terminal.len(), 30, "the sample no longer has the fields it was measured with");
    assert_eq!(
        form.by_type,
        vec![("Tx".to_string(), 19), ("Btn".to_string(), 7), ("Ch".to_string(), 4)],
        "the sample's fields are no longer the mix this was measured with"
    );

    let missing: Vec<&str> = form
        .terminal
        .iter()
        .filter(|field| field.tooltip.is_none())
        .filter_map(|field| field.qualified_name.as_deref())
        .collect();
    assert_eq!(
        missing.len(),
        30,
        "the sample's fields have gained a /TU, which changes what this documents"
    );
    assert!(
        missing.contains(&"名前漢字"),
        "the field this names is not among the ones reported: {missing:?}"
    );
}

/// **A window drawing this form is shown all three kinds at once.**
///
/// The drawer draws something different for a text field, a button and a choice, and
/// every fixture until now had one kind in it. This form has all three, so what it lists
/// is what a reader will be handed.
#[test]
fn a_real_form_carries_all_three_kinds_the_drawer_draws() {
    let bytes = std::fs::read("../../samples/sample_02c.pdf").expect("the sample is there");
    let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens");
    let form = fepdf::form_of(doc.inner());

    for wanted in ["Tx", "Btn", "Ch"] {
        assert!(
            form.terminal.iter().any(|f| f.field_type.as_deref() == Some(wanted)),
            "the form has no {wanted} field, so the drawer's arm for it is untried"
        );
    }
    assert!(
        form.terminal
            .iter()
            .filter(|f| f.field_type.as_deref() == Some("Ch"))
            .any(|f| !f.options.is_empty()),
        "no choice field offers any option, so the drawer would fall back to a text box"
    );
}
