//! What an annotation this engine writes looks like on the page, rasterised.
//!
//! **Asked of the pixels, because the dictionary can be right and the page wrong.** A
//! highlight carried its `/QuadPoints`, `/C` and appearance and still hid the words it
//! marked: its appearance was a rectangle in normal blending, and when it was given
//! `/BM /Multiply` the renderer recorded the mode and composited normally anyway. Measured
//! on `constitution.pdf`, the title under a yellow highlight went from 449 dark pixels to
//! none (ROADMAP W-13).

use fepdf::{AnnotationKind, AnnotationSpec, IngestionOptions, Operation, PdfDocument, Rasteriser};

/// The page's pixels, rasterised on the CPU, with its size in points.
fn raster(doc: &PdfDocument) -> (image::GrayImage, f64) {
    let path = std::env::temp_dir().join(format!(
        "fepdf_annotation_drawing_{}_{:?}.png",
        std::process::id(),
        std::thread::current().id()
    ));
    doc.render_page_to_file_with(0, &path, Rasteriser::Cpu).expect("the page renders");
    let pixels = image::open(&path).expect("what was written reads").to_luma8();
    let _ = std::fs::remove_file(&path);
    (pixels, doc.get_page_size(0).expect("a size").1)
}

/// How many pixels inside `rect` are darker than `below`.
fn darker_than(pixels: &image::GrayImage, page_height: f64, rect: [f32; 4], below: u8) -> usize {
    let scale = 4.0 / 3.0;
    let to = |value: f64| pixel(value * scale);
    let (left, right) = (to(f64::from(rect[0])), to(f64::from(rect[2])));
    let (top, bottom) =
        (to(page_height - f64::from(rect[3])), to(page_height - f64::from(rect[1])));
    let mut count = 0;
    for y in top..bottom.min(pixels.height()) {
        for x in left..right.min(pixels.width()) {
            count += usize::from(pixels.get_pixel(x, y).0[0] < below);
        }
    }
    count
}

/// A position in pixels, as an index. Bounded: nothing here is wider than a page.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn pixel(value: f64) -> u32 {
    value.round().clamp(0.0, 100_000.0) as u32
}

/// A position in points, as the annotation spec holds one. A page's coordinates fit.
#[allow(clippy::cast_possible_truncation)]
fn point(value: f64) -> f32 {
    value as f32
}

fn annotate(doc: &mut PdfDocument, rect: [f32; 4], kind: AnnotationKind) {
    doc.apply(Operation::AddAnnotation(AnnotationSpec { page: 0, rect, kind }))
        .expect("the annotation applies");
}

/// **A highlight leaves the words it marks readable.**
#[test]
fn a_highlight_leaves_the_text_it_marks_visible() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/constitution.pdf");
    let bytes = std::fs::read(path).expect("samples/constitution.pdf is in this working copy");
    let mut doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens");

    let runs = fepdf::text::runs_of_page(doc.inner(), 0).expect("it lists");
    let found = &fepdf::text::find_on_page(doc.inner(), 0, "日本国憲法").expect("it searches")[0];
    let (first, last) = (&found.runs[0], found.runs.last().expect("a run"));
    let start = runs[first.run].corners(first.from, first.to).expect("the first run's box");
    let end = runs[last.run].corners(last.from, last.to).expect("the last run's box");
    let rect = [point(start[0].0), point(start[0].1), point(end[2].0), point(end[2].1)];

    let (before, height) = raster(&doc);
    let dark = darker_than(&before, height, rect, 100);
    assert!(dark > 100, "the title is not where this looks for it: {dark} dark pixels");

    annotate(&mut doc, rect, AnnotationKind::Highlight { color_rgb: [1.0, 1.0, 0.0] });
    let (after, _) = raster(&doc);
    let still = darker_than(&after, height, rect, 100);
    assert!(
        still * 10 >= dark * 9,
        "the highlight hid the words it marks: {dark} dark pixels before, {still} after"
    );
}

/// **Every kind that draws, draws inside its rectangle**, on a page that is otherwise
/// white — the text kinds in Japanese, set in a face this engine embeds.
#[test]
fn every_kind_draws_on_the_page() {
    let rect = [100.0, 500.0, 300.0, 560.0];
    let kinds = [
        ("note", AnnotationKind::TextComment { contents: "メモ".into() }),
        ("stamp", AnnotationKind::Stamp { stamp_image_bytes: fepdf_fixtures::red_jpeg() }),
        ("highlight", AnnotationKind::Highlight { color_rgb: [1.0, 0.8, 0.0] }),
        ("underline", AnnotationKind::Underline { color_rgb: [0.0, 0.0, 1.0] }),
        ("strike-out", AnnotationKind::StrikeOut { color_rgb: [1.0, 0.0, 0.0] }),
        ("squiggly", AnnotationKind::Squiggly { color_rgb: [0.0, 0.5, 0.0] }),
        ("text box", AnnotationKind::TextBox { contents: "確認済み".into(), font_size: 14.0 }),
        ("typewriter", AnnotationKind::Typewriter { contents: "記入例".into(), font_size: 14.0 }),
        (
            "callout",
            AnnotationKind::Callout {
                contents: "ここ".into(),
                font_size: 14.0,
                points_at: [80.0, 450.0],
            },
        ),
        (
            "ink",
            AnnotationKind::Ink {
                strokes: vec![vec![[110.0, 510.0], [200.0, 550.0], [290.0, 510.0]]],
                color_rgb: [0.0, 0.0, 0.0],
                width: 3.0,
            },
        ),
        (
            "rectangle",
            AnnotationKind::Shape {
                form: fepdf::ShapeForm::Rectangle,
                color_rgb: [0.0, 0.0, 0.0],
                width: 2.0,
            },
        ),
        (
            "line",
            AnnotationKind::Shape {
                form: fepdf::ShapeForm::Line { from: [110.0, 510.0], to: [290.0, 550.0] },
                color_rgb: [0.0, 0.0, 0.0],
                width: 2.0,
            },
        ),
    ];
    for (name, kind) in kinds {
        let mut doc = PdfDocument::open_with_options(
            fepdf_fixtures::assemble(&[
                "<< /Type /Catalog /Pages 2 0 R >>",
                "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            ])
            .into(),
            &IngestionOptions::default(),
        )
        .expect("the fixture opens");
        annotate(&mut doc, rect, kind);
        let (pixels, height) = raster(&doc);
        let marked = darker_than(&pixels, height, rect, 250);
        assert!(marked > 20, "{name} drew nothing inside its rectangle: {marked} pixels");
    }
}
