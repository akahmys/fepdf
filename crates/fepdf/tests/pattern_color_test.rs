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
use std::sync::OnceLock;

/// `samples/fy05.pdf`, opened once for the whole binary.
///
/// **Opening it is most of what these tests cost.** The document is 846 pages and a debug
/// build takes about twenty seconds over it, which the first test below paid in full to
/// read six pages. Sharing one open takes this binary from **47.2s to 40.3s**, measured
/// A/B under one load, two runs each.
///
/// The saving is smaller than the open it removes because the two tests run on separate
/// threads, so the two opens overlapped: what is recovered is the second open's *work*,
/// not its wall clock, plus one 846-page arena's worth of memory. It removes duplicated
/// work rather than coverage, which is the only kind of speed-up worth taking here.
///
/// `None` when the sample is absent: `.gitignore` excludes `/samples/`, so a fresh clone
/// has no corpus and these skip rather than fail.
fn fy05() -> Option<&'static PdfDocument> {
    static DOCUMENT: OnceLock<Option<PdfDocument>> = OnceLock::new();
    DOCUMENT
        .get_or_init(|| {
            let data = std::fs::read("../../samples/fy05.pdf").ok()?;
            Some(
                PdfDocument::open_with_options(data.into(), &IngestionOptions::default())
                    .expect("it opens"),
            )
        })
        .as_ref()
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
