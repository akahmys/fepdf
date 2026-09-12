//! A `/MCID` comes back with the box it drew in (14.7.4.2).
//!
//! The engine read `/MCID` nowhere until this file's subject was built: the one match for
//! it in the workspace was the writer that stamps ids into a content stream. A structure
//! element could therefore say *which marks are mine* and get no answer, which is why the
//! GUI's reading-order overlay drew nothing on every document ever opened in it.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_fixtures::assemble;

/// A stream object with `extra` merged into its dictionary.
fn stream(extra: &str, data: &str) -> String {
    format!("<< {extra} /Length {} >>\nstream\n{data}endstream", data.len())
}

/// The marked-content boxes of a one-page document whose content is `content`.
fn boxes(resources: &str, content: &str) -> std::collections::BTreeMap<u32, kurbo::Rect> {
    let bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
             /Resources << {resources} >> /Contents 4 0 R >>"
        ),
        stream("", content),
    ];
    let doc =
        PdfDocument::open_with_options(assemble(&bodies).into(), &IngestionOptions::default())
            .expect("the fixture opens");
    doc.marked_content_boxes(0).expect("the page interprets")
}

#[test]
fn a_filled_rectangle_gives_its_section_that_rectangle() {
    // The one shape whose box is exact: no font metric is approximated, and the path is
    // the answer.
    let found = boxes("", "/P << /MCID 0 >> BDC\n10 20 30 40 re f\nEMC\n");
    let measured = found.get(&0).expect("the section drew something");
    assert_eq!((measured.x0, measured.y0), (10.0, 20.0));
    assert_eq!((measured.x1, measured.y1), (40.0, 60.0));
}

#[test]
fn a_section_that_draws_nothing_has_no_box() {
    // Absent rather than empty: a zero-sized rectangle at the origin would be drawn by a
    // consumer as a mark on the page, and there is no mark.
    let found = boxes("", "/P << /MCID 7 >> BDC\nEMC\n");
    assert!(found.is_empty(), "an empty section reported {found:?}");
}

#[test]
fn the_ctm_is_applied_before_the_box_is_kept() {
    // `cm` is the difference between where a path is written and where it lands, and a
    // box that ignored it would be wrong on every page that scales its content.
    let found = boxes("", "/P << /MCID 0 >> BDC\nq 2 0 0 2 5 5 cm 10 10 10 10 re f Q\nEMC\n");
    let measured = found.get(&0).expect("the section drew something");
    assert_eq!((measured.x0, measured.y0), (25.0, 25.0));
    assert_eq!((measured.x1, measured.y1), (45.0, 45.0));
}

#[test]
fn a_nested_section_marks_both_itself_and_the_one_around_it() {
    // 14.7.4.2 lets a `/P` hold spans that are each their own section. The paragraph's
    // box has to include them: a structure element that references only the outer id is
    // the shape every tagged document in the corpus writes.
    let found = boxes(
        "",
        "/P << /MCID 0 >> BDC\n0 0 10 10 re f\n\
         /Span << /MCID 1 >> BDC\n100 100 10 10 re f\nEMC\nEMC\n",
    );
    let outer = found.get(&0).expect("the paragraph drew something");
    let inner = found.get(&1).expect("the span drew something");
    assert_eq!((outer.x0, outer.y0, outer.x1, outer.y1), (0.0, 0.0, 110.0, 110.0));
    assert_eq!((inner.x0, inner.y0, inner.x1, inner.y1), (100.0, 100.0, 110.0, 110.0));
}

/// The same page, with an optional content group that `/D` `/OFF` turns off around the
/// mark. Written out in full rather than through `boxes`, because the switch lives in the
/// catalog and `boxes` does not write one.
fn boxes_with_group_off() -> std::collections::BTreeMap<u32, kurbo::Rect> {
    let content = "/P << /MCID 0 >> BDC\n/OC /OC1 BDC\n0 0 10 10 re f\nEMC\nEMC\n";
    let bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R /OCProperties          << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200]          /Resources << /Properties << /OC1 5 0 R >> >> /Contents 4 0 R >>"
            .to_string(),
        stream("", content),
        "<< /Type /OCG /Name (a layer that is off) >>".to_string(),
    ];
    let doc =
        PdfDocument::open_with_options(assemble(&bodies).into(), &IngestionOptions::default())
            .expect("the fixture opens");
    doc.marked_content_boxes(0).expect("the page interprets")
}

