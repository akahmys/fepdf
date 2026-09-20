//! Changing the text of one run, and nothing else.
//!
//! **A run is what the file declares.** Which runs belong together is a question about
//! meaning that a content stream does not answer: characters drawn next to each other may
//! be a word, or a label and its value, or two columns set in one stream. Most PDFs are
//! not structured and many have a visual order unrelated to their sense, so a processor
//! that joined runs would be guessing at what it was editing — and guessing on the
//! caller's behalf about their own document
//! ([ADR-0091](../../../docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md)).
//!
//! So a caller lists the runs and names one. The cost is plain: a word is several runs in
//! a real file, and changing it is several edits. What is bought is that nothing is
//! changed that was not named.

use fepdf::{IngestionOptions, PdfDocument, SaveOptions};
use fepdf_doc::apply::text::runs_of_page;
use fepdf_doc::operation::Operation;
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;

/// A page drawing `first` and then `second`, as two runs.
fn page_drawing(first: &str, second: &str) -> PdfDocument {
    let content = format!("BT /F1 24 Tf 1 0 0 1 40 700 Tm ({first}) Tj ({second}) Tj ET");
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

fn round_trip(doc: &PdfDocument, name: &str) -> PdfDocument {
    let path = std::env::temp_dir().join(format!("fepdf_edit_run_{name}.pdf"));
    doc.save_with_options(&path, "2.0", &SaveOptions::default()).expect("it writes");
    let written = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);
    PdfDocument::open_with_options(written.into(), &IngestionOptions::default())
        .expect("what this engine wrote, this engine opens")
}

/// **The listing and the edit agree about which run is which.**
///
/// Two counters for one thing is the shape of ADR-0064, where the interpreter's operator
/// index and another way of counting met at 9 and nowhere else. Here one walk answers
/// both questions, and this is what says so.
#[test]
fn the_run_a_caller_reads_is_the_run_the_edit_changes() {
    let mut doc = page_drawing("ALPHA", "BETA");
    let listed = runs_of_page(doc.inner(), 0).expect("it lists");
    assert_eq!(
        listed.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
        vec!["ALPHA", "BETA"],
        "the listing does not read the page"
    );

    doc.apply(Operation::EditRun { page: 0, run: 1, text: "OMEGA".to_string() })
        .expect("the edit applies");

    let text = round_trip(&doc, "second").extract_text(0).expect("it extracts");
    assert!(text.contains("ALPHA"), "the run that was not named changed: {text:?}");
    assert!(text.contains("OMEGA"), "the run that was named did not change: {text:?}");
    assert!(!text.contains("BETA"), "the old text is still there: {text:?}");
}

/// **What follows moves by the difference, because that is what the operators do.**
///
/// The run is longer, so the text after it on the line starts further along. Nothing is
/// re-wrapped and nothing else is touched.
#[test]
fn the_text_after_a_longer_run_moves_along_with_it() {
    let mut doc = page_drawing("A", "TAIL");
    let before = doc.extract_spans(0).expect("it extracts")[1].x;

    doc.apply(Operation::EditRun { page: 0, run: 0, text: "AAAAAAAA".to_string() })
        .expect("the edit applies");
    let after = round_trip(&doc, "advance").extract_spans(0).expect("it extracts")[1].x;

    assert!(after > before + 10.0, "the following run did not move: {before} then {after}");
}

/// And a shorter run brings it back.
#[test]
fn the_text_after_a_shorter_run_moves_back() {
    let mut doc = page_drawing("AAAAAAAA", "TAIL");
    let before = doc.extract_spans(0).expect("it extracts")[1].x;

    doc.apply(Operation::EditRun { page: 0, run: 0, text: "A".to_string() })
        .expect("the edit applies");
    let after = round_trip(&doc, "shorter").extract_spans(0).expect("it extracts")[1].x;

    assert!(after < before - 10.0, "the following run did not move back: {before} then {after}");
}

/// A character the run's font cannot draw is refused by name, and the page is unchanged.
#[test]
fn a_character_the_font_cannot_draw_is_refused() {
    let mut doc = page_drawing("ALPHA", "BETA");
    let refused = doc.apply(Operation::EditRun { page: 0, run: 0, text: "図面".to_string() });
    let said = refused.expect_err("Helvetica draws no 図").to_string();
    assert!(said.contains('図'), "the refusal does not name the character: {said}");
    assert!(doc.extract_text(0).expect("it extracts").contains("ALPHA"));
}

