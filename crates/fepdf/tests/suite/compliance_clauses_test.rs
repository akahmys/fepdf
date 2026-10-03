//! The clauses an audit reports having met are the clauses the document carries.
//!
//! **`fepdf inspect audit` reported none of the font clauses until 2026-09-06.**
//! `samples/constitution.pdf` read `Validated Clauses: 14.3.3, 7.7.2, 7.7.3.3, 9.2, 9.8`
//! for a document whose every font is a TrueType: 9.6.3 was absent from a list of what
//! the document met.
//!
//! `font/schema.rs` declares `PdfType1Font`, `PdfTrueTypeFont` and `PdfType0Font`,
//! carrying clauses 9.6.2, 9.6.3 and 9.7. All three were **referenced by nothing** —
//! found in a sweep for public items no code names — while `audit_specific_types` beside
//! them dispatched on `/Subtype` for OpenType, CIDFontType0 and CIDFontType2. The three
//! most common font types were the three it did not ask about.
//!
//! Deleting them was the other option. They are the shape ADR-0017 warns of — a
//! declaration that reads nothing is not modelling — but here the reader was one match
//! arm away, and a clause the engine parses and does not report is a coverage figure
//! that understates itself.

use fepdf::PdfDocument;
use fepdf_fixtures::assemble;

fn clauses_for(font: &str) -> Vec<String> {
    let doc = PdfDocument::open(bytes::Bytes::from(assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
          /Resources << /Font << /F1 4 0 R >> >> >>"
            .to_string(),
        font.to_string(),
    ])))
    .expect("the fixture opens");
    doc.get_summary().expect("the audit runs").compliance.iso_clauses
}

/// A Type 1 font makes 9.6.2 a clause the document presented.
#[test]
fn a_type1_font_reports_its_clause() {
    let clauses = clauses_for(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );
    assert!(clauses.contains(&"9.6.2".to_string()), "{clauses:?}");
}

/// So does a TrueType font, under 9.6.3.
#[test]
fn a_truetype_font_reports_its_clause() {
    let clauses = clauses_for(
        "<< /Type /Font /Subtype /TrueType /BaseFont /Arial /Encoding /WinAnsiEncoding >>",
    );
    assert!(clauses.contains(&"9.6.3".to_string()), "{clauses:?}");
}

/// And a Type 0 font under 9.7 — the one every CJK document in the corpus carries.
#[test]
fn a_type0_font_reports_its_clause() {
    let clauses = clauses_for(
        "<< /Type /Font /Subtype /Type0 /BaseFont /X /Encoding /Identity-H \
          /DescendantFonts [] >>",
    );
    assert!(clauses.contains(&"9.7".to_string()), "{clauses:?}");
}

/// A document with no font reports none of them.
///
/// Without this, an audit that inserted all three unconditionally would pass the three
/// above and make the clause list a constant.
#[test]
fn a_document_with_no_font_reports_no_font_clause() {
    let doc = PdfDocument::open(bytes::Bytes::from(assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
    ])))
    .expect("the fixture opens");
    let clauses = doc.get_summary().expect("the audit runs").compliance.iso_clauses;
    for font_clause in ["9.6.2", "9.6.3", "9.7"] {
        assert!(!clauses.contains(&font_clause.to_string()), "{font_clause} in {clauses:?}");
    }
}

/// A Type 3 font reports 9.6.5 — the clause for the one font kind that has no program.
///
/// **`samples/fugaku.pdf` reported no font clause at all.** Twenty-five pages of
/// Japanese, seventy-two fonts, every one of them Type 3, and
/// `Validated Clauses: 14.3.3, 7.7.2, 7.7.3.3`. Not even 9.2: Arlington confirms
/// `FontType3` has no `/BaseFont`, and `PdfFont`'s gate requires one, so the general
/// font type correctly does not match and there was nothing else to match instead.
///
/// The required entries are Arlington's for `FontType3`: `/FontBBox`, `/FontMatrix`,
/// `/CharProcs`, `/Encoding`, `/FirstChar`, `/LastChar` and `/Widths`, all `TRUE`.
/// `/Encoding` is a dictionary here and not a name, which is the other way a Type 3
/// font differs — its glyphs are named by `/CharProcs`, so an encoding is the only
/// route from a code to a glyph.
#[test]
fn a_type3_font_reports_its_clause() {
    let clauses = clauses_for(
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 750 750] \
          /FontMatrix [0.001 0 0 0.001 0 0] /CharProcs << /square 5 0 R >> \
          /Encoding << /Type /Encoding /Differences [97 /square] >> \
          /FirstChar 97 /LastChar 97 /Widths [750] >>",
    );
    assert!(clauses.contains(&"9.6.5".to_string()), "{clauses:?}");
}

/// A Type 3 font missing an entry 9.6.5 requires does not report the clause.
///
/// The other half of every clause test here: the list says what the document met, not
/// what it declared. A dictionary calling itself `/Type3` with no `/CharProcs` has not
/// met 9.6.5, and saying it did would make the audit's coverage figure a count of
/// `/Subtype` values.
#[test]
fn an_incomplete_type3_font_reports_nothing() {
    let clauses = clauses_for("<< /Type /Font /Subtype /Type3 /FirstChar 97 /LastChar 97 >>");
    assert!(!clauses.contains(&"9.6.5".to_string()), "{clauses:?}");
}

/// A font descriptor missing an entry ISO requires does not report 9.8.
///
/// `PdfFontDescriptor` declares eight metrics — `/FontBBox`, `/ItalicAngle`, `/Ascent`,
/// `/Descent`, `/CapHeight`, `/StemV` and the two 1.5 additions — and **no code reads any
/// of them.** A sweep for that shape usually ends in deletion (ADR-0017), and here it
/// must not: the parse is what decides whether the audit reports 9.8, so the required
/// fields *are* the conformance check. This test is what says so, because without it a
/// later reader sees eight unread fields and takes the usual course.
#[test]
fn an_incomplete_font_descriptor_does_not_report_its_clause() {
    // The descriptor exists and is well-formed apart from what it omits: /Ascent and
    // /Descent, Required = TRUE in all four of Arlington's non-Type3 descriptors. An
    // earlier version of this test pointed the font at an object that was not in the
    // file at all, so it passed because the reference dangled and proved nothing.
    let clauses = descriptor_clauses(
        "<< /Type /FontDescriptor /FontName /Arial /Flags 32 /FontBBox [0 -200 1000 900] \
          /ItalicAngle 0 /CapHeight 700 /StemV 80 >>",
    );
    assert!(!clauses.contains(&"9.8".to_string()), "{clauses:?}");
}

/// The clauses reported for a TrueType font pointing at `descriptor`.
fn descriptor_clauses(descriptor: &str) -> Vec<String> {
    let doc = PdfDocument::open(bytes::Bytes::from(assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
          /Resources << /Font << /F1 4 0 R >> >> >>"
            .to_string(),
        "<< /Type /Font /Subtype /TrueType /BaseFont /Arial /FontDescriptor 5 0 R >>".to_string(),
        descriptor.to_string(),
    ])))
    .expect("the fixture opens");
    doc.get_summary().expect("the audit runs").compliance.iso_clauses
}

/// A complete one does.
#[test]
fn a_complete_font_descriptor_reports_its_clause() {
    let clauses = descriptor_clauses(
        "<< /Type /FontDescriptor /FontName /Arial /Flags 32 /FontBBox [0 -200 1000 900] \
          /ItalicAngle 0 /Ascent 900 /Descent -200 /CapHeight 700 /StemV 80 >>",
    );
    assert!(clauses.contains(&"9.8".to_string()), "{clauses:?}");
}
