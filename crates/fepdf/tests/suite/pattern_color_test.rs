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
//!
//! **The corpus tests here skip when `samples/` is absent, which `.gitignore` makes it on
//! every machine but the one that generated it** — so on a fresh clone they passed
//! without running, which is [ADR-0068](../../../../docs/adr/0068-a-suite-that-skipped-itself-and-asserted-nothing.md)'s
//! shape. They are kept, because 846 real pages is what they measure and a fixture cannot
//! stand in for that. What was missing is a test of the *defect*, which needs no corpus:
//! `a_page_that_paints_with_a_pattern_yields_its_text` builds the operand shape that
//! failed and runs everywhere.

use fepdf::{IngestionOptions, PdfDocument};
use std::sync::OnceLock;

use fepdf_fixtures::assemble;

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

/// `/Pattern cs` then `/P1 scn`, which is the operand shape that used to fail.
///
/// One operand, and it is a name. The interpreter chose how to read `scn`'s operands by
/// counting them, so a single operand meant a single grey component and a name is not a
/// number. This needs no corpus: the failure is in reading one content stream.
#[test]
fn a_page_that_paints_with_a_pattern_yields_its_text() {
    let content = "q /Pattern cs /P1 scn 0 0 100 100 re f Q\n\
                   BT /F1 24 Tf 20 120 Td (Painted beside a pattern) Tj ET\n";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 4 0 R \
          /Resources << /Font << /F1 5 0 R >> /Pattern << /P1 6 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        {
            let cell = "1 0 0 RG 0 0 m 10 10 l S\n";
            format!(
                "<< /Type /Pattern /PatternType 1 /PaintType 1 /TilingType 1 \
                  /BBox [0 0 10 10] /XStep 10 /YStep 10 /Resources << >> /Length {} >>\n\
                 stream\n{cell}endstream",
                cell.len()
            )
        },
    ];

    let out = assemble(&objects);

    let document =
        PdfDocument::open_with_options(bytes::Bytes::from(out), &IngestionOptions::default())
            .expect("the fixture opens");

    let text = document.extract_text(0).expect("the page extracts");
    assert!(
        text.contains("Painted beside a pattern"),
        "the page paints with a pattern and still shows its text: {text:?}"
    );
}
