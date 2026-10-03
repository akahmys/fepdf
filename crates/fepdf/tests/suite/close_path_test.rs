//! What `h` does to the current point, and what it does when there is nothing to close
//! (ISO 32000-2, 8.5.2.1).
//!
//! **Both halves were wrong until 2026-09-09, and a comment stood where the answer
//! should have been**: *"current_point remains at the point where close_path was called?
//! Actually, PDF spec says it's the start of the subpath. But for common usage, tracking
//! it after close is tricky."* The clause is not in doubt, and the tracking is one field.

use fepdf::PdfDocument;
use fepdf_fixtures::assemble;
use fepdf_fixtures::recorder::{Event, Recorder};
use kurbo::Affine;

/// The elements of every path the page painted.
fn painted(content: &str) -> Vec<String> {
    let file = assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ]);
    let doc = PdfDocument::open(file.into()).expect("the fixture opens");
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder
        .events
        .iter()
        .filter_map(|e| {
            if let Event::Stroke { path, .. } | Event::Fill { path, .. } = e {
                Some(format!("{:?}", path.elements()))
            } else {
                None
            }
        })
        .collect()
}

/// `v` takes the current point as its first control point, and after `h` that is the
/// start of the subpath rather than where the last segment ended.
///
/// The two streams differ only in the `h`, so the first control point is the whole of
/// what this asserts: `(10, 10)` against `(50, 10)`.
#[test]
fn the_current_point_after_h_is_the_start_of_the_subpath() {
    let closed = painted("10 10 m 50 10 l h 30 40 60 70 v S\n");
    let open = painted("10 10 m 50 10 l 30 40 60 70 v S\n");

    assert_eq!(closed.len(), 1, "one stroked path: {closed:?}");
    assert!(
        closed[0].contains("CurveTo((10.0, 10.0)"),
        "after `h` the curve starts from the subpath's start: {}",
        closed[0]
    );
    assert!(
        open[0].contains("CurveTo((50.0, 10.0)"),
        "without `h` it starts from the last segment's end: {}",
        open[0]
    );
}

/// A stream that closes a subpath it never opened is malformed, and drawing what follows
/// is the whole of what this engine owes it.
///
/// **`kurbo::BezPath::close_path` debug-asserts on an empty path**, so this aborted the
/// process until the guard was added. `samples/fugaku.pdf` is where it was found, by
/// `crates/fepdf/tests/parser_twin_test.rs`.
#[test]
fn h_before_any_m_draws_nothing_and_does_not_abort() {
    let drawn = painted("h 0 0 10 10 re f\n");
    assert_eq!(drawn.len(), 1, "the rectangle after the stray `h` is still painted");
    assert!(
        drawn[0].starts_with("[MoveTo((0.0, 0.0))"),
        "the stray `h` left nothing in front of it: {}",
        drawn[0]
    );
}
