//! Text in a font on `/MacRomanEncoding` reads as the encoding says (D.2).
//!
//! **The engine carried no MacRomanEncoding**, for want of the document to take it from,
//! so a font naming it was left with no encoding and every code above ASCII read as
//! nothing — with a decision saying so, which is how the gap was known.

use fepdf::{IngestionOptions, PdfDocument};

/// A page showing `shown` in Helvetica on `encoding`.
fn showing(encoding: &str, shown: &str) -> PdfDocument {
    let content = format!("BT /F1 12 Tf 20 100 Td {shown} Tj ET");
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        format!("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding {encoding} >>"),
    ];
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// **0x80, 0x8E and 0xD2 are Ä, é and “ on a Mac**, where WinAnsi has €, Ž and Ò.
#[test]
fn mac_roman_codes_read_as_mac_roman() {
    let text = showing("/MacRomanEncoding", "<808ED2>").extract_text(0).expect("text");
    assert!(text.contains("Äé\u{201C}"), "{text:?}");
    let text = showing("/WinAnsiEncoding", "<808ED2>").extract_text(0).expect("text");
    assert!(text.contains("€ŽÒ"), "{text:?}");
}

/// **As a `/BaseEncoding` too**, under a `/Differences` that renames one code.
#[test]
fn mac_roman_is_a_base_encoding_as_well() {
    let encoding = "<< /BaseEncoding /MacRomanEncoding /Differences [128 /eacute] >>";
    let text = showing(encoding, "<808A>").extract_text(0).expect("text");
    assert!(text.contains("éä"), "{text:?}");
}
