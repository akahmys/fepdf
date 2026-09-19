//! Changing the text a page already draws.
//!
//! **The unit is a run**, which is one show-text operator: a run that reads what was asked
//! for is rewritten whole, and one that merely contains it is left alone. Splitting a run
//! means re-spacing what remains, and that is W-E4.
//!
//! What is asserted is the round trip a reader sees — the page is edited, written to a
//! file, opened again, and read — because an edit that only holds in the arena is an edit
//! nobody else can see.

use fepdf::{IngestionOptions, PdfDocument, SaveOptions};
use fepdf_doc::operation::Operation;

/// A page drawing `text` in Helvetica, at a known place.
fn page_drawing(text: &str) -> PdfDocument {
    let content = format!("BT /F1 24 Tf 1 0 0 1 40 700 Tm ({text}) Tj ET");
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// The document, through a file and back.
fn round_trip(doc: &PdfDocument, name: &str) -> PdfDocument {
    let path = std::env::temp_dir().join(format!("fepdf_edit_text_{name}.pdf"));
    doc.save_with_options(&path, "2.0", &SaveOptions::default()).expect("it writes");
    let written = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);
    PdfDocument::open_with_options(written.into(), &IngestionOptions::default())
        .expect("what this engine wrote, this engine opens")
}

#[test]
fn a_run_reads_what_it_was_changed_to() {
    let mut doc = page_drawing("ORIGINAL");
    assert!(doc.extract_text(0).expect("it extracts").contains("ORIGINAL"));

    doc.apply(Operation::EditTextRun {
        page: 0,
        find: "ORIGINAL".to_string(),
        replace: "CHANGED".to_string(),
    })
    .expect("the edit applies");

    let text = round_trip(&doc, "changed").extract_text(0).expect("it extracts");
    assert!(text.contains("CHANGED"), "the new text is not there: {text:?}");
    assert!(!text.contains("ORIGINAL"), "the old text is still there: {text:?}");
}

/// **A run that merely contains the text is left alone**, which is the line this draws
/// between editing a run and editing a range.
#[test]
fn a_run_that_only_contains_the_text_is_left_alone() {
    let mut doc = page_drawing("UNORIGINAL");
    doc.apply(Operation::EditTextRun {
        page: 0,
        find: "ORIGINAL".to_string(),
        replace: "CHANGED".to_string(),
    })
    .expect("the edit applies");

    let text = round_trip(&doc, "untouched").extract_text(0).expect("it extracts");
    assert!(
        text.contains("UNORIGINAL"),
        "a run that was not an exact match was rewritten: {text:?}"
    );
}

/// A character the run's font cannot draw is refused by name, and nothing is written.
#[test]
fn a_character_the_run_s_font_cannot_draw_is_refused() {
    let mut doc = page_drawing("ORIGINAL");
    let refused = doc.apply(Operation::EditTextRun {
        page: 0,
        find: "ORIGINAL".to_string(),
        replace: "図面".to_string(),
    });
    let message = refused.expect_err("Helvetica draws no 図").to_string();
    assert!(message.contains('図'), "the refusal does not name the character: {message}");

    let text = doc.extract_text(0).expect("it extracts");
    assert!(text.contains("ORIGINAL"), "the page changed although the edit was refused: {text:?}");
}

/// Asking for text no run reads changes nothing, and is not an error.
#[test]
fn asking_for_text_no_run_reads_changes_nothing() {
    let mut doc = page_drawing("ORIGINAL");
    doc.apply(Operation::EditTextRun {
        page: 0,
        find: "ABSENT".to_string(),
        replace: "CHANGED".to_string(),
    })
    .expect("it applies");
    assert!(doc.extract_text(0).expect("it extracts").contains("ORIGINAL"));
}

/// A page drawing `text` as one run split by a kerning correction, which is the ordinary
/// shape of text in a file a person did not hand-write.
fn page_drawing_kerned(text: &str) -> PdfDocument {
    let (head, tail) = text.split_at(text.len() / 2);
    let content = format!("BT /F1 24 Tf 1 0 0 1 40 700 Tm [({head}) -50 ({tail})] TJ ET");
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// **A run is the whole array, not each string in it.**
///
/// `[(ORIG) -50 (INAL)] TJ` is one run reading `ORIGINAL`, split where the producer kerned
/// it. Matching the pieces on their own finds neither, so the edit did nothing and said it
/// had succeeded — on the ordinary shape of real text rather than on an unusual one.
#[test]
fn a_run_split_by_kerning_is_still_one_run() {
    let mut doc = page_drawing_kerned("ORIGINAL");
    assert_eq!(
        doc.extract_spans(0).expect("it extracts").len(),
        2,
        "the fixture is not kerned, so this asks nothing"
    );

    doc.apply(Operation::EditTextRun {
        page: 0,
        find: "ORIGINAL".to_string(),
        replace: "CHANGED".to_string(),
    })
    .expect("the edit applies");

    let text = round_trip(&doc, "kerned").extract_text(0).expect("it extracts");
    assert!(text.contains("CHANGED"), "a kerned run was not edited: {text:?}");
    assert!(!text.contains("ORIG"), "part of the old run is still there: {text:?}");
}
