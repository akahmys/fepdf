//! **What this engine writes on a page comes back out of it.**
//!
//! This is the question the whole of Phase W's font work exists to answer, and the defect
//! it started from failed it: `edit bates --prefix "図面-"` drew a row of Latin glyphs and
//! extracted as `-0001`, because the text was escaped into a literal string byte by byte
//! and shown through a `/Helvetica` nobody embedded.
//!
//! Here the face is embedded and subsetted, the codes written are glyph ids through
//! `Identity-H`, and `/ToUnicode` says what each id stood for. The test reads the page
//! back through this engine's own extraction, which is what a reader's viewer stands in
//! for — and it reads it **after a round trip through a file**, so the writer is in the
//! loop rather than only the arena.

use fepdf::{IngestionOptions, PdfDocument, SaveOptions};
use fepdf_doc::apply::font::{ShownText, show_text};

/// A one-page document with nothing on it.
fn blank_page() -> PdfDocument {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
    ];
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// The document, written out and opened again.
///
/// **`name` is per test, because these run in parallel.** One path shared between two of
/// them had each reading what the other wrote, and the failure read as a lost character
/// rather than as a collision.
fn round_trip(doc: &PdfDocument, name: &str) -> PdfDocument {
    let path = std::env::temp_dir().join(format!("fepdf_written_text_{name}.pdf"));
    doc.save_with_options(&path, "2.0", &SaveOptions::default()).expect("it writes");
    let written = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);
    PdfDocument::open_with_options(written.into(), &IngestionOptions::default())
        .expect("what this engine wrote, this engine opens")
}

#[test]
fn text_written_in_an_embedded_face_survives_a_round_trip() {
    let program = fepdf_fixtures::truetype_program(&[1024, 2048, 1024, 512]);
    let doc = blank_page();
    show_text(
        doc.inner(),
        0,
        &ShownText {
            program: &program,
            base_font: "TestFace",
            text: "ABC",
            at: (72.0, 700.0),
            size: 12.0,
        },
    )
    .expect("it draws");

    let reopened = round_trip(&doc, "reads_back");
    let text = reopened.extract_text(0).expect("the page extracts");
    assert!(text.contains("ABC"), "what was written did not come back: {text:?}");
}

/// **The font that comes back is embedded**, which is what the audit asks of a document
/// claiming to conform and what the old path never produced.
#[test]
fn the_face_the_text_is_set_in_is_in_the_file() {
    let program = fepdf_fixtures::truetype_program(&[1024, 2048, 1024, 512]);
    let doc = blank_page();
    show_text(
        doc.inner(),
        0,
        &ShownText {
            program: &program,
            base_font: "TestFace",
            text: "AB",
            at: (72.0, 700.0),
            size: 12.0,
        },
    )
    .expect("it draws");

    let reopened = round_trip(&doc, "is_embedded");
    let fonts = reopened.fonts();
    assert!(!fonts.is_empty(), "the page names no font at all");
    assert!(
        fonts.iter().all(|f| f.is_embedded),
        "a face this engine wrote is not embedded: {:?}",
        fonts.iter().map(|f| (&f.name, f.is_embedded)).collect::<Vec<_>>()
    );
    // `/ToUnicode` belongs to the Type 0 font and not to the CIDFont under it, so the
    // question is whether the face a reader reaches has one.
    assert!(
        fonts.iter().any(|f| f.font_type == "Type0" && f.has_to_unicode),
        "no Type 0 face carries a ToUnicode: {:?}",
        fonts.iter().map(|f| (&f.font_type, f.has_to_unicode)).collect::<Vec<_>>()
    );
}

/// A character the face cannot draw is named back, and nothing is written.
#[test]
fn a_character_the_face_does_not_draw_is_refused_by_name() {
    let program = fepdf_fixtures::truetype_program(&[1024, 2048, 1024, 512]);
    let doc = blank_page();
    let refused = show_text(
        doc.inner(),
        0,
        &ShownText {
            program: &program,
            base_font: "TestFace",
            text: "図",
            at: (72.0, 700.0),
            size: 12.0,
        },
    );
    let message = refused.expect_err("a face with no CJK cannot set 図").to_string();
    assert!(message.contains('図'), "the refusal does not name the character: {message}");
}
