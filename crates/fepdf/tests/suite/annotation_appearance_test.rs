//! Drawing the appearance streams annotations carry (12.5.5), and the flags that stop it.
//!
//! **6.3.2.2 is not a subset a processor chooses.** It says that when a PDF processor
//! renders a page it shall render the appropriate appearance stream for all annotations
//! that have one, unless the annotation flags say otherwise — and this engine drew none
//! at all. A page whose only mark is an annotation came out blank while every other
//! reader painted it; `pdf20examples/PDF 2.0 UTF-8 string and annotation.pdf` is exactly
//! that page and is now in `crosscheck_image.sh`.
//!
//! What is asserted here is the part a picture cannot show: which annotations are skipped
//! and why, and that the appearance lands where `/Rect` says rather than where its own
//! coordinates would have put it.

use fepdf::{IngestionOptions, PdfDocument};
use kurbo::Affine;

use fepdf_fixtures::recorder::Recorder;

use fepdf_fixtures::assemble;

/// A page with **no content of its own** and one annotation, so anything drawn came from
/// the appearance. `annot` is merged into the annotation dictionary; `extra` are objects
/// from 6 onward.
fn page_with_annotation(annot: &str, extra: &[String]) -> Vec<u8> {
    // The appearance paints its whole bounding box, so where it lands is measurable.
    let appearance = "0 0 0 rg 0 0 10 10 re f\n";
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [4 0 R] >>".to_string(),
        format!("<< /Type /Annot /Subtype /Square /Rect [20 40 120 90] {annot} >>"),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Resources << >> \
             /Length {} >>\nstream\n{appearance}endstream",
            appearance.len()
        ),
    ];
    bodies.extend_from_slice(extra);

    assemble(&bodies)
}

/// Draws page 1 and reports the marks and the decisions taken.
fn draw(file: Vec<u8>) -> (Recorder, Vec<String>) {
    let doc = PdfDocument::open_with_options(file.into(), &IngestionOptions::default())
        .expect("the fixture opens");
    let mut marks = Recorder::new();
    doc.render_page(0, &mut marks, Affine::IDENTITY).expect("the page renders");
    let decisions = doc.decisions().iter().map(|d| format!("{} {}", d.clause, d.found)).collect();
    (marks, decisions)
}

/// **The gap.** A page with no `/Contents` and one annotation drew nothing at all.
#[test]
fn an_annotation_with_an_appearance_is_drawn() {
    let (marks, _) = draw(page_with_annotation("/AP << /N 5 0 R >>", &[]));
    assert_eq!(
        marks.device_fills().len(),
        1,
        "the appearance was not drawn: {:?}",
        marks.device_fills()
    );
}

/// And it lands on `/Rect`, not on its own coordinates. The appearance's box is 10x10 at
/// the origin and the rectangle is 100x50 at (20, 40); 12.5.5's algorithm scales and
/// translates the first onto the second.
#[test]
fn the_appearance_is_placed_on_the_annotations_rectangle() {
    let (marks, _) = draw(page_with_annotation("/AP << /N 5 0 R >>", &[]));
    let placed = marks.device_fills()[0];
    let (x0, y0, x1, y1) = (placed.x0, placed.y0, placed.x1, placed.y1);
    for (got, want, what) in
        [(x0, 20.0, "left"), (y0, 40.0, "bottom"), (x1, 120.0, "right"), (y1, 90.0, "top")]
    {
        assert!(
            (got - want).abs() < 0.01,
            "the {what} edge is at {got}, and /Rect puts it at {want}"
        );
    }
}

/// Bit 2 is `Hidden`: do not render it, whatever its type (12.5.3, Table 167).
#[test]
fn the_hidden_flag_stops_it() {
    let (marks, _) = draw(page_with_annotation("/F 2 /AP << /N 5 0 R >>", &[]));
    assert!(
        marks.device_fills().is_empty(),
        "a hidden annotation was drawn: {:?}",
        marks.device_fills()
    );
}

