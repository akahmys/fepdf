//! **A path a redaction region lies over is cut to what lies outside it** (ROADMAP Y-10,
//! decided by the owner 2026-10-03).
//!
//! What is asked here is what the page draws after the redaction, from the renderer:
//! whether any of a fill or a stroke is inside the region, and whether what is outside
//! is drawn as it was.

use fepdf::{Operation, PdfDocument, Redaction};
use fepdf_fixtures::recorder::{Event, Recorder};
use kurbo::{Affine, BezPath, ParamCurve, Point, Shape};

/// A 200-point page drawing `content`.
fn page_drawing(content: &str) -> PdfDocument {
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> /Contents 4 0 R >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ]);
    PdfDocument::open(bytes.into()).expect("the fixture opens")
}

fn redact(doc: &mut PdfDocument, region: (f64, f64, f64, f64)) {
    let redaction = Redaction { page: 0, regions: vec![region], fill: Some(vec![]) };
    doc.apply(Operation::Redact(redaction)).expect("it redacts");
}

/// Every filled path and every stroked one the page draws, on the page.
fn marks(doc: &PdfDocument) -> (Vec<BezPath>, Vec<(BezPath, f64)>) {
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("it renders");
    let (mut fills, mut strokes) = (Vec::new(), Vec::new());
    for event in recorder.events {
        if let Event::Fill { path, ctm, .. } = event {
            fills.push(ctm * path);
        } else if let Event::Stroke { path, ctm, style, .. } = event {
            strokes.push((ctm * path, style.width));
        }
    }
    (fills, strokes)
}

/// Whether any fill covers `point`.
fn filled(fills: &[BezPath], point: (f64, f64)) -> bool {
    fills.iter().any(|f| f.winding(Point::new(point.0, point.1)) != 0)
}

/// **A full-page background loses the region and keeps the rest**: MuPDF's rule, which
/// removes what the region touches, would have taken all of it.
#[test]
fn a_background_keeps_what_lies_outside() {
    let mut doc = page_drawing("0.9 g 0 0 200 200 re f");
    redact(&mut doc, (50.0, 50.0, 100.0, 100.0));
    let (fills, _) = marks(&doc);
    assert!(!filled(&fills, (75.0, 75.0)), "the region is still filled");
    for outside in
        [(10.0, 10.0), (150.0, 150.0), (75.0, 150.0), (75.0, 10.0), (10.0, 75.0), (150.0, 75.0)]
    {
        assert!(filled(&fills, outside), "{outside:?} lost its fill");
    }
}

/// **A curve is cut where it crosses, and keeps its shape outside**: a circle drawn in four
/// Béziers loses its left half and keeps the right one to the edge.
#[test]
fn a_curve_keeps_its_shape_outside() {
    // A circle of radius 50 round (100, 100), as four cubics (k = 0.5523).
    let k = 50.0 * 0.552_284_75;
    let circle = format!(
        "150 100 m 150 {a} {b} 150 100 150 c {c} 150 50 {a} 50 100 c 50 {d} {c} 50 100 50 c \
         {b} 50 150 {d} 150 100 c f",
        a = 100.0 + k,
        b = 100.0 + k,
        c = 100.0 - k,
        d = 100.0 - k,
    );
    let mut doc = page_drawing(&circle);
    redact(&mut doc, (0.0, 0.0, 100.0, 200.0));
    let (fills, _) = marks(&doc);
    assert!(!filled(&fills, (60.0, 100.0)), "the left half is still drawn");
    assert!(filled(&fills, (149.0, 100.0)), "the right edge of the circle went");
    assert!(filled(&fills, (135.0, 135.0)), "inside the arc, near it, went");
    assert!(!filled(&fills, (137.0, 137.0)), "outside the arc is drawn: the curve was flattened");
}

