//! Filling a field by the name `form_of` gives it.
//!
//! **It filled nothing and said it had.** `SetFormFieldValue` compared the name it was
//! given, as raw UTF-8, with each field's own `/T`: a nested field's qualified name
//! matched no `/T`, and neither did any field of `sample_02c.pdf`, whose names are UTF-16.
//! A name that matched nothing — and a document with no form — answered `Ok`. The
//! window's form drawer lists fields by the names `form_of` reads, so on that sample
//! every "write" did nothing, silently. The value, when it was written, went in as the
//! UTF-8 bytes of the text, which a reader takes for PDFDocEncoding.

use fepdf::{FormFieldSpec, FormValue, IngestionOptions, Operation, PdfDocument};

fn sample() -> PdfDocument {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/sample_02c.pdf");
    let bytes = std::fs::read(path).expect("the sample is in this working copy");
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens")
}

fn fill(doc: &mut PdfDocument, name: &str, value: FormValue) -> fepdf::PdfResult<()> {
    doc.apply(Operation::SetFormFieldValue(FormFieldSpec { name: name.to_string(), value }))
}

fn field(doc: &PdfDocument, name: &str) -> fepdf::FormField {
    fepdf::form_of(doc.inner())
        .terminal
        .into_iter()
        .find(|field| field.qualified_name.as_deref() == Some(name))
        .expect("the form has the field")
}

fn reopened(doc: &PdfDocument) -> PdfDocument {
    let path = std::env::temp_dir().join(format!("fepdf_form_filling_{}.pdf", std::process::id()));
    doc.save_with_options(&path, "2.0", &fepdf::SaveOptions::default()).expect("it writes");
    let written = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);
    PdfDocument::open_with_options(written.into(), &IngestionOptions::default())
        .expect("what this engine wrote, this engine opens")
}

/// **A field named in Japanese is filled, and reads back as written through a save.**
#[test]
fn a_field_named_in_utf16_is_filled_and_its_value_survives_a_save() {
    let mut doc = sample();
    fill(&mut doc, "住所", FormValue::Text("東京都千代田区".into())).expect("the field is filled");
    assert_eq!(field(&doc, "住所").value.as_deref(), Some("東京都千代田区"));
    assert_eq!(
        field(&reopened(&doc), "住所").value.as_deref(),
        Some("東京都千代田区"),
        "the value did not survive the save as the text it was"
    );
}

/// A choice whose options are UTF-16 is chosen by its export value, and `/I` follows.
#[test]
fn a_choice_with_utf16_options_is_chosen() {
    let mut doc = sample();
    fill(&mut doc, "お見積り", FormValue::Choice("お見積りは必要です".into()))
        .expect("the choice is made");
    let chosen = field(&doc, "お見積り");
    assert_eq!(chosen.value.as_deref(), Some("お見積りは必要です"));
    assert_eq!(chosen.selected_indices, [1], "/I does not point at the option chosen");
}

/// **A name the form does not have is refused by name**, and nothing changes.
#[test]
fn a_field_the_form_does_not_have_is_refused() {
    let mut doc = sample();
    let before =
        fepdf::form_of(doc.inner()).terminal.iter().map(|f| f.value.clone()).collect::<Vec<_>>();
    let error = fill(&mut doc, "no such field", FormValue::Text("x".into())).expect_err("refused");
    assert!(error.to_string().contains("no such field"), "the refusal does not say which: {error}");
    let after =
        fepdf::form_of(doc.inner()).terminal.iter().map(|f| f.value.clone()).collect::<Vec<_>>();
    assert_eq!(before, after, "a refused fill changed the form");
}

/// And a document with no form says so, rather than answering that it filled a field.
#[test]
fn a_document_with_no_form_is_refused() {
    let mut doc = PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        ])
        .into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens");
    let error = fill(&mut doc, "anything", FormValue::Text("x".into())).expect_err("refused");
    assert!(error.to_string().contains("no form"), "the refusal does not say why: {error}");
}

/// **A nested field is named by its qualified name**, `parent.child`, which is what
/// `form_of` reports and what matched no `/T` before.
#[test]
fn a_nested_field_is_filled_by_its_qualified_name() {
    let mut doc = PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] >> >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [5 0 R] >>".to_string(),
            "<< /T (address) /Kids [5 0 R] >>".to_string(),
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (city) /Parent 4 0 R \
             /Rect [10 10 150 30] /P 3 0 R >>"
                .to_string(),
        ])
        .into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens");
    fill(&mut doc, "address.city", FormValue::Text("Kyoto".into())).expect("the field is filled");
    assert_eq!(field(&doc, "address.city").value.as_deref(), Some("Kyoto"));
}

/// **A check box is turned on by the state its appearance names**, not `/Yes`.
/// `sample_02c.pdf` names each box's on state after the box; writing `/Yes` found no
/// appearance for it, recorded a violation of 12.7.5.2.3 against the file, and left the
/// box drawn empty.
#[test]
fn a_check_box_is_turned_on_by_its_own_on_state() {
    let mut doc = sample();
    fill(&mut doc, "電話", FormValue::Boolean(true)).expect("the box is ticked");
    let value = field(&doc, "電話").value.expect("the box has a value");
    assert!(value != "/Yes" && value != "/Off", "the box was set to {value}, not its own state");
    let refused: Vec<_> =
        doc.decisions().into_iter().filter(|d| d.clause == "12.7.5.2.3").collect();
    assert!(refused.is_empty(), "the box's own state was reported missing: {refused:?}");

    fill(&mut doc, "電話", FormValue::Boolean(false)).expect("the box is cleared");
    assert_eq!(field(&doc, "電話").value.as_deref(), Some("/Off"));
}

/// **Radio buttons are refused an on/off**, because their widgets each name a different
/// state and "on" does not say which.
#[test]
fn a_set_of_radio_buttons_is_not_turned_on_without_saying_which() {
    let mut doc = sample();
    let error = fill(&mut doc, "相談", FormValue::Boolean(true)).expect_err("refused");
    assert!(error.to_string().contains("which"), "the refusal does not say why: {error}");
}
