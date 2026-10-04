//! **A form's font name survives a save** (7.8.3, ROADMAP Y-F34).
//!
//! A form with no resources of its own reads them from what draws it. The content parser
//! read such a form alone, found its `/F1` defined nowhere, and kept `/Fallback-Sans` in
//! its place — a name no resource defines — which every save then wrote out, so the
//! form's text lost its font in the file. Found 2026-10-04 redacting such a form.

use fepdf::{PdfDocument, SaveOptions};

#[test]
fn a_form_reading_the_pages_font_keeps_its_name() {
    let content = "BT /F1 20 Tf 10 100 Td (SECRET) Tj ET";
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 400] /Contents 4 0 R \
           /Resources << /Font << /F1 6 0 R >> /XObject << /Fm0 5 0 R >> >> >>"
            .to_string(),
        "<< /Length 7 >>\nstream\n/Fm0 Do\nendstream".to_string(),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 200 200] /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ]);
    let doc = PdfDocument::open(bytes.into()).expect("the fixture opens");
    let path = std::env::temp_dir().join(format!("fepdf-form-font-{}.pdf", std::process::id()));
    let options = SaveOptions { compress: false, obj_stm: false, ..SaveOptions::default() };
    doc.save_with_options(&path, "2.0", &options).expect("it writes");
    let file = std::fs::read(&path).expect("it is there");
    let _ = std::fs::remove_file(&path);
    let text = String::from_utf8_lossy(&file);
    assert!(!text.contains("Fallback-Sans"), "a stand-in font name was written");
    assert!(text.contains("/F1 20 Tf"), "the form's font name was not written");
}
