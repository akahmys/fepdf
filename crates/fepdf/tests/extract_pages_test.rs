//! What `extract_pages` actually produces.
//!
//! It had one caller — `fepdf extract` — and no test. `create_empty` was in the same
//! position and had never once produced a well-formed file, so nothing about a function
//! being present and called is evidence that it works.

use fepdf::PdfDocument;

/// A sample from the corpus, or `None` when `samples/` is not in the tree.
fn sample(name: &str) -> Option<PdfDocument> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples").join(name);
    let bytes = std::fs::read(path).ok()?;
    PdfDocument::open(bytes.into()).ok()
}

/// A directory to write into, removed by the caller.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("fepdf-extract-{name}"));
    std::fs::create_dir_all(&dir).expect("a directory to write into");
    dir
}

#[test]
fn the_extracted_document_has_the_pages_asked_for_and_no_others() {
    let Some(doc) = sample("fy05.pdf") else { return };
    let out = doc.extract_pages(vec![2, 4, 6]).expect("three pages come out");
    assert_eq!(out.page_count().expect("it counts"), 3);
}

/// The pages that come out are the pages that went in, not the first three.
#[test]
fn the_pages_are_the_ones_named() {
    let Some(doc) = sample("fy05.pdf") else { return };
    let wanted = [2usize, 4, 6];
    let out = doc.extract_pages(wanted.to_vec()).expect("it extracts");
    for (place, &source) in wanted.iter().enumerate() {
        let before = doc.extract_text(source).expect("the source page has text");
        let after = out.extract_text(place).expect("the extracted page has text");
        assert_eq!(after, before, "extracted page {place} is not source page {source}");
    }
}

/// It must survive being written and read back — an in-memory arena is not a file.
#[test]
fn it_survives_being_written_and_read_back() {
    let Some(doc) = sample("fy05.pdf") else { return };
    let out = doc.extract_pages(vec![2, 4, 6]).expect("it extracts");
    let dir = scratch("round-trip");
    let path = dir.join("three.pdf");
    let _ = out.save_as_version(&path, "2.0").expect("it writes");

    let bytes = std::fs::read(&path).expect("it is on disk");
    let back = PdfDocument::open(bytes.into()).expect("it opens");
    let pages = back.page_count().expect("it counts");
    let decisions = back.decisions();
    let text = back.extract_text(0).expect("the first page has text");
    let source = doc.extract_text(2).expect("the source page has text");
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(pages, 3);
    assert!(decisions.is_empty(), "the written file needed repairing: {decisions:?}");
    assert_eq!(text, source, "the written file's first page is not the page extracted");
}

/// An empty selection is refused rather than answered with an empty document.
#[test]
fn extracting_nothing_is_refused() {
    let Some(doc) = sample("fy05.pdf") else { return };
    assert!(doc.extract_pages(Vec::new()).is_err());
}

/// A page index the document does not have is refused, not skipped.
#[test]
fn a_page_that_is_not_there_is_refused() {
    let Some(doc) = sample("fy05.pdf") else { return };
    let past_the_end = doc.page_count().expect("it counts");
    assert!(doc.extract_pages(vec![0, past_the_end]).is_err());
}

/// `merge` built its document the same way and reported the same nothing.
///
/// Its one caller is `fepdf merge`, which writes the result immediately — so the count
/// being wrong was invisible for as long as nobody asked the returned document anything.
#[test]
fn a_merged_document_knows_how_many_pages_it_has() {
    let Some(first) = sample("print_sample.pdf") else { return };
    let Some(second) = sample("sample.pdf") else { return };
    let total = first.page_count().expect("it counts") + second.page_count().expect("it counts");
    let merged = PdfDocument::merge(vec![first, second]).expect("they merge");
    assert_eq!(merged.page_count().expect("it counts"), total);
}

/// `InsertFrom` puts every page of the source in, and the document says so afterwards.
///
/// The counts are re-derived with
/// `cargo run --release --example page_counts -p fepdf -- samples/print_sample.pdf samples/sample.pdf`
/// — 23 and 13 on 2026-09-14.
#[test]
fn inserting_a_document_adds_all_of_its_pages() {
    let Some(mut doc) = sample("print_sample.pdf") else { return };
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/sample.pdf");
    let Ok(bytes) = std::fs::read(source) else { return };
    let before = doc.page_count().expect("it counts");
    let added = PdfDocument::open(bytes.clone().into()).expect("it opens").page_count().unwrap();

    doc.apply(fepdf::Operation::InsertFrom { source: bytes, at: 2 }).expect("it inserts");

    assert_eq!(doc.page_count().expect("it counts"), before + added);
    // The insertion went in at the position asked for: page 2 is the source's first page.
    let source_first = sample("sample.pdf").expect("it opens").extract_text(0).expect("it reads");
    assert_eq!(doc.extract_text(2).expect("it reads"), source_first);
}
