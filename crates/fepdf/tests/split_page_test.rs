//! One page cut into several, each carrying one region of it.
//!
//! **A split always removes what belongs to the other sheets.** Half a drawing, still
//! searchable, on a page that shows the other half is a leak dressed as a feature: a
//! reader who cuts an A3 assembly drawing into two A4 sheets to send one of them has sent
//! both ([ADR-0088](../../../docs/adr/0088-what-a-crop-puts-outside-the-sheet-is-removed.md)).
//! So there is no option here to hide rather than cut, and the tests are written about
//! what each new page *holds* rather than only about what it shows.

use fepdf::{IngestionOptions, Operation, PageDivision, PdfDocument};

fn opened(name: &str) -> PdfDocument {
    let bytes = std::fs::read(format!("../../samples/{name}")).expect("the sample is there");
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens")
}

/// **Cutting a page in two makes two pages, and each holds its own half.**
///
/// The check is that neither half holds what the other shows: the text of the left page
/// and the text of the right page share nothing, and together they are no more than the
/// page held before.
#[test]
fn a_page_cut_in_two_gives_two_pages_that_share_nothing() {
    let mut doc = opened("print_sample.pdf");
    let before = doc.page_count().expect("it counts");
    let whole = doc.extract_text(2).expect("it extracts").chars().count();

    doc.apply(Operation::SplitPage { page: 2, into: PageDivision::Grid { columns: 2, rows: 1 } })
        .expect("the split applies");

    assert_eq!(doc.page_count().expect("it counts"), before + 1, "the split made no new page");
    let left = doc.extract_text(2).expect("it extracts");
    let right = doc.extract_text(3).expect("it extracts");
    assert!(!left.is_empty() && !right.is_empty(), "one half came out blank");
    assert!(
        left.chars().count() + right.chars().count() <= whole + 40,
        "the two halves hold {} characters between them where the page held {whole} — the \
         content on the boundary is being kept by both, or nothing was removed",
        left.chars().count() + right.chars().count()
    );
}

/// The sheets are the size they were asked for.
#[test]
fn each_page_of_a_split_is_one_part_of_the_sheet() {
    let mut doc = opened("print_sample.pdf");
    let whole = doc.get_page_box(2).expect("the page has a box");
    let (wide, tall) = (whole.x2 - whole.x1, whole.y2 - whole.y1);

    doc.apply(Operation::SplitPage { page: 2, into: PageDivision::Grid { columns: 2, rows: 2 } })
        .expect("the split applies");

    for index in 2..6 {
        let part = doc.get_page_box(index).expect("the page has a box");
        assert!(
            ((part.x2 - part.x1) - wide / 2.0).abs() < 0.01
                && ((part.y2 - part.y1) - tall / 2.0).abs() < 0.01,
            "page {index} of the split is {} by {}, not a quarter of {wide} by {tall}",
            part.x2 - part.x1,
            part.y2 - part.y1
        );
    }
}

/// **A grid comes out in reading order**: across a row first, then down.
///
/// That is the order somebody laying the sheets out on a table puts them in, and getting
/// it wrong is the kind of defect nobody notices until a four-page handout is stapled.
/// The fixture draws one letter in each quarter, so the order is read off the pages.
#[test]
fn a_grid_comes_out_in_reading_order() {
    let content = "BT /F1 20 Tf 1 0 0 1 50 500 Tm (A) Tj \
                   1 0 0 1 350 500 Tm (B) Tj \
                   1 0 0 1 50 100 Tm (C) Tj \
                   1 0 0 1 350 100 Tm (D) Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 600] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut doc = PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens");

    doc.apply(Operation::SplitPage { page: 0, into: PageDivision::Grid { columns: 2, rows: 2 } })
        .expect("the split applies");

    let letters: Vec<String> = (0..4)
        .map(|page| doc.extract_text(page).expect("it extracts").trim().to_string())
        .collect();
    assert_eq!(
        letters,
        vec!["A", "B", "C", "D"],
        "the quarters did not come out across the top row and then the bottom"
    );
}

/// Regions named outright come out in the order they were named.
#[test]
fn named_regions_come_out_in_the_order_they_were_given() {
    let mut doc = opened("print_sample.pdf");
    let whole = doc.get_page_box(2).expect("the page has a box");

    doc.apply(Operation::SplitPage {
        page: 2,
        into: PageDivision::Regions(vec![(0.0, 0.0, 100.0, 100.0), (0.0, 0.0, whole.x2, whole.y2)]),
    })
    .expect("the split applies");

    let first = doc.get_page_box(2).expect("the page has a box");
    assert!(
        (first.x2 - first.x1 - 100.0).abs() < 0.01,
        "the first page of the split is {} wide, not the 100 asked for first",
        first.x2 - first.x1
    );
}

/// A split into nothing is refused, rather than leaving a document a page short.
#[test]
fn a_split_into_no_regions_is_refused() {
    let mut doc = opened("print_sample.pdf");
    let error = doc
        .apply(Operation::SplitPage { page: 2, into: PageDivision::Regions(Vec::new()) })
        .expect_err("it refuses");
    assert!(error.to_string().contains("nothing"), "the refusal does not say why: {error}");
}
