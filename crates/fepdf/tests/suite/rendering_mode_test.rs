//! The text rendering mode, drawn as Table 106 says (9.3.6).
//!
//! **The renderer stored the mode and never read it**, so every mode was a fill: text in
//! mode 3 — the text layer of every OCR'd scan — was drawn over the scan it was read from.

use fepdf::{IngestionOptions, PdfDocument, Rasteriser};

/// A 200 by 100 page showing "HHH" large in `mode`, black fill and black stroke.
fn page(mode: Option<u8>) -> PdfDocument {
    let text = mode
        .map_or_else(String::new, |mode| format!("BT /F1 60 Tf {mode} Tr 10 20 Td (HHH) Tj ET"));
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{text}\nendstream", text.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens")
}

/// How many pixels are dark.
fn inked(doc: &PdfDocument) -> usize {
    let path = std::env::temp_dir().join(format!(
        "fepdf_mode_{}_{:?}.png",
        std::process::id(),
        std::thread::current().id()
    ));
    doc.render_page_to_file_with(0, &path, Rasteriser::Cpu).expect("the page renders");
    let pixels = image::open(&path).expect("it reads").to_rgba8();
    let _ = std::fs::remove_file(&path);
    pixels.pixels().filter(|p| p.0[0] < 128).count()
}

/// **Mode 3 draws nothing, and mode 7 draws nothing either.**
#[test]
fn the_invisible_modes_draw_nothing() {
    assert!(inked(&page(Some(0))) > 500, "the fill drew nothing; the check proves nothing");
    assert_eq!(inked(&page(Some(3))), inked(&page(None)), "mode 3 was drawn");
    assert_eq!(inked(&page(Some(7))), inked(&page(None)), "mode 7 was drawn");
}

/// **A stroke is an outline**: it inks some of what the fill inks, and not all of it.
#[test]
fn a_stroke_is_less_than_a_fill() {
    let (fill, stroke) = (inked(&page(Some(0))), inked(&page(Some(1))));
    assert!(stroke > 50 && stroke < fill, "stroke {stroke} against fill {fill}");
}
