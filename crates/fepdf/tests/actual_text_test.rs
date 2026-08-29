//! `/ActualText` reaching the interpreter (14.9.4), in the shapes the corpus does not use.
//!
//! All 6,080 spans in the corpus are a `/Span` tag with the property list written in
//! place, so the two other shapes 14.6.2 permits — a named list, and any other tag — were
//! implemented and then exercised by nothing. That is what this file is for. The
//! extraction rules themselves are tested against the backend in `fepdf-doc`.

use fepdf::{IngestionOptions, PdfDocument};

/// A stream object with `extra` merged into its dictionary.
fn stream(extra: &str, data: &str) -> String {
    format!("<< {extra} /Length {} >>\nstream\n{data}endstream", data.len())
}

fn assemble(bodies: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let table_at = out.len();
    let size = bodies.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n")
            .as_bytes(),
    );
    out
}

/// The text a one-page document extracts, with `resources` on the page.
fn extracted(resources: &str, content: &str) -> String {
    let bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
             /Resources << {resources} >> /Contents 4 0 R >>"
        ),
        stream("", content),
    ];
    let doc =
        PdfDocument::open_with_options(assemble(&bodies).into(), &IngestionOptions::default())
            .expect("the fixture opens");
    doc.extract_text(0).expect("the page interprets")
}

#[test]
fn a_property_list_named_in_the_resources_is_looked_up() {
    // 14.6.2 allows the operand to be a name into `/Properties`. Nothing in the corpus
    // does it, so this fixture is the only thing that ever has.
    let text =
        extracted("/Properties << /P1 << /ActualText (recovered) >> >>", "/Span /P1 BDC\nEMC\n");
    assert_eq!(text, "recovered");
}

#[test]
fn a_tag_that_is_not_span_carries_its_text_too() {
    // 14.9.4 attaches `/ActualText` to a marked-content sequence, not to one tag's name.
    let text = extracted("", "/P << /ActualText (paragraph) >> BDC\nEMC\n");
    assert_eq!(text, "paragraph");
}

#[test]
fn a_named_list_without_the_key_contributes_nothing() {
    let text = extracted("/Properties << /P1 << /Type /OCG >> >>", "/Span /P1 BDC\nEMC\n");
    assert_eq!(text, "", "a property list that says nothing says nothing");
}

#[test]
fn a_name_that_resolves_to_no_resource_is_not_an_error() {
    // The file is wrong; refusing the operator would abort the content stream and take
    // the rest of the page's text with it (ADR-0018).
    let text = extracted("", "/Span /Missing BDC\nEMC\n");
    assert_eq!(text, "");
}

#[test]
fn hex_and_literal_strings_both_decode() {
    // 376 of page 389's 393 spans in `volvo_xc90.pdf` are hex with a byte order mark.
    assert_eq!(extracted("", "/Span << /ActualText <FEFF00680069> >> BDC\nEMC\n"), "hi");
    assert_eq!(extracted("", "/Span << /ActualText (hi) >> BDC\nEMC\n"), "hi");
}
