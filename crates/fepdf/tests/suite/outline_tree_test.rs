//! Reading `/Outlines` back out, and writing what was read.
//!
//! The counts below were re-derived on 2026-09-13 with
//! `cargo run --release --example outline_survey -p fepdf`, and each was checked against
//! `fepdf inspect interactive`, whose reader walks the same tree with a queue instead of
//! recursion and shares no code with this one. Both say 135, 3,853, 23 and 1,319.

use fepdf::PdfDocument;
use fepdf_doc::{Operation, read_outlines};
use fepdf_model::document::extensions::{OutlineNode, OutlineTree};

/// A document with no `/Outlines` answers an empty tree, not an error.
#[test]
fn a_document_without_bookmarks_reads_as_empty() {
    let doc = PdfDocument::create_empty().expect("a new document opens");
    let (tree, report) = read_outlines(doc.inner());
    assert!(tree.items.is_empty(), "read {} items from a blank document", tree.items.len());
    assert_eq!(report.items, 0);
    assert!(!report.looped);
}

/// What the operation wrote is what comes back.
#[test]
fn what_was_written_is_what_is_read() {
    let mut doc = PdfDocument::create_empty().expect("a new document opens");
    let written = OutlineTree {
        items: vec![
            OutlineNode {
                title: "第一章".into(),
                destination_page: 0,
                children: vec![OutlineNode {
                    title: "1.1 まえがき".into(),
                    destination_page: 0,
                    children: Vec::new(),
                }],
            },
            OutlineNode { title: "Appendix".into(), destination_page: 0, children: Vec::new() },
        ],
    };
    doc.apply(Operation::UpdateOutlines(written.clone())).expect("the outline is written");

    let (read, report) = read_outlines(doc.inner());
    assert_eq!(read, written);
    assert_eq!(report.items, 3, "two roots and one child");
    assert_eq!(report.placeless, 0);
    assert!(!report.looped);
}

/// The tree survives being written to a file and read back from one.
///
/// The title is deliberately outside PDFDocEncoding. `/Title` was being written as raw
/// UTF-8 bytes in a byte string, which 7.9.2.2 does not admit: an ASCII title round-trips
/// through that bug untouched and would have proved nothing.
#[test]
fn the_tree_survives_a_round_trip_through_a_file() {
    let mut doc = PdfDocument::create_empty().expect("a new document opens");
    let written = OutlineTree {
        items: vec![OutlineNode {
            title: "表紙 — Cover".into(),
            destination_page: 0,
            children: Vec::new(),
        }],
    };
    doc.apply(Operation::UpdateOutlines(written.clone())).expect("the outline is written");

    let dir = std::env::temp_dir().join("fepdf-outline-round-trip");
    std::fs::create_dir_all(&dir).expect("a directory to write into");
    let path = dir.join("bookmarked.pdf");
    doc.save_as_version(&path, "2.0").expect("it writes");

    let bytes = std::fs::read(&path).expect("it is on disk");
    let reopened = PdfDocument::open(bytes.into()).expect("it opens");
    let (read, report) = read_outlines(reopened.inner());
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(read, written);
    // Without this the test cannot tell a bookmark that resolved to page 0 from one that
    // resolved to nothing, since `destination_page` falls back to 0 either way.
    assert_eq!(report.placeless, 0);
}

/// A `/Next` that points back at its own chain stops the walk instead of hanging it.
///
/// Nothing in the format forbids the cycle and nothing in the corpus contains one, so
/// this is the only thing that exercises the visited set — without it the bound is a
/// comment.
#[test]
fn a_looping_chain_is_reported_rather_than_followed() {
    let doc = PdfDocument::open(looping_outline()).expect("the document opens");
    let (tree, report) = read_outlines(doc.inner());
    assert_eq!(report.items, 2, "the two distinct items are both read");
    assert!(report.looped, "the cycle was not noticed");
    assert_eq!(tree.items.len(), 2);
}

/// Two outline items whose `/Next` links form a ring: 5 → 6 → 5.
fn looping_outline() -> bytes::Bytes {
    use std::fmt::Write as _;
    const BODIES: [&str; 6] = [
        "<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>",
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
        "<< /Type /Outlines /First 5 0 R /Last 6 0 R /Count 2 >>",
        "<< /Title (first) /Parent 4 0 R /Next 6 0 R /Dest [3 0 R /Fit] >>",
        "<< /Title (second) /Parent 4 0 R /Prev 5 0 R /Next 5 0 R /Dest [3 0 R /Fit] >>",
    ];
    let mut out = String::from("%PDF-2.0\n");
    let mut offsets = Vec::with_capacity(BODIES.len());
    for (n, body) in BODIES.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{body}\nendobj\n", n + 1);
    }
    let start_xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", BODIES.len() + 1);
    for offset in &offsets {
        let _ = writeln!(out, "{offset:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start_xref}\n%%EOF\n",
        BODIES.len() + 1
    );
    bytes::Bytes::from(out.into_bytes())
}

/// Every bookmark in the corpus names a page, and no chain doubles back.
#[test]
fn the_corpus_reads_the_same_numbers_the_other_reader_reports() {
    // (file, items) — `fepdf inspect interactive` reports the same four, 2026-09-13.
    const EXPECTED: [(&str, usize); 4] = [
        ("fy05.pdf", 135),
        ("intel_sdm.pdf", 3853),
        ("print_sample.pdf", 23),
        ("unicode_16.pdf", 1319),
    ];
    let samples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    if !samples.is_dir() {
        // `.gitignore` excludes `samples/`, so a fresh clone has none.
        return;
    }
    for (name, items) in EXPECTED {
        let Ok(bytes) = std::fs::read(samples.join(name)) else { continue };
        let doc = PdfDocument::open(bytes.into()).expect("the sample opens");
        let (tree, report) = read_outlines(doc.inner());
        assert_eq!(report.items, items, "{name} reported {} items", report.items);
        assert_eq!(report.placeless, 0, "{name} has a bookmark that names no page");
        assert!(!report.looped, "{name}'s outline doubles back");
        assert!(!tree.items.is_empty(), "{name} read no roots");
    }
}
