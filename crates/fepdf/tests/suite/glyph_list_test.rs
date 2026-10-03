//! A glyph name is read through Adobe's Glyph List, all of it (9.10.2).
//!
//! **The list was 61 names typed out by hand**, so a `/Differences` array naming any of
//! the other 4,220 read as nothing, and two of the 61 were the wrong character.

use fepdf::{IngestionOptions, PdfDocument};

/// A page showing codes 65 and 66 in Helvetica, which `/Differences` renames.
fn renamed(first: &str, second: &str) -> PdfDocument {
    let content = "BT /F1 12 Tf 20 100 Td (AB) Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica \
             /Encoding << /Differences [65 /{first} /{second}] >> >>"
        ),
    ];
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// **Names the hand table never had** are read as the list says.
#[test]
fn a_name_only_the_full_list_has_is_read() {
    let text = renamed("eacute", "Aogonek").extract_text(0).expect("text");
    assert!(text.contains("éĄ"), "{text:?}");
}

/// **And the quotes are the list's**, not the ASCII look-alikes the hand table had.
#[test]
fn a_quote_is_the_lists_quote() {
    let text = renamed("quoteleft", "quoteright").extract_text(0).expect("text");
    assert!(text.contains("\u{2018}\u{2019}"), "{text:?}");
}
