//! The paths a crop puts outside the sheet, cut rather than hidden (ROADMAP W-G1-c,
//! ADR-0088).
//!
//! What is asked is what the page draws after a crop to its left half, from the renderer:
//! how far each line and fill reaches.

use fepdf::{
    CropRegion, IngestionOptions, Operation, PageSelection, PdfDocument, WhatFallsOutside,
};
use fepdf_fixtures::recorder::{Event, Recorder};
use kurbo::{Affine, Rect, Shape};

/// A 200-point square page drawing, in order: a line across the middle, a rectangle
/// straddling it, a rectangle wholly on the right, a curve across the top, and a fill
/// under a clip that lies wholly on the right — twice, the second time a clip that is
/// also filled (`W f`), which is the path a clip is most easily taken for a fill.
fn drawing() -> PdfDocument {
    let content = "2 w 20 50 m 180 50 l S \
                   50 100 100 20 re f \
                   150 150 40 30 re f \
                   20 170 m 60 190 100 150 180 180 c S \
                   q 150 0 40 40 re W n 0 0 200 200 re f Q \
                   q 150 60 40 20 re W f 0 0 200 200 re f Q";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// What the page paints: each stroke's and each fill's extent on the page, and how many
/// clips it sets.
fn painted(doc: &PdfDocument) -> (Vec<Rect>, Vec<Rect>, usize) {
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    let (mut strokes, mut fills, mut clips) = (Vec::new(), Vec::new(), 0);
    for event in &recorder.events {
        if let Event::Stroke { path, ctm, .. } = event {
            strokes.push((*ctm * path.clone()).bounding_box());
        } else if let Event::Fill { path, ctm, .. } = event {
            fills.push((*ctm * path.clone()).bounding_box());
        } else if matches!(event, Event::PushClip { .. }) {
            clips += 1;
        }
    }
    (strokes, fills, clips)
}

fn cropped_to_the_left_half() -> PdfDocument {
    let mut doc = drawing();
    doc.apply(Operation::CropPages(
        PageSelection::Single(0),
        CropRegion { keep: (0.0, 0.0, 100.0, 200.0), outside: WhatFallsOutside::Goes },
    ))
    .expect("the crop applies");
    doc
}

/// **A line across the edge stops at the edge**, and keeps the half of the pen that
/// shows there.
#[test]
fn a_line_across_the_edge_is_cut_there() {
    let (strokes, _, _) = painted(&cropped_to_the_left_half());
    let line = strokes
        .iter()
        .find(|r| (r.y0 - 50.0).abs() < 0.01 && (r.y1 - 50.0).abs() < 0.01)
        .expect("the line is still drawn");
    assert!((line.x0 - 20.0).abs() < 0.01, "the kept end moved: {line:?}");
    assert!((line.x1 - 101.0).abs() < 0.01, "the line runs to {} and the edge is at 100", line.x1);
}

/// **A rectangle across the edge is filled to the edge.**
#[test]
fn a_fill_across_the_edge_is_cut_there() {
    let (_, fills, _) = painted(&cropped_to_the_left_half());
    let band = fills
        .iter()
        .find(|r| (r.y0 - 100.0).abs() < 0.01 && (r.y1 - 120.0).abs() < 0.01)
        .expect("the band is still filled");
    assert!((band.x0 - 50.0).abs() < 0.01 && (band.x1 - 100.0).abs() < 0.01, "{band:?}");
}

/// **A fill wholly outside is taken out**, not left under the crop.
#[test]
fn a_fill_wholly_outside_is_taken_out() {
    let (_, fills, _) = painted(&cropped_to_the_left_half());
    assert!(
        !fills.iter().any(|r| r.x0 >= 149.0 && r.y0 >= 149.0),
        "the rectangle on the right is still in the page: {fills:?}"
    );
}

/// **A curve is left whole** — cut at the edge it would be a different curve — and so is
/// a clip, which decides what everything after it shows.
#[test]
fn a_curve_and_a_clip_are_left_whole() {
    let (strokes, _, clips) = painted(&cropped_to_the_left_half());
    assert!(strokes.iter().any(|r| r.x1 > 150.0 && r.y0 > 140.0), "the curve was cut: {strokes:?}");
    assert_eq!(clips, 2, "a clip was taken out, uncovering what it hid");
}
