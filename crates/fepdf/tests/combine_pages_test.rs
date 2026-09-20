//! Several pages put onto one sheet.
//!
//! **Each source page becomes a form XObject and is drawn into a cell.** A form XObject
//! carries its own resources (8.10), so a page brought onto another sheet keeps the fonts
//! and images it names without those having to be merged into anything — which is what
//! makes this an arrangement rather than a rewrite.

use fepdf::{IngestionOptions, Operation, PageArrangement, PageSelection, PdfDocument};
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;
use std::collections::BTreeMap;

fn opened(name: &str) -> PdfDocument {
    let bytes = std::fs::read(format!("../../samples/{name}")).expect("the sample is there");
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens")
}

/// **Four pages become one, and every glyph of all four is drawn on it.**
///
/// What a form XObject has to carry across is the resources the drawing names: a page
/// drawn through one whose resources did not come would draw nothing, or draw the wrong
/// glyphs, and the sheet would look plausible and be empty.
///
/// The comparison is of *how many of each character* is drawn, not of the text as a
/// string. Pages side by side on one sheet are read across it, so the top-left page's
/// second line follows the top-right page's first — and a run of this sample is one or
/// two characters, so even a short substring of one page is interrupted by the other.
#[test]
fn four_pages_become_one_that_draws_all_four() {
    let tally = |doc: &PdfDocument, page: usize| {
        let mut recorder = Recorder::new();
        doc.render_page(page, &mut recorder, Affine::IDENTITY).expect("the page interprets");
        let mut seen: BTreeMap<char, usize> = BTreeMap::new();
        for character in recorder.text().chars().filter(|c| !c.is_whitespace()) {
            *seen.entry(character).or_default() += 1;
        }
        seen
    };

    let doc = opened("print_sample.pdf");
    let mut wanted: BTreeMap<char, usize> = BTreeMap::new();
    for page in 0..4 {
        for (character, count) in tally(&doc, page) {
            *wanted.entry(character).or_default() += count;
        }
    }
    assert!(wanted.len() > 20, "the four pages draw too little for this to measure anything");
    let before = doc.page_count().expect("it counts");

    let mut doc = opened("print_sample.pdf");
    doc.apply(Operation::CombinePages(
        PageSelection::Indices(vec![0, 1, 2, 3]),
        PageArrangement { sheet: None, columns: 2, rows: 2 },
    ))
    .expect("the arrangement applies");

    assert_eq!(doc.page_count().expect("it counts"), before - 3, "four pages did not become one");
    assert_eq!(tally(&doc, 0), wanted, "the sheet does not draw what the four pages drew");
}

/// **The pages fill the grid in reading order**, across a row and then down — the same
/// order a split cuts one up in, so that cutting a sheet into four and putting four onto
/// a sheet are the two directions of one thing.
#[test]
fn pages_fill_the_grid_in_reading_order() {
    let lettered = |letter: &str| {
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Contents {{}} 0 R \
             /Resources << /Font << /F1 9 0 R >> >> >>|BT /F1 40 Tf 1 0 0 1 100 150 Tm \
             ({letter}) Tj ET"
        )
    };
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R 7 0 R] /Count 3 >>".to_string(),
    ];
    for (nth, letter) in ["A", "B", "C"].iter().enumerate() {
        let spec = lettered(letter);
        let (page, content) = spec.split_once('|').expect("the fixture is two halves");
        bodies.push(page.replace("{}", &(4 + nth * 2).to_string()));
        bodies.push(format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()));
    }
    bodies.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
    let mut doc = PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens");
    assert_eq!(doc.page_count().expect("it counts"), 3, "the fixture is not three pages");

    doc.apply(Operation::CombinePages(
        PageSelection::All,
        PageArrangement { sheet: Some((600.0, 600.0)), columns: 2, rows: 2 },
    ))
    .expect("the arrangement applies");

    let spans = doc.extract_spans(0).expect("it extracts");
    let placed: Vec<(String, f64, f64)> =
        spans.iter().map(|s| (s.text.trim().to_string(), s.x, s.y)).collect();
    let at = |letter: &str| {
        placed.iter().find(|(text, _, _)| text == letter).map_or_else(
            || panic!("the sheet does not draw {letter}: {placed:?}"),
            |(_, x, y)| (*x, *y),
        )
    };
    let (a, b, c) = (at("A"), at("B"), at("C"));
    assert!(a.0 < b.0, "A is not to the left of B: {a:?} and {b:?}");
    assert!((a.1 - b.1).abs() < 1.0, "A and B are not on the same row: {a:?} and {b:?}");
    assert!(c.1 < a.1, "C is not below A: {c:?} and {a:?}");
}

/// More pages than cells make more sheets.
#[test]
fn more_pages_than_cells_make_more_sheets() {
    let mut doc = opened("print_sample.pdf");
    let before = doc.page_count().expect("it counts");

    doc.apply(Operation::CombinePages(
        PageSelection::Indices((0..6).collect()),
        PageArrangement { sheet: None, columns: 2, rows: 2 },
    ))
    .expect("the arrangement applies");

    // Six pages at four to a sheet is two sheets, so six came out and two went in.
    assert_eq!(doc.page_count().expect("it counts"), before - 4, "the sheets do not add up");
}

/// A grid with no cells is refused, rather than making a sheet nothing is drawn on.
#[test]
fn a_grid_with_no_cells_is_refused() {
    let mut doc = opened("print_sample.pdf");
    let error = doc
        .apply(Operation::CombinePages(
            PageSelection::All,
            PageArrangement { sheet: None, columns: 0, rows: 2 },
        ))
        .expect_err("it refuses");
    assert!(error.to_string().contains("no cells"), "the refusal does not say why: {error}");
}

/// **A page keeps its shape in a cell that is not its shape.**
///
/// It is scaled by the smaller of the two ratios and centred, so a tall page in a square
/// cell stands in the middle of it rather than being stretched to fill it — and, more to
/// the point, rather than running over into the cell below.
///
/// The fixture is 200 by 400 going into cells of 300 by 300: fitting asks for 0.75 and
/// the width alone would ask for 1.5, which would draw the page 600 tall in a cell 300
/// tall.
#[test]
fn a_tall_page_in_a_square_cell_stays_inside_it() {
    let content = "BT /F1 30 Tf 1 0 0 1 20 360 Tm (X) Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 400] /Contents 4 0 R \
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

    doc.apply(Operation::CombinePages(
        PageSelection::All,
        PageArrangement { sheet: Some((600.0, 600.0)), columns: 2, rows: 2 },
    ))
    .expect("the arrangement applies");

    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    let drawn = recorder.device_text_origins();
    assert_eq!(drawn.len(), 1, "the sheet does not draw the one letter the page drew");
    let at = drawn[0];
    assert!(
        at.1 >= 300.0 && at.1 <= 600.0 && at.0 >= 0.0 && at.0 <= 300.0,
        "the letter is at {at:?}, outside the top-left cell it belongs in"
    );
}