/// **A stroke loses what the pen would draw inside, and no more**: a line across the region
/// keeps both ends, each stopping half a pen short of it.
#[test]
fn a_stroke_loses_what_its_pen_draws_inside() {
    let mut doc = page_drawing("4 w 0 100 m 200 100 l S");
    redact(&mut doc, (90.0, 0.0, 110.0, 200.0));
    let (_, strokes) = marks(&doc);
    let reaches: Vec<(f64, f64)> =
        strokes.iter().map(|(path, _)| (path.bounding_box().x0, path.bounding_box().x1)).collect();
    assert_eq!(strokes.len(), 1, "the two ends are one stroke: {reaches:?}");
    let segments: Vec<(f64, f64)> = strokes[0]
        .0
        .segments()
        .map(|s| (s.start().x.min(s.end().x), s.start().x.max(s.end().x)))
        .collect();
    assert_eq!(segments.len(), 2, "{segments:?}");
    assert!((segments[0].1 - 88.0).abs() < 1e-6, "the left end stops at 88: {segments:?}");
    assert!((segments[1].0 - 112.0).abs() < 1e-6, "the right end starts at 112: {segments:?}");
}

/// **A dashed stroke's pieces keep their dashes where they were**: the piece after the
/// region starts its dash at the phase the line had reached there.
#[test]
fn a_dashed_stroke_keeps_its_phase() {
    let mut doc = page_drawing("[10 10] 3 d 2 w 0 100 m 200 100 l S");
    redact(&mut doc, (95.0, 0.0, 105.0, 200.0));
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("it renders");
    let phases: Vec<(f64, f64)> = recorder
        .events
        .iter()
        .filter_map(|e| {
            let Event::Stroke { path, style, .. } = e else { return None };
            let start = path.segments().next()?.start().x;
            style.dash_pattern.as_ref().map(|(_, phase)| (start, *phase))
        })
        .collect();
    // The right piece starts at 106, half a pen past the region, and the line had run 106
    // there from a phase of 3.
    assert!(
        phases
            .iter()
            .any(|(start, phase)| (start - 106.0).abs() < 1e-6 && (phase - 109.0).abs() < 1e-6),
        "{phases:?}"
    );
    assert!(
        phases.iter().any(|(start, phase)| start.abs() < 1e-6 && (phase - 3.0).abs() < 1e-6),
        "{phases:?}"
    );
}

/// **A mitred corner whose point could reach the region is broken there**, so no point is
/// drawn into it; the rest of the outline is drawn as it was.
#[test]
fn a_mitred_corner_near_the_region_is_broken() {
    // A sharp V whose point is at (100, 150): its miter could reach far beyond the pen.
    let mut doc = page_drawing("4 w 10 M 0 j 60 10 m 100 150 l 140 10 l S");
    redact(&mut doc, (90.0, 160.0, 110.0, 200.0));
    let (_, strokes) = marks(&doc);
    let subpaths: usize = strokes
        .iter()
        .map(|(p, _)| p.elements().iter().filter(|e| matches!(e, kurbo::PathEl::MoveTo(_))).count())
        .sum();
    assert_eq!(subpaths, 2, "the corner was not broken: {strokes:?}");
}

/// A path the region misses is left as it was written.
#[test]
fn a_path_the_region_misses_is_left() {
    let mut doc = page_drawing("0 0 50 50 re f");
    let said = doc
        .what_redaction_removes(&Redaction {
            page: 0,
            regions: vec![(100.0, 100.0, 150.0, 150.0)],
            fill: None,
        })
        .expect("it reads");
    assert!(said.paths.is_empty(), "{:?}", said.paths);
    redact(&mut doc, (100.0, 100.0, 150.0, 150.0));
    let (fills, _) = marks(&doc);
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0].bounding_box(), kurbo::Rect::new(0.0, 0.0, 50.0, 50.0));
}

/// **A clipping path loses the region too**, so what it lets through outside is as it was
/// and nothing of its outline is left inside.
#[test]
fn a_clipping_path_loses_the_region() {
    let mut doc = page_drawing("q 0 0 200 200 re W n 0.5 g 0 0 200 200 re f Q");
    redact(&mut doc, (50.0, 50.0, 100.0, 100.0));
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("it renders");
    let clips: Vec<BezPath> = recorder
        .events
        .iter()
        .filter_map(|e| {
            if let Event::PushClip { path, ctm, .. } = e { Some(*ctm * path.clone()) } else { None }
        })
        .collect();
    assert_eq!(clips.len(), 1, "{clips:?}");
    assert_eq!(clips[0].winding(Point::new(75.0, 75.0)), 0, "the clip still covers the region");
    assert_ne!(clips[0].winding(Point::new(10.0, 10.0)), 0, "the clip lost what lies outside");
}
