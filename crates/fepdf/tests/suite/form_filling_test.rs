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

/// What a widget's normal appearance shows, read back through the font it is shown in, as
/// extraction reads it: the last `Tf`, and the hexadecimal `Tj` after it.
fn appearance_reads(doc: &PdfDocument, widget: u32) -> String {
    let arena = doc.inner().arena();
    let entry = |dict, key: &str| arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena));
    let widget =
        arena.get_object(arena.handle(widget)).and_then(|o| o.as_dict_handle()).expect("widget");
    let ap = entry(widget, "AP").and_then(|a| a.as_dict_handle()).expect("an appearance");
    let normal = entry(ap, "N").expect("a normal appearance");
    let content = doc.inner().decode_stream(&normal).expect("it decodes");
    let content = String::from_utf8_lossy(&content).into_owned();
    let tokens: Vec<&str> = content.split_whitespace().collect();
    let tf = tokens.iter().rposition(|t| *t == "Tf").expect("a Tf");
    let font_name = tokens[tf - 2].trim_start_matches('/');
    let hex = tokens.iter().find(|t| t.starts_with('<') && t.ends_with('>')).expect("codes in hex");
    let codes: Vec<u8> = (1..hex.len() - 1)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect();
    let stream = normal.as_dict_handle().expect("a stream");
    let fonts = entry(stream, "Resources")
        .and_then(|r| entry(r.as_dict_handle()?, "Font"))
        .and_then(|f| f.as_dict_handle())
        .expect("the appearance names fonts");
    let font = arena
        .dict_entry(fonts, arena.name(font_name))
        .and_then(|f| f.as_reference())
        .expect("the font is named by reference");
    // Through the loaded font, which is how extraction reads a code.
    let font = doc.inner().get_font(font).expect("the font loads");
    let mut read = String::new();
    let mut rest = codes.as_slice();
    while !rest.is_empty() {
        let (used, text) = font.decode_next(rest);
        read.push_str(&text.unwrap_or_default());
        rest = &rest[used.max(1)..];
    }
    read
}

/// **A filled field shows what was typed.** The value was written into its appearance as
/// UTF-8 bytes, which the form's font shows as codes of its own: 東京 in a field set in
/// Helvetica came out as six Latin letters, and in a Japanese form's font as three
/// characters nobody typed. A font that cannot show it hands it to a face that can.
#[test]
fn a_filled_field_shows_what_was_typed() {
    let mut doc = PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&[
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] /DA (/Helv 12 Tf 0 g) \
             /DR << /Font << /Helv 5 0 R >> >> >> >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Annots [4 0 R] >>",
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (city) /P 3 0 R /Rect [20 300 220 330] >>",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        ])
        .into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens");
    fill(&mut doc, "city", FormValue::Text("東京".into())).expect("the value is written");
    assert_eq!(appearance_reads(&doc, 4), "東京");
    // What the form's own font can show, it shows — é through WinAnsiEncoding's 0xE9.
    fill(&mut doc, "city", FormValue::Text("Café".into())).expect("the value is written");
    assert_eq!(appearance_reads(&doc, 4), "Café");
}
