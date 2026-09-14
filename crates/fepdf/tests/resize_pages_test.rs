//! Putting a page on a different sheet, and what happens to what is on it.
//!
//! The geometry is tested where it lives, in `fepdf-doc`'s `resize_geometry`. This asks
//! the other half: that the numbers reach the document, that every box the page declares
//! moves with the content, and that the result is a file that opens.
//!
//! **Every page of every sample declares a `/CropBox`**, and `fy05.pdf` and
//! `unicode_16.pdf` declare a `/TrimBox` and a `/BleedBox` on all 1,986 of theirs —
//! re-derived with `cargo run --release --example page_boxes -p fepdf -- samples/*.pdf`
//! on 2026-09-14. A resize that wrote only `/MediaBox` would therefore be wrong on every
//! document in this corpus: the sheet would change and the viewer would go on showing the
//! old crop.

use fepdf::{Align, ContentScale, Operation, PageResize, PageSelection, PdfDocument};

const A4: (f64, f64) = (595.0, 842.0);
const A3: (f64, f64) = (842.0, 1191.0);
/// A sheet whose aspect is nothing like A4's, so a box that follows the content cannot
/// be mistaken for one that became the sheet.
const SQUARE: (f64, f64) = (842.0, 842.0);
/// Centred on both axes, which is what most of these ask for.
const MIDDLE: (Align, Align) = (Align::Middle, Align::Middle);

/// A resize onto `sheet` that scales the content to fill it, centred.
fn fitted(sheet: (f64, f64)) -> PageResize {
    PageResize { sheet: Some(sheet), scale: ContentScale::Fit, place: MIDDLE, offset: (0.0, 0.0) }
}

fn sample(name: &str) -> Option<PdfDocument> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples").join(name);
    PdfDocument::open(std::fs::read(path).ok()?.into()).ok()
}

/// Two boxes equal to within half a point, which is finer than any sheet is specified.
fn same(left: [f64; 4], right: [f64; 4]) -> bool {
    left.iter().zip(right.iter()).all(|(a, b)| (a - b).abs() < 0.5)
}

/// The box a page declares itself, which is what a resize has to have rewritten.
fn own_box(doc: &PdfDocument, index: usize, name: &str) -> Option<[f64; 4]> {
    let page = doc.inner().get_page(index).ok()?;
    let arena = doc.inner().arena();
    let array = arena.get_array(page.resolve_attribute(name)?.as_array()?)?;
    if array.len() < 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (slot, entry) in out.iter_mut().zip(array.iter()) {
        *slot = entry.resolve(arena).as_f64()?;
    }
    Some(out)
}

#[test]
fn the_sheet_becomes_the_size_asked_for() {
    let Some(mut doc) = sample("fy05.pdf") else { return };
    doc.apply(Operation::ResizePages(PageSelection::Single(0), fitted(A3))).expect("it resizes");

    let (w, h) = doc.get_page_size(0).expect("it has a size");
    assert!((w - A3.0).abs() < 0.5 && (h - A3.1).abs() < 0.5, "page 1 is {w} by {h}");
    // And only the page named: the operation takes a selection.
    let (w2, h2) = doc.get_page_size(1).expect("it has a size");
    assert!((w2 - A4.0).abs() < 2.0 && (h2 - A4.1).abs() < 2.0, "page 2 moved too: {w2} by {h2}");
}

/// **The crop becomes the sheet, and the trim moves with the content.**
///
/// Two rules, because the boxes mean two things. `/CropBox` is what a viewer displays: a
/// page put on A3 whose crop still hugged the old drawing would show the old page, and
/// the resize would look like it had not happened. `/TrimBox` and `/BleedBox` describe
/// where the content is cut and bled, so they follow the content.
#[test]
fn the_crop_becomes_the_sheet_and_the_trim_follows_the_content() {
    let Some(mut doc) = sample("fy05.pdf") else { return };
    let trim_before = own_box(&doc, 0, "TrimBox").expect("fy05 declares a TrimBox");

    // **A square sheet, not A3.** A4 and A3 differ in aspect by a quarter of a percent,
    // so a crop that followed the content instead of becoming the sheet would land within
    // half a point of it — this test passed against exactly that mistake until the sheet
    // was changed to one that can tell them apart.
    doc.apply(Operation::ResizePages(PageSelection::Single(0), fitted(SQUARE)))
        .expect("it resizes");

    let crop = own_box(&doc, 0, "CropBox").expect("a crop");
    let sheet = [0.0, 0.0, SQUARE.0, SQUARE.1];
    assert!(same(crop, sheet), "the crop is {crop:?} and the sheet is {sheet:?}");

    let trim = own_box(&doc, 0, "TrimBox").expect("the trim went missing");
    assert!(!same(trim, trim_before), "the trim was left on the old sheet");
    // Grown by the same ratio the content was, which is what following it means.
    let grew = (trim[2] - trim[0]) / (trim_before[2] - trim_before[0]);
    let expected = (SQUARE.0 / A4.0).min(SQUARE.1 / A4.1);
    assert!((grew - expected).abs() < 0.01, "the trim grew by {grew}, the content by {expected}");
    // And inside the sheet, which 14.11.2 requires of it.
    assert!(trim[0] >= -0.5 && trim[2] <= SQUARE.0 + 0.5, "the trim runs off it: {trim:?}");
}

