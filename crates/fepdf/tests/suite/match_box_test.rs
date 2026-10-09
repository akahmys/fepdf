//! Where a match is on the page, as opposed to where the run it fell in is.
//!
//! **A run is not a word in either direction.** `find_across_runs_test.rs` is the case
//! where a word is several runs; this is the case where a run is several words. A run of
//! 68 characters that holds a four-letter name is 68 characters wide, and the redaction
//! studio drew its box to that width (ROADMAP W-15) — a redaction that took the other 64
//! with it.
//!
//! **Checked against the renderer, which was written separately.** A cut at a code makes
//! that code the first of a run of its own, and the renderer then says where it draws the
//! run — by its own arithmetic, for its own purpose. So each code's place is compared
//! with where the page draws it once it is cut there. Comparing the place with the walk
//! that computed it would pass whatever it said.

use fepdf::operation::Operation;
use fepdf::text::runs_of_page;
use fepdf::{IngestionOptions, PdfDocument};
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;

/// Every placement the text state has, and a `TJ` offset inside a run: `Tc` and `Tw`
/// added to every glyph and space, `Tz` scaling them, and a number that moves the pen
/// without drawing.
const CONTENT: &str = "BT /F1 12 Tf 1 0 0 1 30 700 Tm 1.5 Tc 4 Tw 80 Tz \
                       [(Invoice ACME 2026) -700 (total)] TJ ET";

fn page() -> PdfDocument {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{CONTENT}\nendstream", CONTENT.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// Where the renderer draws the second run, once the only run is cut after `after` codes.
///
/// The renderer reports each string of a `TJ` as a call of its own, so the second run's
/// first string comes after the first run's strings: one of them up to the 17th code,
/// which is where the fixture's `-700` falls, and two after it.
fn drawn_after_cutting(after: usize) -> (f64, f64) {
    let mut doc = page();
    doc.apply(Operation::SplitRun { page: 0, run: 0, after }).expect("the cut applies");
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    let head = if after <= 17 { 1 } else { 2 };
    recorder.device_text_origins()[head]
}

/// **Every code is where the page draws it.**
///
/// All twenty-two, so a place that is right at the start of the run and drifts — a `Tc`
/// missed, a `Tz` applied twice, the `TJ` number counted against the wrong code — shows
/// up as the code it first goes wrong at.
#[test]
fn each_code_is_placed_where_the_page_draws_it() {
    let doc = page();
    let runs = runs_of_page(doc.inner(), 0).expect("it lists");
    assert_eq!(runs.len(), 1, "the fixture is not the one run this is written about");
    let run = &runs[0];
    assert_eq!(run.places.len(), run.pieces.len(), "not one place to a code");

    for (code, place) in run.places.iter().enumerate().skip(1) {
        let drawn = drawn_after_cutting(code);
        assert!(
            (place.origin.0 - drawn.0).abs() < 0.01 && (place.origin.1 - drawn.1).abs() < 0.01,
            "code {code} ({:?}) is placed at {:?} and the page draws it at {drawn:?}",
            run.pieces[code],
            place.origin
        );
    }
}

/// **A match's box is the match.**
///
/// ACME is four of the run's twenty-two codes. Its box starts where the page draws the A,
/// ends where it draws the space after the E, and is not the run's box.
#[test]
fn a_matchs_box_covers_the_match_and_not_its_run() {
    let doc = page();
    let runs = runs_of_page(doc.inner(), 0).expect("it lists");
    let found = fepdf::text::find_on_page(doc.inner(), 0, "ACME").expect("it searches");
    assert_eq!(found.len(), 1, "ACME is on the page once: {found:?}");
    let matched = &found[0].runs[0];

    let corners = runs[0].corners(matched.from, matched.to).expect("the run has those codes");
    let starts = drawn_after_cutting(8);
    let ends = drawn_after_cutting(12);
    assert!(
        (corners[0].0 - starts.0).abs() < 0.01,
        "the box starts at {:?} and the page draws the A at {starts:?}",
        corners[0]
    );
    assert!(
        (corners[1].0 - ends.0).abs() < 0.01,
        "the box ends at {:?} and the E ends at {ends:?}",
        corners[1]
    );
    let width = corners[1].0 - corners[0].0;
    assert!(
        width > 1.0 && width < runs[0].advance.0 / 2.0,
        "ACME is {width} wide in a run {} wide",
        runs[0].advance.0
    );
}

/// **A box that names no codes is not a box.**
///
/// An empty range, or one past the run's end, answers `None` rather than a box of no
/// width at the run's origin — which a caller would draw, and a reader would take for a
/// match at the start of the line.
#[test]
fn a_range_the_run_does_not_have_has_no_corners() {
    let doc = page();
    let run = &runs_of_page(doc.inner(), 0).expect("it lists")[0];
    assert!(run.corners(3, 3).is_none(), "an empty range has corners");
    assert!(run.corners(0, 0).is_none(), "an empty range at the start has corners");
    assert!(run.corners(20, 23).is_none(), "a range past the end has corners");
    assert!(run.corners(0, 22).is_some(), "the whole run has none");
}

/// **One place to a code on every sample**, which is what lets a code named by a match
/// be looked up among the places at all.
///
/// `pieces` comes from decoding and `places` from measuring, and they chunk the strings
/// separately: a font whose codes the two read at different widths would put every box
/// after its first multi-byte code on the wrong glyph.
#[test]
#[ignore = "needs samples/, which the repository does not hold"]
fn every_sample_run_has_a_place_for_each_code() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut runs_checked = 0usize;
    for name in [
        "bokutokitan.pdf",
        "constitution.pdf",
        "fugaku.pdf",
        "fy05.pdf",
        "intel_sdm.pdf",
        "print_sample.pdf",
        "unicode_16.pdf",
        "volvo_xc90.pdf",
    ] {
        let bytes = std::fs::read(dir.join(name)).expect("the sample is there");
        let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
            .expect("the sample opens");
        for run in runs_of_page(doc.inner(), 0).expect("it lists") {
            assert_eq!(
                run.places.len(),
                run.pieces.len(),
                "{name}: run {} has {} codes read and {} placed",
                run.index,
                run.pieces.len(),
                run.places.len()
            );
            runs_checked += 1;
        }
    }
    assert!(runs_checked > 3000, "only {runs_checked} runs were checked");
}
