//! Comparing two documents, page by page (ROADMAP W-18): what the text says differently,
//! and where the page looks different.

use fepdf::compare::{compare, compare_text};
use fepdf::{IngestionOptions, PdfDocument};

/// A document of `pages`, each a content stream on a 300 by 200 page with Helvetica.
fn document(pages: &[&str]) -> PdfDocument {
    let count = pages.len();
    let kids: Vec<String> = (0..count).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [{}] /Count {count} >>", kids.join(" ")),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    for (i, content) in pages.iter().enumerate() {
        bodies.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents {} 0 R \
             /Resources << /Font << /F1 3 0 R >> >> >>",
            5 + 2 * i
        ));
        bodies.push(format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()));
    }
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

const PRICE_100: &str =
    "BT /F1 12 Tf 20 150 Td (Total) Tj ET BT /F1 12 Tf 20 100 Td (Price 100) Tj ET";
const PRICE_120: &str =
    "BT /F1 12 Tf 20 150 Td (Total) Tj ET BT /F1 12 Tf 20 100 Td (Price 120) Tj ET";

/// A document compared with itself differs nowhere.
#[test]
fn a_document_is_the_same_as_itself() {
    let a = document(&[PRICE_100]);
    let comparison = compare(&a, &document(&[PRICE_100]), 72.0).expect("it compares");
    assert!(comparison.differences.is_empty(), "{comparison:?}");
}

/// **A changed word is a line removed, a line added, and a region where it is drawn.**
#[test]
fn a_changed_word_is_found_in_the_text_and_on_the_page() {
    let comparison =
        compare(&document(&[PRICE_100]), &document(&[PRICE_120]), 72.0).expect("it compares");
    let [page] = comparison.differences.as_slice() else { panic!("{comparison:?}") };
    assert_eq!(
        (page.removed.as_slice(), page.added.as_slice()),
        (&["Price 100".to_owned()][..], &["Price 120".to_owned()][..])
    );
    // The number is drawn from about x 50 at y 100; the region holds it and not "Total".
    assert!(
        page.regions.iter().any(|r| r[0] <= 60.0 && r[2] >= 60.0 && r[1] <= 101.0 && r[3] >= 101.0),
        "no region where the number is: {:?}",
        page.regions
    );
    assert!(
        page.regions.iter().all(|r| r[1] < 145.0),
        "a region reaches the unchanged line: {:?}",
        page.regions
    );
}

/// **A mark moved and no word changed** is a region and no text.
#[test]
fn a_moved_mark_is_a_region_and_no_text() {
    let before = "0 g 20 20 40 20 re f";
    let after = "0 g 200 20 40 20 re f";
    let comparison = compare(&document(&[before]), &document(&[after]), 72.0).expect("it compares");
    let [page] = comparison.differences.as_slice() else { panic!("{comparison:?}") };
    assert!(page.removed.is_empty() && page.added.is_empty());
    assert_eq!(page.regions.len(), 2, "where it was and where it went: {:?}", page.regions);
}

/// **A page one document has and the other does not** is said to be only in one, with
/// its text.
#[test]
fn an_extra_page_is_only_in_one() {
    let comparison = compare_text(&document(&[PRICE_100]), &document(&[PRICE_100, PRICE_120]))
        .expect("it compares");
    assert_eq!(comparison.pages, (1, 2));
    let [page] = comparison.differences.as_slice() else { panic!("{comparison:?}") };
    assert_eq!(page.page, 1);
    assert!(page.only_in_one);
    assert_eq!(page.added, ["Total", "Price 120"]);
}
