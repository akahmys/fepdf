//! Finding a word the page draws as several runs.
//!
//! **A run is not a word, and the vocabulary only offered runs.**
//! `Operation::EditRun` names one run by index, and measured over the samples on
//! 2026-09-24 the median run is **one character** in `constitution.pdf`, `fugaku.pdf` and
//! `volvo_xc90.pdf` — the last two are 100% single-character with a longest run of 1 —
//! against **68** in `intel_sdm.pdf`. "1 to 2 characters over the samples" was an average
//! with an order of magnitude inside it.
//!
//! So a reader could not say 日本国憲法: on the first page of `constitution.pdf` it is
//! four runs. `find_on_page` is the half of ROADMAP W-E3d that says where a string is;
//! rewriting one is the other half and is not here.
//!
//! `cargo run --release --example run_lengths` and `--example run_gaps` re-derive both
//! measurements.

use fepdf::{IngestionOptions, PdfDocument};

fn page_drawing(content: &str) -> PdfDocument {
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

/// **A word split across two show-text operators is one word.**
///
/// The two runs are carried one to the next by the text matrix, so `bc` is at the end of
/// the first and the start of the second, and is answered in codes because that is what
/// rewriting it would have to touch.
#[test]
fn a_match_crosses_the_runs_that_draw_it() {
    let doc = page_drawing("BT /F1 24 Tf 1 0 0 1 40 700 Tm (ab) Tj (cd) Tj ET");
    let found = fepdf::text::find_on_page(doc.inner(), 0, "bc").expect("it searches");

    assert_eq!(found.len(), 1, "bc spans the two runs and was not found: {found:?}");
    let runs = &found[0].runs;
    assert_eq!(runs.len(), 2, "the match names one run, not the two it covers: {runs:?}");
    assert_eq!((runs[0].run, runs[0].from, runs[0].to), (0, 1, 2), "the first half is wrong");
    assert_eq!((runs[1].run, runs[1].from, runs[1].to), (1, 0, 1), "the second half is wrong");
}

/// **And does not cross a break in the text matrix, which is the honest answer.**
///
/// The same four characters, with the second pair put somewhere else on the page by a new
/// `Tm`. `bc` is not a word there and must not be found — a search that joined them would
/// report a match a reader cannot see.
#[test]
fn a_match_does_not_cross_a_break_in_the_text_matrix() {
    let doc = page_drawing("BT /F1 24 Tf 1 0 0 1 40 700 Tm (ab) Tj 1 0 0 1 400 300 Tm (cd) Tj ET");
    let found = fepdf::text::find_on_page(doc.inner(), 0, "bc").expect("it searches");

    assert!(found.is_empty(), "two runs on opposite corners were read as one word: {found:?}");
    // Each half is still findable on its own, so the emptiness above is the join being
    // refused and not the search being broken.
    assert_eq!(
        fepdf::text::find_on_page(doc.inner(), 0, "ab").expect("it searches").len(),
        1,
        "the first run is not findable at all"
    );
}

/// **The entry's own example, on the file it names.**
///
/// 日本国憲法 on the first page of `constitution.pdf` is four runs — 日, 本, 国 and 憲法 —
/// and there was no way to name it. A fixture cannot check this: the shape being tested
/// is one real producers make and hand-written fixtures do not, which is the same defect
/// the decoration fixtures had (ROADMAP W-E3d).
#[test]
#[ignore = "needs samples/, which the repository does not hold"]
fn the_constitution_names_itself_across_four_runs() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/constitution.pdf");
    let Ok(bytes) = std::fs::read(&path) else {
        panic!("samples/constitution.pdf is not in this working copy");
    };
    let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens");

    let found = fepdf::text::find_on_page(doc.inner(), 0, "日本国憲法").expect("it searches");
    assert_eq!(found.len(), 1, "日本国憲法 is on the first page and was not found: {found:?}");
    assert_eq!(
        found[0].runs.len(),
        4,
        "the five characters are drawn by four runs: {:?}",
        found[0].runs
    );

    // And the run indices are the ones `runs_of_page` hands out, because an edit would
    // act on those.
    let runs = fepdf::text::runs_of_page(doc.inner(), 0).expect("it lists");
    let read: String = found[0]
        .runs
        .iter()
        .flat_map(|m| runs[m.run].pieces[m.from..m.to].iter().cloned())
        .collect();
    assert_eq!(read, "日本国憲法", "the codes the match names do not read back as the word");
}
