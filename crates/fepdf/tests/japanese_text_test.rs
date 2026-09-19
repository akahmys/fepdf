//! 図面-0001, written by this engine and read back out of the file.
//!
//! **This is the case the whole of Phase W started from.** `edit bates --prefix "図面-"`
//! drew a row of Latin glyphs and extracted as `-0001`, because the text was escaped into
//! a literal string byte by byte and shown through a `/Helvetica` nobody embedded. Six
//! bytes of UTF-8 became six character codes in a WinAnsi font, and the engine's own
//! reader reported both causes while the feature went on shipping.
//!
//! What it takes to do instead: read the face's own terms (`OS/2.fsType`), find the
//! glyphs through its `cmap`, subset the CFF it draws them with, embed that as a
//! `CIDFontType0`, write the **identifiers its collection gave those glyphs** rather than
//! their ids, and carry a `/ToUnicode` that says what each identifier stood for.
//!
//! Measured on the face this machine has: Hiragino, a CID-keyed CFF of 20,327 glyphs from
//! Adobe-Japan1-7, subsetted to five, in a 47,654-byte file.

use fepdf::{IngestionOptions, PdfDocument, SaveOptions};
use fepdf_doc::apply::font::{ShownText, show_text};

/// The face this machine offers for Japanese, if it offers one.
fn japanese_face() -> Option<(String, Vec<u8>)> {
    fepdf_model::document::fallback_fonts()
        .iter()
        .find(|(kind, _)| format!("{kind:?}").contains("Japanese"))
        .map(|(kind, data)| (format!("{kind:?}"), data.as_ref().clone()))
}

/// **Either it is written and comes back, or it is refused and the character is named.**
///
/// Both branches assert something, because a machine with no Japanese face installed is
/// not a machine where this test has nothing to say: it is where ADR-0090's refusal is
/// the right answer, and a test that skipped would not notice the refusal going wrong.
#[test]
fn japanese_is_written_and_read_back_or_refused_by_name() {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
    ];
    let doc = PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens");

    let Some((kind, program)) = japanese_face() else {
        return no_face_refuses_by_name(&doc);
    };

    show_text(
        doc.inner(),
        0,
        &ShownText {
            program: &program,
            base_font: "BatesFace",
            text: "図面-0001",
            at: (72.0, 700.0),
            size: 12.0,
        },
    )
    .unwrap_or_else(|e| panic!("{kind} draws 図 and the engine would not write it: {e}"));

    let path = std::env::temp_dir().join("fepdf_japanese_text_test.pdf");
    doc.save_with_options(&path, "2.0", &SaveOptions::default()).expect("it writes");
    let written = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);

    let reopened = PdfDocument::open_with_options(written.into(), &IngestionOptions::default())
        .expect("what this engine wrote, this engine opens");
    assert_eq!(
        reopened.extract_text(0).expect("the page extracts").trim(),
        "図面-0001",
        "the text did not survive the round trip"
    );

    // The defect this began with is two decisions the reader took about the output.
    let complaints: Vec<String> = reopened
        .decisions()
        .iter()
        .filter(|d| d.clause == "9.6.2" || d.clause == "9.10.2")
        .map(|d| format!("{} {}", d.clause, d.found))
        .collect();
    assert!(complaints.is_empty(), "the engine complains about its own output: {complaints:?}");

    assert!(
        reopened.fonts().iter().any(|f| f.is_embedded && f.font_type == "CIDFontType0"),
        "the CFF face is not embedded as a CIDFontType0: {:?}",
        reopened.fonts().iter().map(|f| (&f.font_type, f.is_embedded)).collect::<Vec<_>>()
    );
}

/// With no face that draws 図, the engine refuses and says which character stopped it.
fn no_face_refuses_by_name(doc: &PdfDocument) {
    let program = fepdf_fixtures::truetype_program(&[1024, 2048, 1024, 512]);
    let refused = show_text(
        doc.inner(),
        0,
        &ShownText {
            program: &program,
            base_font: "BatesFace",
            text: "図面-0001",
            at: (72.0, 700.0),
            size: 12.0,
        },
    );
    let message = refused.expect_err("no face here draws 図").to_string();
    assert!(message.contains('図'), "the refusal does not name the character: {message}");
}
