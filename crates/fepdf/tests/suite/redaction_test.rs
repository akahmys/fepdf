//! Redaction removes the text the caller named, and only that text.
//!
//! **Nothing here was covered before 2026-09-06.** Redaction is reachable from the MCP
//! server and the viewer and not from the CLI, and its only test asserted `is_ok()` — so
//! the operation had never been shown to remove anything. Measured against four runs on
//! one page, it removed the second one and no other, whatever rectangle it was given:
//!
//! | rectangle | result |
//! | :--- | :--- |
//! | over the first run | nothing changed, `Ok(())` |
//! | over the second run | the second run redacted |
//! | **the whole page** | **the second run only; three of four survived** |
//!
//! **The count it reports is what it removed.** `RedactionReport.redacted_count` was
//! `args.targets.len()` under a field documented "Number of redactions successfully
//! scrubbed", so a rectangle over empty space was reported to the caller — an agent,
//! through `fepdf-mcp` — as a redaction that had happened.
//!
//! The cause was two counters. `Interpreter::op_index` counts operator tokens and is
//! incremented before the operator runs, so the four `Tj`s report 4, 9, 14 and 19.
//! `rewrite_redacted_tokens` counted operators *and* strings and tested a string before
//! incrementing, so it offered 3, 9, 15 and 21. The two agree on 9 and nowhere else.

use fepdf::PdfDocument;

use fepdf_fixtures::assemble;

/// Four runs, one per line, a hundred points apart so a rectangle can name exactly one.
fn four_runs() -> Vec<u8> {
    let content = "BT /F1 24 Tf 72 700 Td (AAA) Tj ET\n\
                   BT /F1 24 Tf 72 600 Td (BBB) Tj ET\n\
                   BT /F1 24 Tf 72 500 Td (CCC) Tj ET\n\
                   BT /F1 24 Tf 72 400 Td (DDD) Tj ET";
    assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
          /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ])
}

/// What survives a rectangle, and how many the engine says it scrubbed.
fn surviving(rect: [f32; 4]) -> (Vec<String>, usize) {
    let doc = PdfDocument::open(bytes::Bytes::from(four_runs())).expect("the fixture opens");
    let scrubbed = doc.apply_redaction_to_page(0, &[rect]).expect("redaction runs");
    let text = doc.extract_text(0).expect("text comes back");
    let left = ["AAA", "BBB", "CCC", "DDD"]
        .into_iter()
        .filter(|w| text.contains(w))
        .map(str::to_string)
        .collect();
    (left, scrubbed)
}

/// Each run in turn, including the first, which used to be unreachable.
#[test]
fn a_rectangle_removes_the_run_under_it_and_no_other() {
    for (word, y) in [("AAA", 700.0_f32), ("BBB", 600.0), ("CCC", 500.0), ("DDD", 400.0)] {
        let (left, scrubbed) = surviving([60.0, y - 10.0, 300.0, y + 30.0]);
        let expected: Vec<String> = ["AAA", "BBB", "CCC", "DDD"]
            .into_iter()
            .filter(|w| *w != word)
            .map(str::to_string)
            .collect();
        assert_eq!(left, expected, "redacting {word} must remove {word} and nothing else");
        assert_eq!(scrubbed, 1, "and must report the one it removed");
    }
}

/// A rectangle over the whole page removes all of it.
///
/// This is the case that says the two counters agree everywhere rather than at one
/// point: they used to intersect in exactly one place, so this left three of four.
#[test]
fn a_rectangle_over_the_page_removes_every_run() {
    let (left, scrubbed) = surviving([0.0, 0.0, 612.0, 792.0]);
    assert_eq!(left, Vec::<String>::new());
    assert_eq!(scrubbed, 4, "all four are reported, not the one rectangle asked for");
}

/// A rectangle over nothing leaves the page alone.
///
/// Without this, a redaction that scrubbed every string unconditionally would pass the
/// two tests above.
#[test]
fn a_rectangle_over_nothing_removes_nothing() {
    let (left, scrubbed) = surviving([0.0, 0.0, 10.0, 10.0]);
    assert_eq!(left, ["AAA", "BBB", "CCC", "DDD"]);
    assert_eq!(
        scrubbed, 0,
        "and says nothing was scrubbed — the count is what was removed, not what was asked"
    );
}

/// A page the interpreter cannot finish is refused, not reported as redacted.
///
/// **This is the other half of the defect, and it is the dangerous half.** A page that
/// will not interpret yields no spans, so nothing intersects the rectangles, so the
/// scrub finds nothing to do — and the caller used to be told `Ok(())` about a page
/// whose text is still there. For an operation whose whole purpose is that something is
/// *gone*, "I could not read it" and "there was nothing there" must not be the same
/// answer.
///
/// `Do` naming an XObject the file does not contain is what a damaged document looks
/// like, and it is the case a caller most needs told about.
#[test]
fn a_page_that_will_not_interpret_is_refused_rather_than_reported_as_redacted() {
    let content = "BT /F1 24 Tf 72 700 Td (AAA) Tj ET\n/NoSuchXObject Do";
    let bytes = assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
          /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ]);
    let doc = PdfDocument::open(bytes::Bytes::from(bytes)).expect("the fixture opens");

    let refused = doc.apply_redaction_to_page(0, &[[0.0, 0.0, 612.0, 792.0]]);
    assert!(refused.is_err(), "a page that cannot be read cannot be redacted");

    let why = format!("{refused:?}");
    assert!(why.contains("nothing was redacted"), "the refusal must say what was not done: {why}");
    assert!(why.contains("NoSuchXObject"), "and name what stopped it: {why}");

    // The text is indeed still on the page, and this test cannot read it to say so:
    // `extract_text` interprets the same stream and fails the same way. That is the
    // shape of the defect rather than a gap in the test — the only thing that ever
    // *succeeded* on this page was the redaction that did nothing.
}