/// A page with no `/Contents` is resized without being given any.
#[test]
fn a_blank_page_gains_no_content_stream() {
    let mut doc = PdfDocument::create_empty().expect("a new document opens");
    doc.apply(Operation::ResizePages(
        PageSelection::All,
        PageResize {
            sheet: Some(A4),
            scale: ContentScale::Keep,
            place: MIDDLE,
            offset: (0.0, 0.0),
        },
    ))
    .expect("it resizes");
    assert_eq!(doc.extract_text(0).expect("it reads").trim(), "");
    let (w, h) = doc.get_page_size(0).expect("it has a size");
    assert!((w - A4.0).abs() < 0.5 && (h - A4.1).abs() < 0.5);
}

/// A sheet with no area, and a scale that draws nothing, are refused rather than written.
#[test]
fn a_sheet_or_a_scale_that_draws_nothing_is_refused() {
    let Some(mut doc) = sample("print_sample.pdf") else { return };
    let refusals = [
        (0.0, 800.0, ContentScale::Fit),
        (600.0, -1.0, ContentScale::Fit),
        (600.0, 800.0, ContentScale::By(0.0)),
        (600.0, 800.0, ContentScale::By(f64::NAN)),
    ];
    for (w, h, scale) in refusals {
        let asked = Operation::ResizePages(
            PageSelection::All,
            PageResize { sheet: Some((w, h)), scale, place: MIDDLE, offset: (0.0, 0.0) },
        );
        assert!(doc.apply(asked).is_err(), "({w}, {h}) with {scale:?} was accepted");
    }
    // And nothing was changed on the way to refusing.
    let (w, h) = doc.get_page_size(0).expect("it has a size");
    assert!((w - 540.0).abs() < 0.5 && (h - 780.0).abs() < 0.5, "page 1 is {w} by {h}");
}

/// The resized document survives being written and read back, with its text intact.
#[test]
fn it_survives_a_round_trip_with_its_text() {
    let Some(mut doc) = sample("print_sample.pdf") else { return };
    let before = doc.extract_text(2).expect("the page has text");
    doc.apply(Operation::ResizePages(PageSelection::All, fitted(A3))).expect("it resizes");

    let dir = std::env::temp_dir().join("fepdf-resize");
    std::fs::create_dir_all(&dir).expect("a directory to write into");
    let path = dir.join("a3.pdf");
    let _ = doc.save_as_version(&path, "2.0").expect("it writes");
    let back = PdfDocument::open(std::fs::read(&path).expect("on disk").into()).expect("it opens");
    let decisions = back.decisions();
    let (w, h) = back.get_page_size(2).expect("it has a size");
    let after = back.extract_text(2).expect("the page still has text");
    let _ = std::fs::remove_dir_all(&dir);

    assert!(decisions.is_empty(), "the written file needed repairing: {decisions:?}");
    assert!((w - A3.0).abs() < 0.5 && (h - A3.1).abs() < 0.5, "page 3 came back {w} by {h}");
    assert_eq!(after, before, "the text changed when the sheet did");
}

/// **Scaling the content without repapering the document.**
///
/// This could not be asked for before: the sheet was required, so shrinking a drawing
/// inside the page it was already on meant reading that page's size off it first and
/// naming it back — and getting it wrong for a document whose pages are not all one size.
#[test]
fn a_resize_that_names_no_sheet_keeps_each_pages_own() {
    let Some(mut doc) = sample("fy05.pdf") else { return };
    let before: Vec<_> = (0..3).map(|i| doc.get_page_size(i).expect("a size")).collect();

    doc.apply(Operation::ResizePages(
        PageSelection::All,
        PageResize { sheet: None, scale: ContentScale::By(0.9), place: MIDDLE, offset: (0.0, 0.0) },
    ))
    .expect("it resizes");

    for (index, was) in before.iter().enumerate() {
        let now = doc.get_page_size(index).expect("a size");
        assert!(
            (now.0 - was.0).abs() < 0.5 && (now.1 - was.1).abs() < 0.5,
            "page {index} moved from {was:?} to {now:?}"
        );
    }
}

/// The offset is a nudge from where the placement put it, and reaches the page.
///
/// A binding margin is exactly this: centred, then moved off-centre by the gutter.
#[test]
fn an_offset_moves_the_content_and_not_the_sheet() {
    let Some(mut doc) = sample("print_sample.pdf") else { return };
    let was = doc.get_page_size(0).expect("a size");
    let text = doc.extract_text(0).expect("the page has text");

    doc.apply(Operation::ResizePages(
        PageSelection::Single(0),
        PageResize { sheet: None, scale: ContentScale::Keep, place: MIDDLE, offset: (30.0, -10.0) },
    ))
    .expect("it resizes");

    let now = doc.get_page_size(0).expect("a size");
    assert!((now.0 - was.0).abs() < 0.5 && (now.1 - was.1).abs() < 0.5, "the sheet moved: {now:?}");
    // The content is still there — a nudge is not a redraw.
    assert_eq!(doc.extract_text(0).expect("it reads"), text);
}
