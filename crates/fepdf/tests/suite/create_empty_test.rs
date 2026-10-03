//! A new document is read, not repaired.
//!
//! `create_empty` was a byte string carrying a cross-reference table written out by hand,
//! and all four numbers in it were wrong — object 2 declared at 60 and lying at 58, object
//! 3 at 120 and lying at 115, `startxref` one byte before the table. It opened anyway,
//! because ingestion scanned the file and rebuilt what the table should have said. Eleven
//! callers begin from this function.

use fepdf::{IngestionOptions, PdfDocument};

#[test]
fn a_new_document_needs_no_repair() {
    let doc = PdfDocument::create_empty().expect("an empty document");
    let decisions = doc.decisions();
    assert!(
        decisions.is_empty(),
        "opening a document this library wrote took {} decisions: {}",
        decisions.len(),
        decisions.iter().map(|d| d.found.clone()).collect::<Vec<_>>().join("; ")
    );
}

#[test]
fn a_new_document_has_one_blank_page() {
    let doc = PdfDocument::create_empty().expect("an empty document");
    assert_eq!(doc.page_count().expect("a page tree"), 1);
    let page = doc.get_page_box(0).expect("a page box");
    assert_eq!((page.x2 - page.x1, page.y2 - page.y1), (612.0, 792.0));
    assert_eq!(doc.extract_text(0).expect("the page interprets"), "");
}

/// **Saved and read back**, which is the half a well-formed table is for: the writer is
/// handed a document the reader did not have to guess at.
#[test]
fn it_survives_a_round_trip() {
    let doc = PdfDocument::create_empty().expect("an empty document");
    let dir = std::env::temp_dir().join("fepdf-create-empty");
    std::fs::create_dir_all(&dir).expect("a place to write");
    let path = dir.join("blank.pdf");
    let notices = doc.save_as_version(&path, "2.0").expect("it writes");
    assert!(notices.is_empty(), "writing a blank page had something to say: {notices:?}");

    let bytes = std::fs::read(&path).expect("it is there");
    let again = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("it opens");
    assert_eq!(again.page_count().expect("a page tree"), 1);
    assert!(again.decisions().is_empty(), "the round trip needed repairing");
    std::fs::remove_file(&path).ok();
}