/// Naming a run the page does not have says so rather than doing nothing.
#[test]
fn naming_a_run_that_is_not_there_is_an_error() {
    let mut doc = page_drawing("ALPHA", "BETA");
    let refused = doc.apply(Operation::EditRun { page: 0, run: 99, text: "X".to_string() });
    let said = refused.expect_err("there is no run 99").to_string();
    assert!(said.contains("99"), "the error does not name what was asked for: {said}");
}

/// **A real page lists as runs of one or two characters**, which is what makes naming one
/// the honest interface: there is no phrase to point at that the file agrees is a phrase.
#[test]
fn a_real_page_lists_its_runs() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/constitution.pdf");
    let bytes = std::fs::read(&path).expect("the sample is in the tree");
    let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("it opens");
    let runs = runs_of_page(doc.inner(), 0).expect("it lists");
    assert!(runs.len() > 100, "a page of this document draws many runs: {}", runs.len());
    let joined: String = runs.iter().map(|r| r.text.as_str()).collect();
    assert!(joined.contains("日本国憲法"), "the runs do not read what the page reads");
}

/// **A split puts the same glyphs in the same places, and gives a caller two names.**
///
/// Consecutive show-text operators draw from the current point, so cutting a run in two
/// needs no position arithmetic — and what it buys is that a reader can say "this part of
/// this run is a thing of its own", which is the half of the grouping question the engine
/// must not answer for them.
#[test]
fn a_split_run_draws_the_same_and_lists_as_two() {
    let mut doc = page_drawing("ALPHA", "BETA");
    let before: Vec<f64> = doc.extract_spans(0).expect("it extracts").iter().map(|s| s.x).collect();

    doc.apply(Operation::SplitRun { page: 0, run: 0, after: 2 }).expect("the split applies");

    let listed = runs_of_page(doc.inner(), 0).expect("it lists");
    assert_eq!(
        listed.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
        vec!["AL", "PHA", "BETA"],
        "the run did not become two"
    );

    let reopened = round_trip(&doc, "split");
    assert_eq!(
        reopened.extract_text(0).expect("it extracts").replace(char::is_whitespace, ""),
        "ALPHABETA",
        "the page reads differently after a split"
    );
    let after: Vec<f64> =
        reopened.extract_spans(0).expect("it extracts").iter().map(|s| s.x).collect();
    assert!(
        (after.last().unwrap_or(&0.0) - before.last().unwrap_or(&0.0)).abs() < 0.01,
        "what followed the split run moved: {before:?} then {after:?}"
    );
}

/// Either half can then be named, which is the point of splitting at all.
#[test]
fn either_half_of_a_split_can_be_edited() {
    let mut doc = page_drawing("ALPHA", "BETA");
    doc.apply(Operation::SplitRun { page: 0, run: 0, after: 2 }).expect("the split applies");
    doc.apply(Operation::EditRun { page: 0, run: 1, text: "OHA".to_string() })
        .expect("the edit applies");

    let text = round_trip(&doc, "half").extract_text(0).expect("it extracts");
    assert!(text.contains("ALOHA"), "the second half was not the one changed: {text:?}");
}

/// A cut outside the run says so rather than doing nothing.
#[test]
fn a_cut_outside_the_run_is_refused() {
    let mut doc = page_drawing("ALPHA", "BETA");
    for after in [0usize, 5, 99] {
        let refused = doc.apply(Operation::SplitRun { page: 0, run: 0, after });
        let said = refused.expect_err("a run of five characters cannot be cut there").to_string();
        assert!(said.contains('5'), "the refusal does not say what the run reads: {said}");
    }
}

