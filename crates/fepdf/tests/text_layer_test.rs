//! A text layer handed back by an OCR engine, laid over the page it read (ROADMAP W-O1).
//!
//! **The entry's two checks, as it states them**: extraction returns the layer, and the
//! page's CPU rasterisation does not change by one byte.

use fepdf::{IngestionOptions, Operation, PdfDocument, Rasteriser, TextLayerItem};

/// A "scan": a 300 by 200 page that draws only grey bars where words would be, and
/// leaves the fill black after them.
///
/// **Black, because grey made the check vacuous.** The layer inherits the page's fill,
/// and drawn visibly in the bars' own grey over the bars it changed no pixel — so the
/// check passed with the text in mode 0.
fn scan() -> PdfDocument {
    let content = "0.6 g 20 150 120 14 re f 20 110 80 14 re f 0 g";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

fn items() -> Vec<TextLayerItem> {
    vec![
        TextLayerItem { text: "Invoice 2026".into(), rect: [20.0, 150.0, 140.0, 164.0] },
        TextLayerItem { text: "日本国憲法".into(), rect: [20.0, 110.0, 100.0, 124.0] },
    ]
}

fn raster(doc: &PdfDocument) -> Vec<u8> {
    let path = std::env::temp_dir().join(format!(
        "fepdf_text_layer_{}_{:?}.png",
        std::process::id(),
        std::thread::current().id()
    ));
    doc.render_page_to_file_with(0, &path, Rasteriser::Cpu).expect("the page renders");
    let pixels = image::open(&path).expect("what was written reads").to_rgba8().into_raw();
    let _ = std::fs::remove_file(&path);
    pixels
}

fn layered() -> PdfDocument {
    let mut doc = scan();
    doc.apply(Operation::AddTextLayer { page: 0, items: items() }).expect("the layer is laid");
    doc
}

/// **Extraction returns the layer**, through a save.
#[test]
fn extraction_returns_the_layer() {
    let doc = layered();
    let path = std::env::temp_dir().join(format!("fepdf_text_layer_{}.pdf", std::process::id()));
    doc.save_as_version(&path, "2.0").expect("it saves");
    let bytes = std::fs::read(&path).expect("it reads");
    let _ = std::fs::remove_file(&path);
    let text = PdfDocument::open(bytes.into()).expect("it reopens").extract_text(0).expect("text");
    assert!(text.contains("Invoice 2026"), "{text:?}");
    assert!(text.contains("日本国憲法"), "{text:?}");
}

/// **The page looks exactly as it did** — not one byte of its CPU rasterisation moves.
#[test]
fn the_page_does_not_change_by_one_byte() {
    assert!(raster(&scan()) == raster(&layered()), "the layer can be seen");
}

/// **Each word lands on the box it was read from**, so a selection covers the word a
/// reader sees.
#[test]
fn each_word_lands_on_its_box() {
    let spans = layered().extract_spans(0).expect("spans");
    for item in items() {
        let span = spans
            .iter()
            .find(|s| s.text.contains(item.text.split(' ').next().unwrap_or_default()))
            .unwrap_or_else(|| panic!("{:?} was not found among {spans:?}", item.text));
        let [x0, _, x1, _] = item.rect;
        let covered: f64 = spans
            .iter()
            .filter(|s| (s.y - span.y).abs() < 0.5)
            .map(|s| s.x + s.width)
            .fold(span.x, f64::max);
        assert!(
            (span.x - x0).abs() < 0.5,
            "{:?} starts at {} and its box at {x0}",
            item.text,
            span.x
        );
        assert!(
            (covered - x1).abs() < 1.0,
            "{:?} ends at {covered} and its box at {x1}",
            item.text
        );
    }
}

/// What is not a word in a box is refused, naming why.
#[test]
fn what_is_not_a_word_in_a_box_is_refused() {
    let mut doc = scan();
    for (items, said) in [
        (vec![], "nothing"),
        (
            vec![TextLayerItem { text: "two\nlines".into(), rect: [0.0, 0.0, 10.0, 10.0] }],
            "one line",
        ),
        (vec![TextLayerItem { text: "flat".into(), rect: [0.0, 5.0, 10.0, 5.0] }], "box"),
    ] {
        let refused = doc.apply(Operation::AddTextLayer { page: 0, items }).expect_err("refused");
        assert!(refused.to_string().contains(said), "{refused}");
    }
    let refused =
        doc.apply(Operation::AddTextLayer { page: 4, items: items() }).expect_err("no page");
    assert!(refused.to_string().contains("no page 5"), "{refused}");
}

/// **A box read off the picture lands where the picture showed it**, on a page turned by
/// `/Rotate` as well as an upright one.
#[test]
#[allow(clippy::float_cmp)] // halves and whole numbers, which f64 holds exactly
fn a_pixel_box_is_turned_back_into_points() {
    let upright = scan();
    let (to_pixels, width, height) = upright.page_to_pixels(0, 144.0).expect("a transform");
    assert_eq!((width, height), (600, 400));
    let back = to_pixels.inverse().as_coeffs();
    // The 20..140 by 150..164 point bar is 40..280 by 72..100 pixels, y running down.
    assert_eq!(
        fepdf::pixel_box_on_page([40.0, 72.0, 280.0, 100.0], back),
        [20.0, 150.0, 140.0, 164.0]
    );

    // Turned a quarter: the same bar, found where the turned picture shows it, comes back
    // as the same box on the page.
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Rotate 90 >>",
    ];
    let turned = PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("it opens");
    let (to_pixels, width, height) = turned.page_to_pixels(0, 144.0).expect("a transform");
    assert_eq!((width, height), (400, 600), "a turned page is a turned picture");
    let corners = [(20.0, 150.0), (140.0, 164.0)].map(|(x, y)| to_pixels * kurbo::Point::new(x, y));
    let seen = [
        corners[0].x.min(corners[1].x),
        corners[0].y.min(corners[1].y),
        corners[0].x.max(corners[1].x),
        corners[0].y.max(corners[1].y),
    ];
    let found = fepdf::pixel_box_on_page(seen, to_pixels.inverse().as_coeffs());
    for (got, want) in found.iter().zip([20.0, 150.0, 140.0, 164.0]) {
        assert!((got - want).abs() < 1e-9, "{found:?}");
    }
}