/// Bit 6 is `NoView`: not on a screen, which is what this renders to.
#[test]
fn the_noview_flag_stops_it() {
    let (marks, _) = draw(page_with_annotation("/F 32 /AP << /N 5 0 R >>", &[]));
    assert!(
        marks.device_fills().is_empty(),
        "a no-view annotation was drawn: {:?}",
        marks.device_fills()
    );
}

/// Bit 3 is `Print`, which says nothing about the screen. An annotation carrying only
/// that flag is still drawn — reading it as "print only" would hide half a document.
#[test]
fn the_print_flag_alone_does_not_stop_it() {
    let (marks, _) = draw(page_with_annotation("/F 4 /AP << /N 5 0 R >>", &[]));
    assert_eq!(
        marks.device_fills().len(),
        1,
        "the print flag hid an annotation: {:?}",
        marks.device_fills()
    );
}

/// `/N` may be a dictionary of states, and `/AS` says which is current — that is how a
/// checkbox keeps `/Off` and `/Yes` in one place.
#[test]
fn the_state_named_by_as_is_the_one_drawn() {
    let off = "\n"; // draws nothing
    let file = page_with_annotation(
        "/AS /Yes /AP << /N << /Yes 5 0 R /Off 6 0 R >> >>",
        &[format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Resources << >> \
             /Length {} >>\nstream\n{off}endstream",
            off.len()
        )],
    );
    let (marks, _) = draw(file);
    assert_eq!(marks.device_fills().len(), 1, "the /Yes state should have drawn its square");
}

/// A dictionary of states with no `/AS` and more than one to choose from draws **nothing**
/// and says so. Picking one would be this engine deciding what the document did not.
#[test]
fn a_state_dictionary_with_no_as_draws_nothing_and_says_so() {
    let off = "\n";
    let file = page_with_annotation(
        "/AP << /N << /Yes 5 0 R /Off 6 0 R >> >>",
        &[format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Resources << >> \
             /Length {} >>\nstream\n{off}endstream",
            off.len()
        )],
    );
    let (marks, decisions) = draw(file);
    assert!(marks.device_fills().is_empty(), "{:?}", marks.device_fills());
    assert!(
        decisions.iter().any(|d| d.starts_with("12.5.5") && d.contains("2 states")),
        "the omission was not reported: {decisions:?}"
    );
}

/// One state and no `/AS` is not ambiguous, so it is drawn and the omission recorded as a
/// repair rather than a refusal.
#[test]
fn a_single_state_with_no_as_is_drawn() {
    let (marks, decisions) = draw(page_with_annotation("/AP << /N << /Yes 5 0 R >> >>", &[]));
    assert_eq!(marks.device_fills().len(), 1, "{:?}", marks.device_fills());
    assert!(decisions.iter().any(|d| d.starts_with("12.5.5")), "{decisions:?}");
}

/// An annotation carries `/OC` too (8.11.3.2), and a group that is off hides it. Nothing
/// exercised this before, because nothing drew annotations at all.
#[test]
fn an_annotation_in_a_layer_that_is_off_is_not_drawn() {
    let mut file = page_with_annotation(
        "/OC 6 0 R /AP << /N 5 0 R >>",
        &["<< /Type /OCG /Name (Hidden) >>".to_string()],
    );
    // The catalogue needs the configuration that turns it off.
    let patched = String::from_utf8_lossy(&file).replace(
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [6 0 R] \
             /D << /OFF [6 0 R] >> >> >>",
    );
    file = patched.into_bytes();
    // The offsets moved, and the reader recovers by scanning — which is what makes this
    // fixture legible instead of arithmetic.
    let (marks, _) = draw(file);
    assert!(
        marks.device_fills().is_empty(),
        "an annotation in a layer that is off was drawn: {:?}",
        marks.device_fills()
    );
}

/// An annotation with no appearance at all is not drawn and is not an error: a `/Link` is
/// the commonest annotation in the corpus and 30,016 of them carry none.
#[test]
fn an_annotation_with_no_appearance_is_skipped_quietly() {
    let (marks, decisions) = draw(page_with_annotation("", &[]));
    assert!(marks.device_fills().is_empty());
    assert!(decisions.is_empty(), "{decisions:?}");
}
