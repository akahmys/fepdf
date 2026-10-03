//! Redaction removes what its regions cover, leaves the rest where it was, and fills the
//! regions (12.5.6.23, ROADMAP Y-10).
//!
//! **What it replaced wrote `[REDACTED]` over whole strings.** Every string of each
//! show-text operator touching a rectangle was replaced, covered or not, in the page's own
//! font, and nothing was drawn: a rectangle over one word of a line took the line, and
//! the window called the result 黒塗り. Before that, until 2026-09-06, two counters that
//! agreed in one place made it remove the second run of a page whatever it was given.
//!
//! **What is reported is what went**: `what_redaction_removes` runs the test the
//! redaction runs and writes nothing, so a caller reports what was removed rather than
//! what was asked for (ADR-0064).

use fepdf::{Operation, PdfDocument, Redaction};
use fepdf_fixtures::assemble;
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;

/// A page drawing `content` with Helvetica as `/F1`.
fn page_drawing(content: &str) -> PdfDocument {
    let bytes = assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
          /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ]);
    PdfDocument::open(bytes::Bytes::from(bytes)).expect("the fixture opens")
}

/// Four runs, one per line, a hundred points apart so a rectangle can name exactly one.
fn four_runs() -> PdfDocument {
    page_drawing(
        "BT /F1 24 Tf 72 700 Td (AAA) Tj ET\n\
         BT /F1 24 Tf 72 600 Td (BBB) Tj ET\n\
         BT /F1 24 Tf 72 500 Td (CCC) Tj ET\n\
         BT /F1 24 Tf 72 400 Td (DDD) Tj ET",
    )
}

fn redaction(region: (f64, f64, f64, f64)) -> Redaction {
    Redaction { page: 0, regions: vec![region] }
}

/// Redacts `region`, and answers what the page still reads and how many glyphs the
/// engine said beforehand would go.
fn redacted(mut doc: PdfDocument, region: (f64, f64, f64, f64)) -> (String, usize) {
    let said = doc.what_redaction_removes(&redaction(region)).expect("it reads").glyphs.len();
    doc.apply(Operation::Redact(redaction(region))).expect("it redacts");
    (doc.extract_text(0).expect("the page reads"), said)
}

/// Each run in turn: the one under the rectangle goes, and only it.
#[test]
fn a_rectangle_removes_the_run_under_it_and_no_other() {
    for (word, y) in [("AAA", 700.0), ("BBB", 600.0), ("CCC", 500.0), ("DDD", 400.0)] {
        let (text, said) = redacted(four_runs(), (60.0, y - 5.0, 300.0, y + 20.0));
        for other in ["AAA", "BBB", "CCC", "DDD"] {
            assert_eq!(text.contains(other), other != word, "redacting {word}: {text:?}");
        }
        assert_eq!(said, 3, "the three glyphs of {word} are what was said to go");
    }
}

/// A rectangle over the whole page removes all of it.
#[test]
fn a_rectangle_over_the_page_removes_every_run() {
    let (text, said) = redacted(four_runs(), (0.0, 0.0, 612.0, 792.0));
    assert!(text.trim().is_empty(), "{text:?}");
    assert_eq!(said, 12);
}

/// A rectangle over nothing removes nothing, and says so.
#[test]
fn a_rectangle_over_nothing_removes_nothing() {
    let (text, said) = redacted(four_runs(), (0.0, 0.0, 10.0, 10.0));
    for word in ["AAA", "BBB", "CCC", "DDD"] {
        assert!(text.contains(word), "{word} went: {text:?}");
    }
    assert_eq!(said, 0);
}