/// A page whose runs are drawn by `'`, which moves to the next line before it draws.
///
/// `aw` is the word spacing the leading `"` sets, so two of these differing only in `aw`
/// say what a delete kept.
fn page_quoting(aw: f64) -> PdfDocument {
    let content = format!("BT /F1 24 Tf 1 0 0 1 40 700 Tm 30 TL {aw} 0 (X) \" (A B) ' (Z) Tj ET");
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

/// **A deleted run is off the page, and what followed it on the line moves back.**
///
/// This is the whole of it: the glyphs are gone, and the text after them closes up, which
/// is what shortening a run already does taken to its end.
#[test]
fn a_deleted_run_is_gone_and_the_rest_closes_up() {
    let mut doc = page_drawing("AAAAAAAA", "TAIL");
    let start = doc.extract_spans(0).expect("it extracts")[0].x;
    let before = doc.extract_spans(0).expect("it extracts")[1].x;
    assert!(before > start + 10.0, "the fixture does not draw them apart");

    doc.apply(Operation::DeleteRun { page: 0, run: 0 }).expect("the delete applies");
    let reopened = round_trip(&doc, "deleted");

    let text = reopened.extract_text(0).expect("it extracts");
    assert!(!text.contains("AAAAAAAA"), "the deleted run is still drawn: {text:?}");
    assert!(text.contains("TAIL"), "the run that was not named went too: {text:?}");
    let after = reopened.extract_spans(0).expect("it extracts")[0].x;
    assert!(
        (after - start).abs() < 1.0,
        "the following run did not close up: it starts at {after}, not at {start}"
    );
}

/// **Deleting a run is not editing it to nothing**, and this is where the two part.
///
/// An emptied run keeps its number, so a caller can put text back into it. A deleted one
/// is gone from the listing and the runs after it move up. Both are reachable on purpose;
/// a caller that meant one and got the other would find out by the numbering shifting
/// under a later edit.
#[test]
fn an_emptied_run_is_still_a_run_and_a_deleted_one_is_not() {
    let mut emptied = page_drawing("ALPHA", "BETA");
    emptied
        .apply(Operation::EditRun { page: 0, run: 0, text: String::new() })
        .expect("the edit applies");
    let listed = runs_of_page(emptied.inner(), 0).expect("it lists");
    assert_eq!(listed.len(), 2, "emptying a run took it out of the listing");
    assert_eq!(listed[0].text, "", "the emptied run still reads something");
    assert_eq!(listed[1].text, "BETA", "the run after it was renumbered by an emptying");

    let mut deleted = page_drawing("ALPHA", "BETA");
    deleted.apply(Operation::DeleteRun { page: 0, run: 0 }).expect("the delete applies");
    let listed = runs_of_page(deleted.inner(), 0).expect("it lists");
    assert_eq!(listed.len(), 1, "the deleted run is still in the listing");
    assert_eq!(listed[0].text, "BETA", "the run that moved up is not the one that followed");
}

/// **What the operator did besides draw survives the delete.**
///
/// `'` moves to the next line and then shows (ISO 32000-2:2020, Table 107). Taking the
/// whole operator out would take the line movement with it, and every line below would
/// rise by one leading — a delete of four characters moving the rest of the page.
#[test]
fn deleting_a_run_keeps_the_line_it_moved_to() {
    let mut doc = page_quoting(0.0);
    let before = doc.extract_spans(0).expect("it extracts");
    let line_of_second = before[1].y;

    doc.apply(Operation::DeleteRun { page: 0, run: 0 }).expect("the delete applies");
    let after = round_trip(&doc, "line").extract_spans(0).expect("it extracts");

    let moved = after.iter().find(|span| span.text.contains('A')).expect("it is still drawn");
    assert!(
        (moved.y - line_of_second).abs() < 1.0,
        "deleting the run above moved this line from {line_of_second} to {}",
        moved.y
    );
}

/// And so does the spacing it set.
///
/// `"` sets word and character spacing before it shows (ISO 32000-2:2020, Table 107), and
/// those are still in force for what is drawn next. Two pages differing only in `aw` say
/// whether the delete left it alone: with the word spacing, the run after the one
/// containing a space starts further right.
///
/// This measures through the renderer because it is the only reader here that honours
/// word spacing — every `set_word_spacing` on the extraction side is an empty body, so
/// extraction reads the two pages identically whatever the delete did.
#[test]
fn deleting_a_run_keeps_the_spacing_it_set() {
    let last_run_x = |aw: f64, name: &str| {
        let mut doc = page_quoting(aw);
        doc.apply(Operation::DeleteRun { page: 0, run: 0 }).expect("the delete applies");
        let doc = round_trip(&doc, name);
        let mut recorder = Recorder::new();
        doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
        let origins = recorder.text_origins();
        assert_eq!(origins.len(), 2, "the page after the delete does not draw two runs");
        origins[1].0
    };

    let (wide, narrow) = (last_run_x(20.0, "wide"), last_run_x(0.0, "narrow"));
    assert!(
        wide - narrow > 15.0,
        "the word spacing the deleted run set did not survive it: {wide} against {narrow}"
    );
}

/// Naming a run that is not there is an error, not a page with something else taken off.
#[test]
fn deleting_a_run_that_is_not_there_is_an_error() {
    let mut doc = page_drawing("ALPHA", "BETA");
    let error = doc.apply(Operation::DeleteRun { page: 0, run: 7 }).expect_err("it refuses");
    assert!(error.to_string().contains("no run 7"), "the refusal does not say which run: {error}");
    let text = doc.extract_text(0).expect("it extracts");
    assert!(text.contains("ALPHA") && text.contains("BETA"), "a refused delete changed the page");
}