#[test]
fn an_optional_content_section_that_is_off_contributes_no_box() {
    // What is not on the page is not in the structure element's rectangle either. The
    // measurement sits behind the same guard that withholds the marks, so this cannot
    // drift from what a reader sees.
    let found = boxes_with_group_off();
    assert!(found.is_empty(), "a hidden layer reported {found:?}");
}

#[test]
fn the_same_section_with_the_group_on_does_keep_its_box() {
    // The other half, so the assertion above is known to be measuring the group and not
    // the nesting: the identical content, with no `/OCProperties` to turn anything off.
    let found = boxes(
        "/Properties << /OC1 5 0 R >>",
        "/P << /MCID 0 >> BDC\n/OC /OC1 BDC\n0 0 10 10 re f\nEMC\nEMC\n",
    );
    let measured = found.get(&0).expect("the section drew something");
    assert_eq!((measured.x0, measured.y0, measured.x1, measured.y1), (0.0, 0.0, 10.0, 10.0));
}

#[test]
fn a_named_property_list_carries_its_id_too() {
    // 14.6.2 allows the operand to be a name into `/Properties`. The corpus writes 409
    // `BDC`s that way, against 14,494 written in place.
    let found = boxes("/Properties << /MC0 << /MCID 3 >> >>", "/P /MC0 BDC\n1 2 3 4 re f\nEMC\n");
    let measured = found.get(&3).expect("the named section drew something");
    assert_eq!((measured.x0, measured.y0, measured.x1, measured.y1), (1.0, 2.0, 4.0, 6.0));
}

/// A sample from the corpus, opened.
fn sample(name: &str) -> PdfDocument {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples").join(name);
    let bytes = std::fs::read(&path).expect("the sample is in the tree");
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens")
}

#[test]
fn a_tagged_sample_comes_back_with_boxes_on_its_page() {
    // `print_sample.pdf` writes 794 `/MCID`s across 4 pages; its first page holds 8, and
    // the numbers below are the page's own — a box outside the page box would mean the
    // user-space mapping was skipped, which is the way this can silently be wrong.
    let doc = sample("print_sample.pdf");
    let found = doc.marked_content_boxes(0).expect("the page interprets");
    assert_eq!(found.len(), 8, "print_sample page 1 reported {found:?}");
    let page = doc.get_page_box(0).expect("the page has a box");
    for (mcid, rect) in &found {
        assert!(
            rect.x0 >= page.x1 && rect.y0 >= page.y1 && rect.x1 <= page.x2 && rect.y1 <= page.y2,
            "/MCID {mcid} landed at {rect:?}, outside {page:?}"
        );
    }
}

#[test]
fn the_first_mark_sits_above_the_second_on_a_page_read_downwards() {
    // The overlay this was built for draws reading order, so the order has to survive the
    // measurement: `print_sample.pdf`'s heading is `/MCID 0` and the paragraph under it
    // is `/MCID 1`, and in default user space "under" means a smaller y.
    let found = sample("print_sample.pdf").marked_content_boxes(0).expect("the page interprets");
    let heading = found.get(&0).expect("the heading drew");
    let paragraph = found.get(&1).expect("the paragraph drew");
    assert!(
        heading.y0 > paragraph.y1,
        "the heading at {heading:?} is not above the paragraph at {paragraph:?}"
    );
}

#[test]
fn an_untagged_sample_comes_back_with_none() {
    // `constitution.pdf` carries no `/MCID` anywhere, so there is nothing to measure and
    // the answer is empty rather than absent. This is the shape 6 of the 9 samples have.
    let found = sample("constitution.pdf").marked_content_boxes(0).expect("the page interprets");
    assert!(found.is_empty(), "an untagged page reported {found:?}");
}
