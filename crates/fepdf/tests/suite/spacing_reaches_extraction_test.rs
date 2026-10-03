//! `Tw`, `Tc` and `Tz` move a run, and the extractor could not see any of them.
//!
//! **One rule written three times, and two of the copies were wrong.** `fepdf-render`
//! applied 9.4.4 — `tx = ((w0 − Tj/1000) × Tfs + Tc + Tw) × Th` — while
//! `TextExtractionBackend` and `CollectorBackend` summed the glyph widths and stopped. A
//! caller reading span positions out of a file that sets spacing was given coordinates
//! wrong by one space per space, and the renderer and the extractor disagreed about the
//! same page (ROADMAP W-E3e).
//!
//! The rule is `fepdf_content::advance_of` now, beside the glyph and the text state it is
//! about, and all three ask it.

use fepdf::{IngestionOptions, PdfDocument};

/// A page drawing `first`, then `setting`, then `second`.
fn page_with(setting: &str, first: &str, second: &str) -> PdfDocument {
    let content = format!("BT /F1 24 Tf 1 0 0 1 40 700 Tm {setting} ({first}) Tj ({second}) Tj ET");
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

/// How wide the span reading `text` is, as `extract_spans` reports it.
///
/// **The width and not the position, because the position was never wrong.** The entry
/// this repairs said the extractor gave the same number with `20 Tw` and without; measured
/// 2026-09-24 it gives the same *extent* — the interpreter advances the text matrix with
/// the spacing applied, so `x` was right all along and the backend's own sum of glyph
/// widths was what ignored it. Two versions of these tests measured `x` and **all four
/// mutations of the rule survived both**, the second time because `runs_of_page` has a
/// correct 9.4.4 of its own and the third because `x` comes from the transform.
fn span_width(doc: &PdfDocument, text: &str) -> f64 {
    let spans = doc.extract_spans(0).expect("it extracts spans");
    let Some(span) = spans.iter().find(|s| s.text == text) else {
        panic!(
            "no span reads {text:?}; the page gave {}",
            spans.iter().map(|s| format!("{:?}", s.text)).collect::<Vec<_>>().join(" ")
        )
    };
    span.width
}

/// **A word space widens the span that holds it.**
///
/// One space at `20 Tw` is twenty points of extent the span did not report.
#[test]
fn word_spacing_widens_the_span_that_holds_the_space() {
    let without = span_width(&page_with("", "a b", "c"), "a b");
    let with = span_width(&page_with("20 Tw", "a b", "c"), "a b");

    assert!(
        (with - without - 20.0).abs() < 0.01,
        "one space at 20 Tw should widen the span by 20 points: {without} then {with}"
    );
}

/// **And `Tw` counts spaces, not glyphs.**
///
/// A run with no space does not widen, which tells a `Tw` applied per space from one
/// applied per glyph. 9.4.4 gives it to the single-byte code 32 and to nothing else.
#[test]
fn word_spacing_does_nothing_to_a_span_with_no_space() {
    let without = span_width(&page_with("", "abc", "d"), "abc");
    let with = span_width(&page_with("20 Tw", "abc", "d"), "abc");

    assert!(
        (with - without).abs() < 0.01,
        "a run with no space widened under 20 Tw: {without} then {with}"
    );
}

/// **Character spacing applies to every glyph, including the last.**
///
/// Three glyphs at `5 Tc` is fifteen points, which tells `Tc` from an off-by-one that
/// charges only the gaps between them.
#[test]
fn character_spacing_applies_to_every_glyph() {
    let without = span_width(&page_with("", "abc", "d"), "abc");
    let with = span_width(&page_with("5 Tc", "abc", "d"), "abc");

    assert!(
        (with - without - 15.0).abs() < 0.01,
        "three glyphs at 5 Tc should widen the span by 15 points: {without} then {with}"
    );
}

/// **And horizontal scaling multiplies the lot.**
///
/// `Tz` is a percentage in the content stream and a ratio in the text state, which is the
/// kind of place a factor of a hundred hides.
#[test]
fn horizontal_scaling_multiplies_the_advance() {
    let plain = span_width(&page_with("", "abc", "d"), "abc");
    let scaled = span_width(&page_with("50 Tz", "abc", "d"), "abc");

    assert!(
        plain.mul_add(-0.5, scaled).abs() < 0.01,
        "50 Tz should halve the span's width: {plain} became {scaled}"
    );
}

/// **The `\"` operator sets both spacings and shows in one go, and is measured too.**
///
/// It is the operator the entry took its figure from, and it reaches the text state by a
/// different path than `Tw` and `Tc` do.
#[test]
fn the_quote_operator_sets_the_spacing_it_shows_with() {
    let plain = span_width(&page_with("", "a b", "c"), "a b");
    let spaced = span_width(&page_with("", "a b", "c"), "a b");
    assert!((plain - spaced).abs() < f64::EPSILON, "the fixture is not deterministic");

    let doc = page_quoted("20 0");
    let width = doc
        .extract_spans(0)
        .expect("it extracts")
        .iter()
        .find(|s| s.text == "a b")
        .map(|s| s.width)
        .expect("the quoted run is there");
    assert!(
        (width - plain - 20.0).abs() < 0.01,
        "`20 0 (a b) \"` should widen the span by 20 points: {plain} then {width}"
    );
}

/// A page drawing `a b` through the `"` operator with `aw ac` in front of it.
fn page_quoted(spacing: &str) -> PdfDocument {
    let content = format!("BT /F1 24 Tf 1 0 0 1 40 700 Tm {spacing} (a b) \" (c) Tj ET");
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
