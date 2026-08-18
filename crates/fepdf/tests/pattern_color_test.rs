//! `scn` with a pattern name, which is a colour operator that takes a name (8.6.8.2).
//!
//! In a Pattern colour space the operands are an optional set of numbers followed by a
//! *name* keying the resource dictionary's `/Pattern` subdictionary. The interpreter
//! chose how to read the operands by counting them, so `/P1 scn` — one operand — was
//! read as one grey component, and failed because a name is not a number.
//!
//! Six pages of `samples/fy05.pdf` were unreadable for it, and because `inspect text`
//! returned on the first failure, the 718 pages after the first of them were unreadable
//! too. Neither was visible: `crosscheck_roundtrip.sh` measures text with PDFKit, which
//! reads all 846 pages, and never asked this engine.

use fepdf::{IngestionOptions, PdfDocument};

fn fy05() -> Option<PdfDocument> {
    let Ok(data) = std::fs::read("../../samples/fy05.pdf") else {
        return None;
    };
    Some(
        PdfDocument::open_with_options(data.into(), &IngestionOptions::default())
            .expect("it opens"),
    )
}

/// The six pages, by number, because a count would pass if the failure moved.
#[test]
fn the_pages_that_name_a_pattern_still_yield_their_text() {
    let Some(document) = fy05() else {
        eprintln!("Sample fy05.pdf not found, skipping");
        return;
    };
    for page in [128, 362, 390, 458, 660, 675] {
        let text = document
            .extract_text(page - 1)
            .unwrap_or_else(|e| panic!("page {page} would not extract: {e:?}"));
        // Non-empty rather than a size: the defect made these pages *fail*, not
        // shrink, and page 390 legitimately carries only 639 bytes — a threshold
        // picked to look substantial would have failed on a page that is simply sparse.
        assert!(!text.trim().is_empty(), "page {page} extracted no text");
    }
}

/// And every other page still works, so the fix is not a blanket "ignore `scn`".
#[test]
fn every_page_of_the_sample_extracts() {
    let Some(document) = fy05() else {
        eprintln!("Sample fy05.pdf not found, skipping");
        return;
    };
    let pages = document.page_count().expect("a page count");
    assert_eq!(pages, 846, "the sample changed; the page numbers above may have moved");

    let failed: Vec<usize> =
        (0..pages).filter(|&i| document.extract_text(i).is_err()).map(|i| i + 1).collect();
    assert!(failed.is_empty(), "pages that would not extract: {failed:?}");
}
