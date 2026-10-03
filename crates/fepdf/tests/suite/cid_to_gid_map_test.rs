//! Which CIDFont loading gives a `/CIDToGIDMap`, and what it says when it does (Y-F15).

use fepdf::PdfDocument;
use fepdf_model::Object;
use fepdf_model::access::{entry, items, name_in};

/// A page drawing with a Type 0 font whose one descendant is `descendant`, and the
/// objects in `extra` from 6.
fn with_descendant(descendant: &str, extra: &[&str]) -> PdfDocument {
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F 4 0 R >> >> >>".to_string(),
        "<< /Type /Font /Subtype /Type0 /BaseFont /F /Encoding /Identity-H /DescendantFonts [5 0 R] >>".to_string(),
        descendant.to_string(),
    ];
    bodies.extend(extra.iter().map(|b| (*b).to_string()));
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

/// The descendant's `/CIDToGIDMap` once loaded, as a name.
fn map_of(doc: &PdfDocument) -> Option<String> {
    let arena = doc.inner().arena();
    let page = Object::Reference(doc.inner().get_page_handle(0).expect("a page"));
    let resources = entry(arena, &page, "Resources").expect("resources");
    let font = entry(arena, &entry(arena, &resources, "Font").expect("fonts"), "F").expect("F");
    let descendant = items(arena, &font, "DescendantFonts").into_iter().next().expect("one");
    entry(arena, &descendant, "CIDToGIDMap")
        .map(|_| name_in(arena, &descendant, "CIDToGIDMap").unwrap_or_default())
}

const CIDS: &str = "/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >>";

/// **An embedded Type 2 CIDFont without one is given `/Identity`, and that is said.**
/// Table 115 requires the entry of exactly this font and gives it no default.
#[test]
fn an_embedded_type_2_cidfont_is_given_identity_and_it_is_recorded() {
    let doc = with_descendant(
        &format!(
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /F {CIDS} /FontDescriptor 6 0 R >>"
        ),
        &[
            "<< /Type /FontDescriptor /FontName /F /Flags 4 /FontFile2 7 0 R >>",
            "<< /Length 4 >>\nstream\n\0\x01\0\0\nendstream",
        ],
    );
    assert_eq!(map_of(&doc).as_deref(), Some("Identity"));
    assert!(
        doc.decisions().iter().any(|d| d.clause == "9.7.4.1" && d.found.contains("CIDToGIDMap")),
        "the map was filled in silence: {:?}",
        doc.decisions()
    );
}

/// **A Type 0 CIDFont is not given one.** Table 115 defines the entry for Type 2 alone,
/// and loading wrote it on every `CIDFontType0` of the corpus.
#[test]
fn a_type_0_cidfont_is_not_given_one() {
    let doc = with_descendant(
        &format!("<< /Type /Font /Subtype /CIDFontType0 /BaseFont /F {CIDS} >>"),
        &[],
    );
    assert_eq!(map_of(&doc), None);
}

/// **A Type 2 CIDFont with no program is left as written, and 31-004 sees it.** Loading
/// filled every one, so the audit's arm for an absent map passed without ever running.
#[test]
fn an_absent_map_on_a_type_2_cidfont_breaks_31_004() {
    let doc = with_descendant(
        &format!("<< /Type /Font /Subtype /CIDFontType2 /BaseFont /F {CIDS} >>"),
        &[],
    );
    assert_eq!(map_of(&doc), None);
    let report = doc.audit_ua2_report().expect("it audits");
    let found: Vec<_> =
        report.findings.iter().filter(|f| f.checkpoint == "31-004").map(|f| f.outcome).collect();
    assert_eq!(
        found,
        [fepdf::Outcome::Broken],
        "{:?}",
        report.findings.iter().filter(|f| f.checkpoint == "31-004").collect::<Vec<_>>()
    );
}
