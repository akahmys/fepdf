//! **An annotation that arrives without an appearance is given one** (ADR-0119, ROADMAP
//! AA-4b): each of the seventeen subtypes XFDF admits and Table 166 does not exempt,
//! imported through FDF without `/AP`, comes out with one that draws inside its rectangle.

use fepdf::{AnnotationKind, AnnotationSpec, Operation, PdfDocument};
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;

fn blank() -> PdfDocument {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> >>",
    ];
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

/// An FDF holding one annotation whose entries are `body`, on page 0.
fn fdf(body: &str) -> Vec<u8> {
    format!(
        "%FDF-1.2\n1 0 obj\n<< /FDF << /Annots [2 0 R] >> >>\nendobj\n2 0 obj\n<< /Type /Annot /Page 0 {body} >>\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n"
    )
    .into_bytes()
}

/// Each subtype, with what it draws from.
const CASES: &[(&str, &str)] = &[
    ("Text", "/Subtype /Text /Rect [20 20 44 44] /Name /Comment"),
    (
        "FreeText",
        "/Subtype /FreeText /Rect [20 20 180 60] /DA (/Helv 12 Tf 0 0 1 rg) /Contents (Hello) /Q 1",
    ),
    (
        "Line",
        "/Subtype /Line /Rect [10 10 190 60] /L [20 20 180 50] /LE [/OpenArrow /ClosedArrow] /IC [1 0 0] /LL 5 /LLE 2",
    ),
    ("Square", "/Subtype /Square /Rect [20 20 120 80] /C [0 0 1] /IC [1 1 0] /BS << /W 2 >>"),
    ("Circle", "/Subtype /Circle /Rect [20 20 120 80] /C [0 0 1] /BS << /W 2 /S /D /D [4 2] >>"),
    (
        "Polygon",
        "/Subtype /Polygon /Rect [10 10 190 190] /Vertices [20 20 180 20 100 180] /IC [0 1 0] /C [0 0 0]",
    ),
    (
        "PolyLine",
        "/Subtype /PolyLine /Rect [10 10 190 190] /Vertices [20 20 100 150 180 20] /LE [/Circle /Square] /C [1 0 0]",
    ),
    (
        "Highlight",
        "/Subtype /Highlight /Rect [20 20 120 40] /QuadPoints [20 40 120 40 20 20 120 20] /C [1 1 0]",
    ),
    (
        "Underline",
        "/Subtype /Underline /Rect [20 20 120 40] /QuadPoints [20 20 120 20 120 40 20 40] /C [0 0 1]",
    ),
    (
        "StrikeOut",
        "/Subtype /StrikeOut /Rect [20 20 120 40] /QuadPoints [20 40 120 40 20 20 120 20]",
    ),
    (
        "Squiggly",
        "/Subtype /Squiggly /Rect [20 20 120 40] /QuadPoints [20 40 120 40 20 20 120 20] /C [0 0.5 0]",
    ),
    ("Caret", "/Subtype /Caret /Rect [20 20 50 40] /Sy /P /C [0 0 1]"),
    ("Stamp", "/Subtype /Stamp /Rect [20 20 180 70] /Name /Approved"),
    (
        "Ink",
        "/Subtype /Ink /Rect [10 10 120 100] /InkList [[20 20 60 80 100 30]] /C [0 0 0] /BS << /W 3 >>",
    ),
    ("FileAttachment", "/Subtype /FileAttachment /Rect [20 20 40 40] /Name /Paperclip"),
    ("Sound", "/Subtype /Sound /Rect [20 20 40 40] /Name /Mic"),
    ("Redact", "/Subtype /Redact /Rect [20 20 120 40] /QuadPoints [20 40 120 40 20 20 120 20]"),
];

/// What the page paints, as fills and strokes counted.
fn painted(doc: &PdfDocument) -> (usize, usize, Vec<kurbo::Rect>) {
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    (recorder.count("fill"), recorder.count("stroke"), recorder.device_fills())
}

#[test]
fn every_subtype_is_given_an_appearance_that_draws() {
    let mut missing = Vec::new();
    for (subtype, body) in CASES {
        let mut doc = blank();
        doc.apply(Operation::ImportFdf { fdf: fdf(body) }).expect("it imports");
        let (fills, strokes, areas) = painted(&doc);
        if fills + strokes == 0 {
            missing.push(format!("{subtype}: nothing drawn"));
        }
        // Inside the page's 200 points, where every case's rectangle is.
        if let Some(out) =
            areas.iter().find(|r| r.x0 < -1.0 || r.y0 < -1.0 || r.x1 > 201.0 || r.y1 > 201.0)
        {
            missing.push(format!("{subtype}: drew outside the page at {out:?}"));
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

/// The blank page draws nothing, so what the cases draw is theirs.
#[test]
fn the_page_draws_nothing_by_itself() {
    let (fills, strokes, _) = painted(&blank());
    assert_eq!(fills + strokes, 0);
}

/// **A stamp `AddAnnotation` makes without a picture has an appearance now**: its name in
/// a frame, where it had `/Name /Draft` and nothing to draw.
#[test]
fn a_stamp_without_a_picture_is_drawn() {
    let mut doc = blank();
    doc.apply(Operation::AddAnnotation(AnnotationSpec {
        page: 0,
        rect: [20.0, 20.0, 180.0, 70.0],
        kind: AnnotationKind::Stamp { stamp_image_bytes: Vec::new() },
        by: fepdf::Authorship::default(),
    }))
    .expect("it adds");
    let (fills, strokes, _) = painted(&doc);
    assert!(fills + strokes > 0, "the stamp draws nothing");
}

/// A highlight covers what it marks, as the one `AddAnnotation` draws does.
#[test]
fn an_imported_highlight_covers_its_quadrilateral() {
    let mut doc = blank();
    let (_, body) = CASES.iter().find(|(s, _)| *s == "Highlight").expect("the case");
    doc.apply(Operation::ImportFdf { fdf: fdf(body) }).expect("it imports");
    let (_, _, areas) = painted(&doc);
    let covers = areas
        .iter()
        .any(|r| r.x0 >= 19.0 && r.x1 <= 121.0 && r.width() >= 90.0 && r.height() >= 18.0);
    assert!(covers, "{areas:?}");
}