/// **A rectangle over one letter takes that letter and leaves the others where they were
/// drawn.** The route this replaced took the whole string.
#[test]
fn the_glyphs_outside_keep_their_places() {
    // Helvetica's A and B are 667 units wide, so at 24 pt B is drawn from 88.008 and C
    // from 104.016.
    let (text, said) =
        redacted(page_drawing("BT /F1 24 Tf 72 700 Td (ABCD) Tj ET"), (90.0, 690.0, 100.0, 730.0));
    assert_eq!(said, 1, "one glyph meets the region");
    assert!(text.contains('A') && text.contains("CD") && !text.contains('B'), "{text:?}");

    let mut doc = page_drawing("BT /F1 24 Tf 72 700 Td (ABCD) Tj ET");
    doc.apply(Operation::Redact(redaction((90.0, 690.0, 100.0, 730.0)))).expect("it redacts");
    let mut after = Recorder::new();
    doc.render_page(0, &mut after, Affine::IDENTITY).expect("it renders");
    // What is left is two strings, A and CD, each starting where its first glyph was.
    let origins = after.device_text_origins();
    let expected = [(72.0, 700.0), (104.016, 700.0)];
    assert_eq!(origins.len(), 2, "{origins:?}");
    for (got, want) in origins.iter().zip(expected) {
        assert!((got.0 - want.0).abs() < 1e-6 && (got.1 - want.1).abs() < 1e-6, "{origins:?}");
    }
}

/// **A glyph the region only touches goes**: one that meets it below the baseline, where
/// a descender would be, is inside (MuPDF's rule, decided 2026-10-03).
#[test]
fn a_glyph_the_region_only_touches_goes() {
    let (text, said) =
        redacted(page_drawing("BT /F1 24 Tf 72 700 Td (gap) Tj ET"), (0.0, 694.0, 612.0, 698.0));
    assert_eq!(said, 3, "the region meets every glyph's box under the baseline");
    assert!(text.trim().is_empty(), "{text:?}");
}

/// **The two quote operators show text too** (9.4.3), and what they show goes like any
/// other.
#[test]
fn text_shown_by_the_quote_operators_goes() {
    let (text, _) = redacted(
        page_drawing("BT /F1 24 Tf 30 TL 72 700 Td (AAA) Tj (BBB) ' 2 1 (CCC) \" ET"),
        (0.0, 0.0, 612.0, 792.0),
    );
    assert!(text.trim().is_empty(), "the quoted lines survived: {text:?}");
}

/// **A page with text it cannot place is refused, and nothing changes.** A string drawn in
/// a font the page's resources do not name has no widths, so whether its glyphs meet the
/// region is not known; reporting the page as redacted would leave them.
#[test]
fn a_page_with_text_it_cannot_place_is_refused() {
    let mut doc =
        page_drawing("BT /F1 24 Tf 72 700 Td (AAA) Tj ET BT /F9 24 Tf 72 600 Td (ZZZ) Tj ET");
    let refused = doc.apply(Operation::Redact(redaction((0.0, 0.0, 612.0, 792.0))));
    let why = format!("{refused:?}");
    assert!(refused.is_err(), "a page it cannot place was redacted");
    assert!(why.contains("nothing was redacted"), "{why}");
    assert!(doc.extract_text(0).expect("it reads").contains("AAA"), "the refusal changed the page");
}

/// **The region is filled black, and that is recorded as this engine's choice**: with no
/// `/Redact` annotation, nothing in the file says what the fill is (12.5.6.23).
#[test]
fn the_region_is_filled_and_the_choice_recorded() {
    let mut doc = four_runs();
    doc.apply(Operation::Redact(redaction((60.0, 595.0, 300.0, 620.0)))).expect("it redacts");
    let mut marks = Recorder::new();
    doc.render_page(0, &mut marks, Affine::IDENTITY).expect("it renders");
    let fills = marks.device_fills();
    assert!(
        fills.iter().any(|f| (f.x0 - 60.0).abs() < 1e-6 && (f.y1 - 620.0).abs() < 1e-6),
        "no fill over the region: {fills:?}"
    );
    assert!(doc.decisions().iter().any(|d| d.clause == "12.5.6.23"), "{:?}", doc.decisions());
}

/// A redaction that names no region, or a region with no area, is refused.
#[test]
fn a_redaction_of_nothing_is_refused() {
    let mut doc = four_runs();
    assert!(doc.apply(Operation::Redact(Redaction { page: 0, regions: vec![] })).is_err());
    assert!(doc.apply(Operation::Redact(redaction((10.0, 10.0, 10.0, 50.0)))).is_err());
}
