//! **A decoration goes where the page shows, reading the way the page reads** (ROADMAP
//! Y-F19): on the `/CropBox`, not the `/MediaBox`, and on a page turned by `/Rotate`, at
//! the named corner of the turned page with its words level there.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_doc::operation::{DecorationPosition, Operation, PageSelection};
use fepdf_fixtures::recorder::Recorder;
use kurbo::Affine;

/// Decorates a page with `page_entries` top left, and answers where the decoration's
/// text is drawn from and which way it runs.
fn decorated(page_entries: &str) -> ((f64, f64), (f64, f64)) {
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /Resources << >> {page_entries} >>"),
    ]);
    let mut doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("it opens");
    doc.apply(Operation::AddPageDecoration {
        pages: PageSelection::All,
        text: "HEADER".to_string(),
        position: DecorationPosition::TopLeft,
        layer: None,
    })
    .expect("the decoration applies");
    let mut marks = Recorder::new();
    doc.render_page(0, &mut marks, Affine::IDENTITY).expect("it renders");
    let matrix = *marks.device_text_matrices().first().expect("the decoration is drawn");
    let [a, b, _, _, e, f] = matrix.as_coeffs();
    let length = a.hypot(b);
    ((e, f), (a / length, b / length))
}

fn near(got: (f64, f64), want: (f64, f64)) -> bool {
    (got.0 - want.0).abs() < 0.01 && (got.1 - want.1).abs() < 0.01
}

/// A page neither cropped nor turned is decorated where it always was.
#[test]
fn a_plain_page_is_decorated_where_it_was() {
    let (origin, direction) = decorated("/MediaBox [0 0 600 800]");
    assert!(near(origin, (36.0, 764.0)) && near(direction, (1.0, 0.0)), "{origin:?} {direction:?}");
}

/// **Top left of what shows**: inside a smaller `/CropBox` it was placed on the media box,
/// outside what a viewer shows.
#[test]
fn a_cropped_page_is_decorated_inside_its_crop() {
    let (origin, _) = decorated("/MediaBox [0 0 600 800] /CropBox [100 100 500 700]");
    assert!(near(origin, (136.0, 664.0)), "{origin:?}");
}

/// **Top left of the page turned a quarter**, reading level there: in the page's own
/// space that is its lower-left corner, with the words running up it.
#[test]
fn a_page_turned_a_quarter_is_decorated_where_it_shows() {
    let (origin, direction) = decorated("/MediaBox [0 0 600 800] /Rotate 90");
    assert!(near(origin, (36.0, 36.0)) && near(direction, (0.0, 1.0)), "{origin:?} {direction:?}");
}

/// And turned three quarters: the upper-right corner, the words running down.
#[test]
fn a_page_turned_three_quarters_is_decorated_where_it_shows() {
    let (origin, direction) = decorated("/MediaBox [0 0 600 800] /Rotate 270");
    assert!(
        near(origin, (564.0, 764.0)) && near(direction, (0.0, -1.0)),
        "{origin:?} {direction:?}"
    );
}
