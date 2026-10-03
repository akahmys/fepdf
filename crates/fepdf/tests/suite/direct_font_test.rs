//! A font dictionary written direct inside the resources (7.3.10).
//!
//! **Conforming, and read as missing.** Every route that found a font keyed it by object
//! number, so `/Font << /F1 << /Type /Font … >> >>` was found by none of them: ingestion
//! recorded a 9.6.2 repair saying the resources did not define `/F1`, and the text was
//! drawn in a substitute sans. Five files of the corpus write one, all in
//! `pdf-differences` (`cargo run --release --example direct_fonts`). ROADMAP W-E2c.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_fixtures::recorder::{Event, Recorder};
use kurbo::Affine;

/// A page drawing `Hello` in Times-Bold, the font written into the resources direct.
fn page(options: &IngestionOptions) -> PdfDocument {
    let content = "BT /F1 24 Tf 1 0 0 1 72 700 Tm (Hello) Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
         /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Times-Bold >> \
         >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    PdfDocument::open_with_options(fepdf_fixtures::assemble(&bodies).into(), options)
        .expect("the fixture opens")
}

fn refined() -> IngestionOptions {
    IngestionOptions::default()
}

fn raw() -> IngestionOptions {
    IngestionOptions { active_refinement: false, ..IngestionOptions::default() }
}

/// **The entry's own test: nothing is repaired, on either path.**
#[test]
fn a_direct_font_is_not_reported_missing() {
    for (path, options) in [("refined", refined()), ("raw", raw())] {
        let doc = page(&options);
        let mut recorder = Recorder::new();
        doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
        let repairs: Vec<_> =
            doc.decisions().into_iter().filter(|d| d.clause.starts_with("9.6")).collect();
        assert!(
            repairs.is_empty(),
            "{path}: a font the resources define was repaired: {repairs:?}"
        );
    }
}

/// **And the face drawn is the one the file names**, not the substitute a missing font
/// gets. What was wrong is visible here and not only in the log: Times-Bold came out sans.
#[test]
fn the_face_drawn_is_the_one_the_file_names() {
    for (path, options) in [("refined", refined()), ("raw", raw())] {
        let doc = page(&options);
        let mut recorder = Recorder::new();
        doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
        let bases: Vec<Option<String>> = recorder
            .events
            .iter()
            .filter_map(
                |e| if let Event::Font { base, .. } = e { Some(base.clone()) } else { None },
            )
            .collect();
        assert_eq!(
            bases,
            vec![Some("Times-Bold".to_string())],
            "{path}: the page drew with {bases:?}"
        );
        assert_eq!(recorder.text(), "Hello", "{path}: the page does not read Hello");
    }
}

/// The runs find it too: `runs_of_page` reads a run only in a font it can resolve.
#[test]
fn the_runs_read_text_set_in_a_direct_font() {
    let doc = page(&refined());
    let runs = fepdf::text::runs_of_page(doc.inner(), 0).expect("it lists");
    assert_eq!(runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), vec!["Hello"]);
}

/// **Drawing a page does not grow the document.** The interpreter gave a direct font a
/// fresh object each time it resolved one, so every draw of the page added to the arena
/// and loaded the font again.
#[test]
fn drawing_the_page_again_adds_no_objects() {
    let doc = page(&raw());
    let draw = || {
        let mut recorder = Recorder::new();
        doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    };
    draw();
    let after_one = doc.inner().arena().object_count();
    draw();
    draw();
    assert_eq!(doc.inner().arena().object_count(), after_one, "drawing the page allocated");
}
