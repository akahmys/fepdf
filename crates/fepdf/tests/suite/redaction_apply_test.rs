//! **A document's own redaction annotations, applied** (12.5.6.23, Table 195, ROADMAP
//! Y-10): what each marks goes, the annotation goes, and its place is drawn as it says.

use fepdf::{Operation, PageSelection, PdfDocument};
use fepdf_fixtures::recorder::{Event, Recorder};
use kurbo::{Affine, Shape};

/// A page with two lines, AAA at y 700 and BBB at y 600, the given redaction annotation
/// as object 6, and `extra` from object 7; `catalog` is added to the catalogue.
fn page_marked(annotation: &str, catalog: &str, extra: &[&str]) -> PdfDocument {
    let content = "BT /F1 24 Tf 72 700 Td (AAA) Tj ET BT /F1 24 Tf 72 600 Td (BBB) Tj ET";
    let mut bodies = vec![
        format!("<< /Type /Catalog /Pages 2 0 R {catalog} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
           /Resources << /Font << /F1 5 0 R >> >> /Annots [6 0 R] >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Type /Annot /Subtype /Redact {annotation} >>"),
    ];
    bodies.extend(extra.iter().map(ToString::to_string));
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

fn apply(doc: &mut PdfDocument) {
    doc.apply(Operation::ApplyRedactAnnotations(PageSelection::All)).expect("it applies");
}

/// Every fill the page draws, on the page, with its colour in RGB.
fn fills(doc: &PdfDocument) -> Vec<(kurbo::Rect, (f64, f64, f64))> {
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("it renders");
    recorder
        .events
        .iter()
        .filter_map(|e| {
            let Event::Fill { path, ctm, color, .. } = e else { return None };
            let fepdf_content::Color::Rgb(r, g, b) = color.to_rgb() else { return None };
            Some(((*ctm * path.clone()).bounding_box(), (r, g, b)))
        })
        .collect()
}

/// How many annotations the page still has.
fn annotations(doc: &PdfDocument) -> usize {
    let arena = doc.inner().arena();
    let page =
        arena.get_object(doc.inner().pages[0]).and_then(|o| o.as_dict_handle()).expect("a page");
    arena
        .dict_entry(page, arena.name("Annots"))
        .and_then(|a| a.as_array())
        .and_then(|a| arena.get_array(a))
        .map_or(0, |a| a.len())
}

/// **`/IC` fills the region; the text under it goes, and so does the annotation.**
#[test]
fn an_interior_colour_fills_the_region() {
    let mut doc = page_marked("/Rect [60 690 300 730] /IC [1 0 0]", "", &[]);
    apply(&mut doc);
    let text = doc.extract_text(0).expect("it reads");
    assert!(!text.contains("AAA") && text.contains("BBB"), "{text:?}");
    assert_eq!(annotations(&doc), 0, "the redaction annotation is still on the page");
    let red = fills(&doc).into_iter().find(|(_, c)| (c.0 - 1.0).abs() < 1e-3 && c.1.abs() < 1e-3);
    assert_eq!(red.map(|(r, _)| (r.x0, r.y0, r.x1, r.y1)), Some((60.0, 690.0, 300.0, 730.0)));
    assert!(!doc.decisions().iter().any(|d| d.clause == "12.5.6.23"), "{:?}", doc.decisions());
}

/// **With no `/IC`, `/RO` or `/OverlayText`, the region is left transparent.**
#[test]
fn no_colour_leaves_the_region_transparent() {
    let mut doc = page_marked("/Rect [60 690 300 730]", "", &[]);
    apply(&mut doc);
    assert!(!doc.extract_text(0).expect("it reads").contains("AAA"));
    assert!(fills(&doc).is_empty(), "something was drawn over the region");
}

/// **`/QuadPoints` say what goes, not `/Rect`**: a rectangle round both lines with a
/// quadrilateral over the first takes the first.
#[test]
fn quadpoints_name_the_region() {
    let mut doc =
        page_marked("/Rect [60 590 300 730] /QuadPoints [60 730 300 730 60 690 300 690]", "", &[]);
    apply(&mut doc);
    let text = doc.extract_text(0).expect("it reads");
    assert!(!text.contains("AAA") && text.contains("BBB"), "{text:?}");
}

/// **`/RO` is drawn with its origin at the lower-left corner of `/Rect`**, and `/IC` is
/// not read.
#[test]
fn an_overlay_form_is_drawn_at_the_rectangle() {
    let ro = "<< /Type /XObject /Subtype /Form /BBox [0 0 50 20] /Length 16 >>\nstream\n0 0 50 20 re f\nendstream";
    let mut doc = page_marked("/Rect [60 690 300 730] /IC [1 0 0] /RO 7 0 R", "", &[ro]);
    apply(&mut doc);
    let drawn: Vec<_> =
        fills(&doc).into_iter().map(|(r, c)| ((r.x0, r.y0, r.x1, r.y1), c)).collect();
    assert_eq!(drawn, [((60.0, 690.0, 110.0, 710.0), (0.0, 0.0, 0.0))], "{drawn:?}");
}

/// **`/OverlayText` is set in the font `/DA` names from the form's `/DR`.**
#[test]
fn overlay_text_is_drawn() {
    let mut doc = page_marked(
        "/Rect [60 690 300 730] /OverlayText (WITHHELD) /DA (/Helv 12 Tf 0 g) /Q 1",
        "/AcroForm << /Fields [] /DR << /Font << /Helv 5 0 R >> >> >>",
        &[],
    );
    apply(&mut doc);
    let text = doc.extract_text(0).expect("it reads");
    assert!(text.contains("WITHHELD") && !text.contains("AAA"), "{text:?}");
}
