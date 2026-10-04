//! **An annotation a redaction region meets goes, with what carries its content**
//! (12.5.6.23, ROADMAP Y-10): its popup, its replies, its place in the field tree and
//! the structure tree, and the form's `/XFA`.

use fepdf::{Operation, PdfDocument, Redaction, SaveOptions};

/// A 400-point page drawing nothing, with `annots` as its `/Annots` and `catalog` added to
/// the catalogue; `objects` are objects 4 on.
fn page_with(annots: &str, catalog: &str, objects: &[&str]) -> PdfDocument {
    let mut bodies = vec![
        format!("<< /Type /Catalog /Pages 2 0 R {catalog} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Resources << >> /Annots [{annots}] >>"
        ),
    ];
    bodies.extend(objects.iter().map(ToString::to_string));
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

/// The region over the lower-left quarter of the page.
fn redact(doc: &mut PdfDocument) {
    let redaction =
        Redaction { page: 0, regions: vec![(0.0, 0.0, 200.0, 200.0)], fill: Some(vec![]) };
    doc.apply(Operation::Redact(redaction)).expect("it redacts");
}

/// The saved file, uncompressed.
fn saved(doc: &PdfDocument, name: &str) -> Vec<u8> {
    let path =
        std::env::temp_dir().join(format!("fepdf-redact-annot-{name}-{}.pdf", std::process::id()));
    let options = SaveOptions { compress: false, obj_stm: false, ..SaveOptions::default() };
    doc.save_with_options(&path, "2.0", &options).expect("it writes");
    let file = std::fs::read(&path).expect("it is there");
    let _ = std::fs::remove_file(&path);
    file
}

/// Whether `file` holds `marker`, literally or as hexadecimal digits.
fn holds(file: &[u8], marker: &str) -> bool {
    let hex: String = marker.bytes().map(|b| format!("{b:02X}")).collect::<Vec<_>>().concat();
    let text = String::from_utf8_lossy(file);
    text.contains(marker) || text.to_uppercase().contains(&hex)
}

/// **A note inside goes with its popup and its reply; one outside stays**, and the
/// structure tree's reference to it does not keep it in the file.
#[test]
fn a_note_goes_with_its_popup_and_reply() {
    let mut doc = page_with(
        "4 0 R 5 0 R 6 0 R 7 0 R",
        "/StructTreeRoot 8 0 R /MarkInfo << /Marked true >>",
        &[
            "<< /Type /Annot /Subtype /Text /Rect [10 10 30 30] /Contents (MARKERNOTE) /Popup 5 0 R /StructParent 0 >>",
            "<< /Type /Annot /Subtype /Popup /Rect [210 210 300 300] /Parent 4 0 R >>",
            "<< /Type /Annot /Subtype /Text /Rect [250 250 270 270] /IRT 4 0 R /Contents (MARKERREPLY) >>",
            "<< /Type /Annot /Subtype /Text /Rect [300 300 320 320] /Contents (KEPTNOTE) >>",
            "<< /Type /StructTreeRoot /K [9 0 R] /ParentTree << /Nums [0 9 0 R] >> >>",
            "<< /Type /StructElem /S /Annot /P 8 0 R /K << /Type /OBJR /Obj 4 0 R >> >>",
        ],
    );
    let said = doc
        .what_redaction_removes(&Redaction {
            page: 0,
            regions: vec![(0.0, 0.0, 200.0, 200.0)],
            fill: None,
        })
        .expect("it reads");
    assert_eq!(said.annotations, [(10.0, 10.0, 30.0, 30.0)]);
    redact(&mut doc);
    let file = saved(&doc, "note");
    assert!(!holds(&file, "MARKERNOTE"), "the note is still in the file");
    assert!(!holds(&file, "MARKERREPLY"), "the reply is still in the file");
    assert!(holds(&file, "KEPTNOTE"), "the note outside the region went");
}

/// **A field whose only widget is inside goes from the tree, and the form's `/XFA` with
/// it**: the value is in the field, so taking the widget off the page leaves it written.
#[test]
fn a_field_with_its_only_widget_inside_goes() {
    let mut doc = page_with(
        "4 0 R",
        "/AcroForm 5 0 R",
        &[
            "<< /Type /Annot /Subtype /Widget /Rect [10 10 100 30] /FT /Tx /T (name) /V (MARKERVALUE) >>",
            "<< /Fields [4 0 R] /CO [4 0 R] /XFA 6 0 R >>",
            "<< /Length 21 >>\nstream\n<value>MARKERXFA</value>\nendstream",
        ],
    );
    redact(&mut doc);
    let file = saved(&doc, "field");
    assert!(!holds(&file, "MARKERVALUE"), "the field's value is still in the file");
    assert!(!holds(&file, "MARKERXFA"), "the XFA is still in the file");
    assert!(
        doc.decisions().iter().any(|d| d.found.contains("/XFA")),
        "the XFA's removal is not recorded"
    );
}

/// **A choice field's options go with it.**
#[test]
fn a_choice_fields_options_go_with_it() {
    let mut doc = page_with(
        "4 0 R",
        "/AcroForm << /Fields [4 0 R] >>",
        &[
            "<< /Type /Annot /Subtype /Widget /Rect [10 10 100 30] /FT /Ch /T (pick) /Opt [(MARKEROPTION)] >>",
        ],
    );
    redact(&mut doc);
    assert!(!holds(&saved(&doc, "choice"), "MARKEROPTION"));
}

/// **A field with a widget outside keeps its value**, which is shown there; the widget
/// inside leaves its `/Kids`.
#[test]
fn a_field_with_a_widget_outside_keeps_its_value() {
    let mut doc = page_with(
        "5 0 R 6 0 R",
        "/AcroForm << /Fields [4 0 R] >>",
        &[
            "<< /FT /Tx /T (twice) /V (SHOWNELSEWHERE) /Kids [5 0 R 6 0 R] >>",
            "<< /Type /Annot /Subtype /Widget /Rect [10 10 100 30] /Parent 4 0 R >>",
            "<< /Type /Annot /Subtype /Widget /Rect [210 210 300 230] /Parent 4 0 R >>",
        ],
    );
    redact(&mut doc);
    let file = saved(&doc, "twice");
    assert!(holds(&file, "SHOWNELSEWHERE"), "the field went with a widget still showing it");
    let arena = doc.inner().arena();
    let field =
        arena.get_object(arena.handle(4)).and_then(|o| o.as_dict_handle()).expect("the field");
    let kids = arena
        .dict_entry(field, arena.name("Kids"))
        .and_then(|k| k.as_array())
        .and_then(|k| arena.get_array(k));
    assert_eq!(kids.map(|k| k.len()), Some(1), "the widget inside is still a kid");
}

/// **A field whose widgets are all inside goes**, though they were its kids rather than
/// merged with it.
#[test]
fn a_field_whose_widgets_all_go_goes() {
    let mut doc = page_with(
        "5 0 R 6 0 R",
        "/AcroForm << /Fields [4 0 R] >>",
        &[
            "<< /FT /Tx /T (both) /V (MARKERBOTH) /Kids [5 0 R 6 0 R] >>",
            "<< /Type /Annot /Subtype /Widget /Rect [10 10 100 30] /Parent 4 0 R >>",
            "<< /Type /Annot /Subtype /Widget /Rect [10 50 100 70] /Parent 4 0 R >>",
        ],
    );
    redact(&mut doc);
    assert!(!holds(&saved(&doc, "both"), "MARKERBOTH"));
}
