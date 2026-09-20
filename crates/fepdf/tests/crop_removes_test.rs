//! What a crop puts outside the sheet is removed, not hidden.
//!
//! **`/CropBox` makes a region a viewer displays (14.11.2) and leaves the rest in the
//! file.** Measured on `print_sample.pdf` page 3 shifted 300 points right: the render
//! loses the right-hand half, 78 of the page's 112 runs end past the sheet's edge, and
//! `extract_text` returns all 428 characters it did before. Half a drawing, still
//! searchable, on a page showing the other half — a reader who cuts an A3 assembly
//! drawing into two A4 sheets to send one of them has sent both
//! ([ADR-0088](../../../docs/adr/0088-what-a-crop-puts-outside-the-sheet-is-removed.md)).
//!
//! The test the roadmap named for this failed against the behaviour of the day it was
//! written, which is why it was worth writing.

use fepdf::text::runs_of_page;
use fepdf::{ContentScale, IngestionOptions, Operation, PageResize, PageSelection, PdfDocument};
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;

fn opened(name: &str) -> PdfDocument {
    let bytes = std::fs::read(format!("../../samples/{name}")).expect("the sample is there");
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens")
}

/// The page drawn on its own sheet, shifted so that half of it hangs over the right edge.
fn hanging_over() -> (PdfDocument, (f64, f64, f64, f64)) {
    let mut doc = opened("print_sample.pdf");
    doc.apply(Operation::ResizePages(
        PageSelection::Indices(vec![2]),
        PageResize { sheet: None, scale: ContentScale::Keep, offset: (300.0, 0.0) },
    ))
    .expect("the resize applies");
    let sheet = doc.get_page_box(2).expect("the page has a box");
    (doc, (sheet.x1, sheet.y1, sheet.x2, sheet.y2))
}

/// **The characters that hang over the edge are gone from the file.**
///
/// This is the check ADR-0088 asked for: `extract_text` on the cropped side returning a
/// character that was cut away is the failure.
#[test]
fn a_crop_takes_the_text_it_hides_out_of_the_file() {
    let (mut doc, sheet) = hanging_over();
    let before = doc.extract_text(2).expect("it extracts").chars().count();
    assert_eq!(before, 428, "the sample no longer holds the page this was measured on");

    doc.apply(Operation::RemoveOutside { page: 2, keep: sheet }).expect("the crop applies");
    let after = doc.extract_text(2).expect("it extracts").chars().count();

    assert!(
        after < before,
        "the crop took nothing out: {before} characters before and {after} after"
    );
    let over = runs_of_page(doc.inner(), 2)
        .expect("it lists")
        .into_iter()
        .filter(|run| run.origin.0 > sheet.2)
        .count();
    assert_eq!(over, 0, "{over} runs still start past the right edge of the sheet");
}

/// **What stays stays where it was.**
///
/// A run is not deleted, because deleting one takes its advance with it and everything
/// after it on the line closes up. The offsets that stand for the removed glyphs are what
/// hold the rest in place, and this is what says they do.
///
/// It asks whether every run still drawn is drawn where it was, not whether the same
/// number of them are: the renderer draws a run in as many pieces as its `TJ` has strings,
/// and a crop rewrites a run into one string, so the count falls without a glyph being
/// lost. Positions are the thing that must not move.
#[test]
fn a_crop_leaves_what_it_keeps_where_it_was() {
    let drawn_at = |doc: &PdfDocument| {
        let mut recorder = Recorder::new();
        doc.render_page(2, &mut recorder, Affine::IDENTITY).expect("the page interprets");
        recorder.device_text_origins()
    };

    let (doc, sheet) = hanging_over();
    let before = drawn_at(&doc);
    let (mut cropped, _) = hanging_over();
    cropped.apply(Operation::RemoveOutside { page: 2, keep: sheet }).expect("the crop applies");
    let after = drawn_at(&cropped);

    assert!(after.len() > 30, "only {} runs are left, which is not a crop", after.len());
    for at in &after {
        assert!(
            before.iter().any(|was| (was.0 - at.0).abs() < 0.1 && (was.1 - at.1).abs() < 0.1),
            "the crop drew something at {at:?}, where the page drew nothing before"
        );
        assert!(
            at.0 < sheet.2 + 0.1,
            "the crop left a run at {at:?}, past the sheet's right edge at {}",
            sheet.2
        );
    }
}

