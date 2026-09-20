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
