//! Where a run says it draws, checked against where the page draws it.
//!
//! **A run's position is cumulative.** No operator states it: `Tm` sets the matrix, `Td`,
//! `TD` and `T*` step the line down from it, and every glyph drawn advances it by its own
//! width and the spacing in force (ISO 32000-2:2020, 9.4.2 to 9.4.4). `runs_of_page` now
//! carries the answer, which is the foundation moving a run needs — a caller cannot say
//! "put this somewhere else" without something knowing where it is.
//!
//! **The check is against a reader that was written separately.** `fepdf-render` computes
//! the same placement by its own route, for its own purpose, and the two agreeing over
//! real documents is evidence that neither is a restatement of the other. A test that
//! compared this walk to itself would pass whatever it said.

use fepdf::operation::Operation;
use fepdf::text::runs_of_page;
use fepdf::{IngestionOptions, PdfDocument};
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;

/// Where the renderer puts each run of a page, in the order it draws them.
fn drawn_at(doc: &PdfDocument, page: usize) -> Vec<(f64, f64)> {
    let mut recorder = Recorder::new();
    doc.render_page(page, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder.device_text_origins()
}

fn opened(name: &str) -> PdfDocument {
    let bytes = std::fs::read(format!("../../samples/{name}")).expect("the sample is there");
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens")
}

/// **The listing and the renderer agree about where every run is.**
///
/// Over the sample documents, first page each, 3434 runs. The renderer draws a run in
/// more pieces than the listing counts — a `TJ` kerned into several strings arrives as
/// several calls — so the listing's origins are checked as a *subsequence* of the drawn
/// ones rather than one for one. Every listed origin must turn up, in order, within half
/// a point.
///
/// Two samples are left out, each for a stated reason:
///
/// - `sample.pdf` is byte-identical to `constitution.pdf` (W-E3b), so counting it would
///   make this look broader than it is.
/// - `fugaku.pdf` is set in Type 3 fonts. A Type 3 glyph is a content stream that draws
///   paths, so the page arrives as 503 fills and **no text at all** — there is nothing
///   here to compare it with, rather than a disagreement.
#[test]
fn a_runs_origin_is_where_the_page_draws_it() {
    let samples = [
        "bokutokitan.pdf",
        "constitution.pdf",
        "fy05.pdf",
        "intel_sdm.pdf",
        "print_sample.pdf",
        "unicode_16.pdf",
        "volvo_xc90.pdf",
    ];
    let mut checked = 0usize;
    for name in samples {
        let doc = opened(name);
        let listed = runs_of_page(doc.inner(), 0).expect("it lists");
        let drawn = drawn_at(&doc, 0);
        assert!(!drawn.is_empty(), "{name}: the renderer drew no text, so nothing is compared");

        let mut next = 0usize;
        for run in &listed {
            let found = drawn[next..].iter().position(|at| {
                (run.origin.0 - at.0).abs() < 0.5 && (run.origin.1 - at.1).abs() < 0.5
            });
            let Some(offset) = found else {
                panic!(
                    "{name}: run {:?} says it draws at {:?}, and the page draws nothing there",
                    run.text, run.origin
                );
            };
            next += offset + 1;
            checked += 1;
        }
    }
    assert_eq!(checked, 3434, "the samples no longer hold the runs this was measured over");
}

/// A page that uses every operator the walk tracks, so that none of them is only
/// exercised by documents that happen to contain it.
fn page_using_every_placement() -> PdfDocument {
    // `q`/`cm`/`Q` around the text object, `TD` setting the leading that `T*` then moves
    // by, `Tz` scaling the advance, and `Tc`/`Tw` added to every glyph and every space.
    //
    // `AFTER SCALING` follows `SCALED` with nothing between, because a run placed by a
    // `Td` is placed from the line matrix and says nothing about what the run before it
    // advanced by. Without it, ignoring `Tz` altogether failed nothing.
    let content = "q 2 0 0 2 10 20 cm \
                   BT /F1 12 Tf 1 0 0 1 30 700 Tm (FIRST RUN) Tj \
                   40 -18 TD (SECOND) Tj \
                   T* (THIRD) Tj \
                   1.5 Tc 4 Tw (WITH SPACING) Tj \
                   50 Tz (SCALED) Tj (AFTER SCALING) Tj \
                   5 -6 Td (LAST) Tj ET Q";
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

/// **Every operator the walk tracks is checked, not only the ones the samples use.**
///
/// Reversing `T*`'s direction failed none of the sample comparison: not one of the seven
/// documents moves a line that way, so the branch was carried untested beside four that
/// the samples do exercise. A corpus says what real files do; it does not say that a
/// branch works, and the difference is invisible until something breaks and nothing
/// notices.
#[test]
fn every_operator_that_moves_the_text_is_followed() {
    let doc = page_using_every_placement();
    let listed = runs_of_page(doc.inner(), 0).expect("it lists");
    let drawn = drawn_at(&doc, 0);

    assert_eq!(
        listed.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
        vec!["FIRST RUN", "SECOND", "THIRD", "WITH SPACING", "SCALED", "AFTER SCALING", "LAST"],
        "the fixture does not draw what this is written about"
    );
    assert_eq!(listed.len(), drawn.len(), "the page draws a different number of runs");
    for (run, (listed, drawn)) in listed.iter().zip(drawn.iter()).enumerate() {
        assert!(
            (listed.origin.0 - drawn.0).abs() < 0.01 && (listed.origin.1 - drawn.1).abs() < 0.01,
            "run {run} ({:?}): the listing says {:?} and the page draws it at {drawn:?}",
            listed.text,
            listed.origin
        );
    }
}

/// **A run this engine reads short can still be cut, and loses no glyph.**
///
/// `decode` answers through `unified_map` and has no character for a code the font does
/// not map: on the first page of each sample, `unicode_16.pdf` loses 60 characters of 348
/// and `volvo_xc90.pdf` 2 of 2381. A cut that re-encoded what a run *read* would take
/// those glyphs off the page as a side effect of moving a boundary, so for a while such a
/// run was refused outright. The cut works on the codes instead, which asks nothing of
/// the reading.
///
/// What decides is the renderer: every glyph the page drew before the cut, it draws after.
#[test]
fn a_run_this_engine_reads_short_is_still_cut_without_loss() {
    let doc = opened("unicode_16.pdf");
    let listed = runs_of_page(doc.inner(), 0).expect("it lists");
    let lossy = listed
        .iter()
        .position(|run| run.pieces.iter().any(|piece| piece.is_empty()) && run.pieces.len() > 3)
        .expect("the sample still has a run with a code this engine cannot name");

    let mut before = Recorder::new();
    doc.render_page(0, &mut before, Affine::IDENTITY).expect("the page interprets");
    let drew = before.text();

    let mut cut = opened("unicode_16.pdf");
    cut.apply(Operation::SplitRun { page: 0, run: lossy, after: 3 }).expect("the cut applies");

    let mut after = Recorder::new();
    cut.render_page(0, &mut after, Affine::IDENTITY).expect("the page interprets");
    assert_eq!(after.text(), drew, "the cut changed what the page draws");
    assert_eq!(
        runs_of_page(cut.inner(), 0).expect("it lists").len(),
        listed.len() + 1,
        "the cut did not make two runs of one"
    );
}

/// And `pieces` is what turns a place in the text into a place among the codes.
#[test]
fn a_runs_pieces_are_what_each_of_its_codes_reads() {
    let doc = opened("unicode_16.pdf");
    for run in runs_of_page(doc.inner(), 0).expect("it lists") {
        assert_eq!(
            run.pieces.concat(),
            run.text,
            "the pieces of run {} do not run together into what it reads",
            run.index
        );
    }
}

/// Where the page draws each of its runs, after a round trip through a file.
fn drawn_after(doc: &PdfDocument, name: &str) -> Vec<(f64, f64)> {
    let path = std::env::temp_dir().join(format!("fepdf_move_run_{name}.pdf"));
    doc.save_with_options(&path, "2.0", &fepdf::SaveOptions::default()).expect("it writes");
    let written = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);
    let reopened = PdfDocument::open_with_options(written.into(), &IngestionOptions::default())
        .expect("what this engine wrote, this engine opens");
    drawn_at(&reopened, 0)
}

/// **A moved run draws from where it was put, and nothing else moves.**
///
/// The second half is the whole difficulty. A run that follows on the same line draws
/// from the current point, so taking one out of the middle of a text object shifts
/// everything after it — unless what it advanced the text matrix by is put back. The
/// fixture puts three runs on one line so that there is something after the move to be
/// wrong.
#[test]
fn a_moved_run_draws_where_it_was_put_and_the_rest_stays() {
    let doc = page_using_every_placement();
    let before = drawn_at(&doc, 0);

    let mut moved = page_using_every_placement();
    moved
        .apply(Operation::MoveRun { page: 0, run: 4, to: (300.0, 120.0) })
        .expect("the move applies");
    let after = drawn_after(&moved, "one");

    assert_eq!(after.len(), before.len(), "the page draws a different number of runs");
    assert!(
        (after[4].0 - 300.0).abs() < 0.1 && (after[4].1 - 120.0).abs() < 0.1,
        "the run was put at {:?} and draws at {:?}",
        (300.0, 120.0),
        after[4]
    );
    for (index, (before, after)) in before.iter().zip(after.iter()).enumerate() {
        if index == 4 {
            continue;
        }
        assert!(
            (before.0 - after.0).abs() < 0.1 && (before.1 - after.1).abs() < 0.1,
            "run {index} moved from {before:?} to {after:?} although another run was named"
        );
    }
}

/// And the listing agrees with the page about where it went.
#[test]
fn a_moved_run_keeps_its_number_and_says_where_it_is() {
    let mut doc = page_using_every_placement();
    let before = runs_of_page(doc.inner(), 0).expect("it lists");
    doc.apply(Operation::MoveRun { page: 0, run: 1, to: (200.0, 400.0) })
        .expect("the move applies");
    let after = runs_of_page(doc.inner(), 0).expect("it lists");

    assert_eq!(
        after.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
        before.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
        "the move renumbered the page's runs"
    );
    assert!(
        (after[1].origin.0 - 200.0).abs() < 0.1 && (after[1].origin.1 - 400.0).abs() < 0.1,
        "the listing says the moved run is at {:?}",
        after[1].origin
    );
}

/// Naming a run that is not there is an error, not a page with something else moved.
#[test]
fn moving_a_run_that_is_not_there_is_an_error() {
    let mut doc = page_using_every_placement();
    let error = doc
        .apply(Operation::MoveRun { page: 0, run: 99, to: (10.0, 10.0) })
        .expect_err("it refuses");
    assert!(error.to_string().contains("no run 99"), "the refusal does not say which: {error}");
}

/// A page whose run is set at an angle, so that moving it has something to lose.
fn page_drawing_at_an_angle() -> PdfDocument {
    let content = "BT /F1 12 Tf 0 2 -2 0 100 300 Tm (TURNED) Tj (AFTER) Tj ET";
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

/// **A move changes where a run is and nothing about how it is set.**
///
/// A run turned on its side and scaled keeps both. Only the translation of its matrix is
/// the caller's business here, and replacing the whole matrix would put the text upright
/// at the right place — which shifts no origin, so an origin is not what says this.
#[test]
fn a_moved_run_keeps_the_matrix_it_was_set_with() {
    let doc = page_drawing_at_an_angle();
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    let before = recorder.device_text_matrices()[0].as_coeffs();

    let mut moved = page_drawing_at_an_angle();
    moved
        .apply(Operation::MoveRun { page: 0, run: 0, to: (400.0, 500.0) })
        .expect("the move applies");
    let mut recorder = Recorder::new();
    moved.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    let after = recorder.device_text_matrices()[0].as_coeffs();

    for (index, (before, after)) in before.iter().zip(after.iter()).take(4).enumerate() {
        assert!(
            (before - after).abs() < 1e-9,
            "coefficient {index} of the matrix went from {before} to {after}"
        );
    }
    assert!(
        (after[4] - 400.0).abs() < 0.1 && (after[5] - 500.0).abs() < 0.1,
        "the run was put at (400, 500) and its matrix translates to {:?}",
        (after[4], after[5])
    );
}
