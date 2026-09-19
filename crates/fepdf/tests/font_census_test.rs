//! How many fonts a document has, which is one number and not two.
//!
//! `inspect info` reported **24 fonts for `samples/constitution.pdf`, 72 for `fugaku.pdf`
//! and 14 for `print_sample.pdf`**, against 12, 36 and 7 with `--no-refinement`: exactly
//! twice, on every sample, from a tool whose business is telling people what is in their
//! documents.
//!
//! **Refinement commits every dictionary to a new handle**, so the one it replaced stays
//! in the arena unreferenced, and `list_fonts` walked every handle there rather than what
//! the document reaches. A count through objects instead would drop the orphan — and
//! would also drop a font written *directly* into a resource dictionary, which is legal
//! (7.3.10) and is the shape this engine's own decorations used until 2026-09-19. What
//! settles it is reaching the fonts the way a reader does.

use fepdf::{IngestionOptions, PdfDocument};

/// The nine samples, by name.
fn samples() -> Vec<std::path::PathBuf> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("samples/ is in the tree")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pdf"))
        .collect();
    files.sort();
    files
}

fn fonts_in(path: &std::path::Path, refine: bool) -> Option<usize> {
    let bytes = std::fs::read(path).ok()?;
    let options = IngestionOptions { active_refinement: refine, ..IngestionOptions::default() };
    Some(PdfDocument::open_with_options(bytes.into(), &options).ok()?.fonts().len())
}

/// **A document has the fonts it has, however it was read.**
///
/// Refinement changes what the engine understands of a file and not what the file holds,
/// so a count that moves when it is switched on is counting the engine rather than the
/// document.
#[test]
fn the_font_count_does_not_depend_on_how_the_file_was_read() {
    let mut disagreed = Vec::new();
    for path in samples() {
        let (refined, plain) = (fonts_in(&path, true), fonts_in(&path, false));
        if refined != plain {
            disagreed.push(format!("{:?}: {refined:?} refined, {plain:?} plain", path.file_name()));
        }
    }
    assert!(disagreed.is_empty(), "the same files count differently: {disagreed:#?}");
}

/// **A font written directly into a resource dictionary is still a font.**
///
/// 7.3.10 lets any object be direct, and a font dictionary in a `/Font` resource needs no
/// object of its own. Counting through objects would have fixed the doubling and lost
/// these — including the ones this engine itself wrote until 2026-09-19 — so this is the
/// half of the fix that is not about the number going down.
#[test]
fn a_font_written_directly_into_the_resources_is_counted() {
    let content = "BT /F1 24 Tf 1 0 0 1 40 700 Tm (A) Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
          /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> \
          >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    let doc = PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens");

    let fonts = doc.fonts();
    assert_eq!(fonts.len(), 1, "a direct font dictionary was not counted: {fonts:?}");
    assert_eq!(fonts[0].name, "Helvetica");
}

/// A font nothing reaches is not a font this document has.
#[test]
fn a_font_no_page_reaches_is_not_counted() {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
        // An orphan, of exactly the shape refinement leaves behind.
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>",
    ];
    let doc = PdfDocument::open_with_options(
        fepdf_fixtures::assemble(&bodies).into(),
        &IngestionOptions::default(),
    )
    .expect("the fixture opens");
    assert!(doc.fonts().is_empty(), "an unreachable font was counted: {:?}", doc.fonts());
}
