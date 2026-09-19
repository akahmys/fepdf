//! One face, embedded once, however many pages it is shown on.
//!
//! **Embedding per run cost more than the document.** Thirteen Bates footers took
//! `samples/constitution.pdf` from 244,790 bytes to 830,167, because each page got its own
//! subset of the same face; with one embedding of the union of their glyphs it is 277,392,
//! so the face costs 32,602 bytes rather than 585,377. A hundred-page document would have
//! paid a hundred times over.
//!
//! Both callers know every string before they touch the first page — a decoration shows
//! the same text on all of them, and Bates numbering generates its labels — so neither has
//! any reason to find out one page at a time.

use fepdf::{IngestionOptions, PdfDocument, SaveOptions};
use fepdf_doc::operation::{DecorationPosition, Operation, PageSelection};

/// A document of `pages` blank pages.
fn blank_pages(pages: usize) -> PdfDocument {
    let kids: String = (0..pages).map(|i| format!("{} 0 R ", i + 3)).collect::<Vec<_>>().concat();
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [{kids}] /Count {pages} >>"),
    ];
    for _ in 0..pages {
        bodies.push("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string());
    }
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// How many font programs `doc` carries, counted in the file it writes.
///
/// Uncompressed and without object streams, so that the count is of what is there rather
/// than of what a search can see through.
fn font_programs_in(doc: &PdfDocument, name: &str) -> usize {
    let path = std::env::temp_dir().join(format!("fepdf_one_face_{name}.pdf"));
    let options = SaveOptions { compress: false, obj_stm: false, ..SaveOptions::default() };
    doc.save_with_options(&path, "2.0", &options).expect("it writes");
    let bytes = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);
    bytes.windows(9).filter(|w| *w == b"/FontFile").count()
}

#[test]
fn bates_numbering_embeds_one_face_for_every_page() {
    let mut doc = blank_pages(8);
    doc.apply(Operation::ApplyBatesNumbering {
        pages: PageSelection::All,
        prefix: "X-".to_string(),
        start_number: 1,
        digits: 4,
        position: DecorationPosition::BottomRight,
    })
    .expect("it numbers");
    assert_eq!(
        font_programs_in(&doc, "bates"),
        1,
        "eight pages of Bates numbers embedded more than one face"
    );
}

#[test]
fn a_decoration_embeds_one_face_for_every_page() {
    let mut doc = blank_pages(8);
    doc.apply(Operation::AddPageDecoration {
        pages: PageSelection::All,
        text: "DRAFT".to_string(),
        position: DecorationPosition::TopCenter,
        layer: None,
    })
    .expect("it decorates");
    assert_eq!(
        font_programs_in(&doc, "decoration"),
        1,
        "eight decorations embedded more than one face"
    );
}

/// And every page still says what it should, which is what makes the sharing worth having
/// rather than merely cheap.
#[test]
fn each_page_keeps_its_own_number() {
    let mut doc = blank_pages(4);
    doc.apply(Operation::ApplyBatesNumbering {
        pages: PageSelection::All,
        prefix: "X-".to_string(),
        start_number: 1,
        digits: 4,
        position: DecorationPosition::BottomRight,
    })
    .expect("it numbers");
    for page in 0..4 {
        let text = doc.extract_text(page).expect("the page extracts");
        assert!(
            text.contains(&format!("X-{:04}", page + 1)),
            "page {page} does not carry its own number: {text:?}"
        );
    }
}