/// **A turned page is turned, not reflected**, and its box is drawn from its own corner.
///
/// `a_pixel_box_is_turned_back_into_points` maps a box through the transform and back
/// through its inverse, which any invertible transform passes — a mirror image included.
/// This asks where each corner of the page lands, which only the right one answers:
/// turned a quarter clockwise, the page's bottom left is the picture's top left and its
/// top left is the picture's top right. `/Rotate 90` and `270` drew the page mirrored
/// until 2026-09-29, and the box's origin was taken to be `(0, 0)` whatever it was.
#[test]
#[allow(clippy::cast_possible_truncation)] // pixels of a 300-point page, rounded
fn each_corner_of_a_turned_page_lands_where_turning_puts_it() {
    let page = |entries: &str| {
        let bodies = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            format!("<< /Type /Page /Parent 2 0 R {entries} >>"),
        ];
        PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("it opens")
    };
    // 72 DPI, so a point is a pixel. The box is 300 by 200 and starts at (50, 20).
    let corners = |rotate: u32| {
        let doc = page(&format!("/MediaBox [50 20 350 220] /Rotate {rotate}"));
        let (t, w, h) = doc.page_to_pixels(0, 72.0).expect("a transform");
        let at = |x: f64, y: f64| {
            let p = t * kurbo::Point::new(x, y);
            (p.x.round() as i64, p.y.round() as i64)
        };
        ((w, h), at(50.0, 20.0), at(50.0, 220.0), at(350.0, 20.0))
    };
    // (size, bottom-left, top-left, bottom-right) of the page, in pixels.
    assert_eq!(corners(0), ((300, 200), (0, 200), (0, 0), (300, 200)));
    assert_eq!(corners(90), ((200, 300), (0, 0), (200, 0), (0, 300)));
    assert_eq!(corners(180), ((300, 200), (300, 0), (300, 200), (0, 0)));
    assert_eq!(corners(270), ((200, 300), (200, 300), (0, 300), (200, 0)));
}
