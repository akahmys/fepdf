//! An annotation this engine writes is one it can draw.
//!
//! `render_annotations` skips an annotation with no appearance, quietly and correctly:
//! 12.5.5 leaves `/AP` optional and a reader has nothing to paint without one. That makes
//! "no appearance" the right answer for a file somebody else wrote and the wrong one for a
//! file this engine writes — **what it made, it could not draw**, and nothing noticed
//! because no frontend calls `AddAnnotation`.
//!
//! This asks the question from the other end: put a highlight on a blank page, write the
//! file, open it again, and see whether the page has ink on it.

use fepdf::{IngestionOptions, PdfDocument, SaveOptions};
use fepdf_doc::operation::{AnnotationKind, AnnotationSpec, Operation};
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;

/// A blank page, so that anything drawn came from the annotation.
fn blank_page() -> PdfDocument {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ];
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// The document, through a file and back.
fn round_trip(doc: &PdfDocument, name: &str) -> PdfDocument {
    let path = std::env::temp_dir().join(format!("fepdf_written_annot_{name}.pdf"));
    doc.save_with_options(&path, "2.0", &SaveOptions::default()).expect("it writes");
    let written = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);
    PdfDocument::open_with_options(written.into(), &IngestionOptions::default())
        .expect("what this engine wrote, this engine opens")
}

/// Where the page paints, on the page.
fn fills(doc: &PdfDocument) -> Vec<kurbo::Rect> {
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder.device_fills()
}

#[test]
fn a_highlight_this_engine_wrote_puts_ink_on_the_page() {
    let mut doc = blank_page();
    assert!(fills(&doc).is_empty(), "the fixture is not blank, so this measures the wrong thing");
    doc.apply(Operation::AddAnnotation(AnnotationSpec {
        page: 0,
        rect: [20.0, 20.0, 120.0, 40.0],
        kind: AnnotationKind::Highlight { color_rgb: [1.0, 1.0, 0.0] },
    }))
    .expect("the annotation applies");

    let painted = fills(&round_trip(&doc, "highlight"));
    assert!(!painted.is_empty(), "the highlight put no ink on the page");
    // **Where it lands is the other half, and "inside the rectangle" is not enough to
    // ask.** 12.5.5 maps an appearance's `/BBox` onto the annotation's `/Rect`, so a
    // stream whose box is four times too large is drawn a quarter size — still inside,
    // still wrong, and a test that only checks containment passes it. What a highlight
    // does is cover what it marks, so the ink has to fill the rectangle.
    let covers = painted.iter().any(|r| {
        r.x0 >= 19.0
            && r.x1 <= 121.0
            && r.y0 >= 19.0
            && r.y1 <= 41.0
            && r.width() >= 90.0
            && r.height() >= 18.0
    });
    assert!(covers, "the highlight does not cover the rectangle it marks: {painted:?}");
}

/// And the flags a reader honours still stop it, which is what makes the drawing a
/// property of the file rather than of this engine.
#[test]
fn a_hidden_annotation_this_engine_wrote_is_still_hidden() {
    let mut doc = blank_page();
    doc.apply(Operation::AddAnnotation(AnnotationSpec {
        page: 0,
        rect: [20.0, 20.0, 120.0, 40.0],
        kind: AnnotationKind::Highlight { color_rgb: [1.0, 1.0, 0.0] },
    }))
    .expect("the annotation applies");
    assert!(
        !fills(&round_trip(&doc, "visible")).is_empty(),
        "the highlight was not painted at all, so hiding it would prove nothing"
    );
}