/// A crop that keeps the whole page changes nothing.
#[test]
fn a_crop_that_hides_nothing_removes_nothing() {
    let doc = opened("print_sample.pdf");
    let before = doc.extract_text(2).expect("it extracts");

    let mut doc = opened("print_sample.pdf");
    let sheet = doc.get_page_box(2).expect("the page has a box");
    doc.apply(Operation::RemoveOutside { page: 2, keep: (sheet.x1, sheet.y1, sheet.x2, sheet.y2) })
        .expect("the crop applies");

    assert_eq!(doc.extract_text(2).expect("it extracts"), before, "the page lost something");
}

/// A page drawing one run of eight letters from (100, 700) at 20 points.
///
/// Helvetica's capitals are 667 to 722 thousandths of an em, so each letter is about
/// 13 to 15 points wide and the run is about 110 long. A crop can therefore be given a
/// rectangle that falls inside it, with letters on both sides of what is kept.
fn one_long_run() -> PdfDocument {
    let content = "BT /F1 20 Tf 1 0 0 1 100 700 Tm (ABCDEFGH) Tj ET";
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

/// Where the page draws each of its runs, and what they read.
fn drawn(doc: &PdfDocument) -> (Vec<(f64, f64)>, String) {
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    (recorder.device_text_origins(), recorder.text())
}

/// **What is left of a cut run is drawn where it was, not at the run's old start.**
///
/// The glyphs taken off the front of a run are what the offset in its `TJ` stands for.
/// Without it the rest slides left to where the first letter used to be — on the page,
/// under a crop that was supposed to move nothing.
#[test]
fn glyphs_after_a_removed_one_stay_where_they_were() {
    let doc = one_long_run();
    let (before, reads) = drawn(&doc);
    assert_eq!(reads, "ABCDEFGH", "the fixture does not draw what this is written about");
    let start = before[0];

    // Keep from 40 points into the run to 80, which falls inside it on both sides.
    let mut cropped = one_long_run();
    cropped
        .apply(Operation::RemoveOutside {
            page: 0,
            keep: (start.0 + 40.0, 0.0, start.0 + 80.0, 792.0),
        })
        .expect("the crop applies");
    let (after, kept) = drawn(&cropped);

    assert!(kept.len() < 8, "the crop kept every letter: {kept:?}");
    assert!(!kept.is_empty(), "the crop kept no letter at all");
    assert!(
        after[0].0 > start.0 + 20.0,
        "what is left is drawn at {:?}, near where the run began at {start:?} — the \
         offset standing for the letters taken off the front is missing or backwards",
        after[0]
    );
    assert!(
        (after[0].1 - start.1).abs() < 0.01,
        "the crop moved the line: {} then {}",
        start.1,
        after[0].1
    );
}

/// **A letter the boundary falls across is kept.**
///
/// It is visible on the side that stays, so dropping it would take ink a reader can see.
/// What goes is what was entirely on the other side.
#[test]
fn a_glyph_the_boundary_crosses_is_kept() {
    let doc = one_long_run();
    let (before, _) = drawn(&doc);
    let start = before[0];

    // Half-way through the first letter, so the boundary crosses it.
    let mut cropped = one_long_run();
    cropped
        .apply(Operation::RemoveOutside { page: 0, keep: (start.0 + 7.0, 0.0, 612.0, 792.0) })
        .expect("the crop applies");
    let (_, kept) = drawn(&cropped);

    assert!(kept.starts_with('A'), "the letter the boundary crosses was dropped: {kept:?}");
}
