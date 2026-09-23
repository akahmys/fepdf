//! What an edit leaves behind in the arena, and what it does not leave in the file.
//!
//! **The arena has no free list and never reclaims.** `write_page_content` allocates a
//! new stream object for the page's contents and points `/Contents` at it; the object it
//! replaced stays where it is, holding the bytes the page used to draw. Measured
//! 2026-09-23 on a page of two runs: **one object and one dictionary per edit, and
//! nothing given back**. Eight objects before a hundred edits, a hundred and eight after,
//! with 102 streams holding 9,333 bytes between them.
//!
//! **That is a cost, and this file is about why it is only a cost.** Two things make it
//! bounded in consequence rather than in size: the writer emits what the document
//! reaches, so the orphans never reach the file; and every walk of the arena by index
//! runs either at ingest, before an edit can have happened, or over bytes read afresh.
//! Neither is structural. The second was not always true — `list_fonts` walked every
//! handle and reported **24 fonts for a document with 12** — and the first is what this
//! file holds.

use fepdf::{IngestionOptions, Operation, PdfDocument, SaveOptions};

/// A page drawing two runs, so that `EditRun` has something to edit.
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

/// Writes `doc` and hands back the bytes.
fn written(doc: &PdfDocument, name: &str) -> Vec<u8> {
    let path = std::env::temp_dir().join(format!("fepdf_arena_growth_{name}.pdf"));
    doc.save_with_options(&path, "2.0", &SaveOptions::default()).expect("it writes");
    let bytes = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);
    bytes
}

/// **A hundred edits and one edit write the same file.**
///
/// This is the guarantee that makes the arena's growth a cost and not a defect: the
/// writer emits what the document reaches, so the ninety-nine content streams an
/// afternoon of editing left in the arena are not in what a reader receives. A writer
/// that walked the arena instead would put every draft of the page in the file, and the
/// document would grow without bound on disk as well as in memory.
#[test]
fn a_hundred_edits_and_one_edit_write_the_same_file() {
    let mut many = page_drawing("Hello", " world");
    for i in 0..100 {
        many.apply(Operation::EditRun { page: 0, run: 0, text: format!("Hello{i}") })
            .expect("it edits");
    }
    let mut once = page_drawing("Hello", " world");
    once.apply(Operation::EditRun { page: 0, run: 0, text: "Hello99".into() }).expect("it edits");

    assert_eq!(
        written(&many, "many").len(),
        written(&once, "once").len(),
        "the drafts an edit replaced reached the file"
    );

    // And the file says what the last edit said, so the sizes agreeing is not two empty
    // documents agreeing.
    let reopened =
        PdfDocument::open_with_options(written(&many, "read").into(), &IngestionOptions::default())
            .expect("it reopens");
    assert_eq!(
        reopened.extract_text(0).expect("it extracts").trim(),
        "Hello99 world",
        "the hundredth edit is not what the file draws"
    );
}

/// **What an edit costs the arena, as a number, so that a change to it is visible.**
///
/// One object and one dictionary per edit. The assertion is a bound rather than an
/// equality: what would matter is growth becoming superlinear, or an edit starting to
/// cost several objects, and neither is what "no reclamation" means today. If reclamation
/// is ever built — ROADMAP W-A5, and `ObjectEntry.generation` was the check it would have
/// wanted — this is where the number changes.
#[test]
fn an_edit_costs_one_object_and_gives_none_back() {
    let mut doc = page_drawing("Hello", " world");
    let before = doc.arena_stats().object_count;

    const EDITS: u32 = 100;
    for i in 0..EDITS {
        doc.apply(Operation::EditRun { page: 0, run: 0, text: format!("Hello{i}") })
            .expect("it edits");
    }
    let after = doc.arena_stats().object_count;

    assert!(
        after >= before + EDITS,
        "an edit stopped costing an object — if that is reclamation, this test is the \
         record of what it replaced: {before} to {after} over {EDITS} edits"
    );
    assert!(
        after <= before + EDITS + 8,
        "an edit costs more than one object now: {before} to {after} over {EDITS} edits"
    );
}
