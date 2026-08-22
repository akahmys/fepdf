//! What a rendered page looks like, in four numbers — a comparator that survives
//! antialiasing.
//!
//! `scripts/test/crosscheck_image.sh` asks this and PDFKit the same question about the
//! same file and compares the answers. Four numbers rather than a pixel-for-pixel diff
//! because two renderers legitimately disagree about edges, and because four numbers say
//! *which way* they disagree: the fixtures are black in one quadrant, so an inverted
//! image, a flipped one, a transposed one and a smeared one each produce a different
//! four.
//!
//! Prints the mean luminance of each quadrant, 0 (black) to 255 (white), in the order
//! top-left, top-right, bottom-left, bottom-right.
//!
//! ```text
//! cargo run --release --example page_quadrants -p fepdf --features render -- file.pdf
//! ```

use fepdf::PdfDocument;
use fepdf_render::VelloBackend;
use fepdf_render::headless::render_to_bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: page_quadrants <file.pdf>")?;
    let document = PdfDocument::open(std::fs::read(&path)?.into())?;
    let page = document.get_page(0)?;

    let media = page.media_box();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (width, height) = (media.width() as u32, media.height() as u32);
    if width == 0 || height == 0 {
        return Err("the page has no area".into());
    }

    let mut backend = VelloBackend::new(VelloBackend::load_system_fonts());
    // PDF counts up from the bottom and a raster counts down from the top.
    let transform = kurbo::Affine::scale_non_uniform(1.0, -1.0)
        * kurbo::Affine::translate((0.0, -f64::from(height)));
    // `render_page` and not an interpreter built here: the page's own contents are only
    // half of what a reader draws, and this example exists to be compared against one
    // that draws the other half. Building the interpreter directly skipped every
    // annotation appearance, so the comparison could not have caught their absence.
    // The outcome is deliberately not propagated: this is a *comparator*, and its answer
    // is what reached the page. A file whose content stream will not decode still draws
    // its annotations, and refusing to report four numbers for it would make the
    // comparison silent exactly where the two renderers are most likely to differ.
    let _ = document.render_page(0, &mut backend, transform);

    let pixels = render_to_bytes(backend.scene(), width, height).await?;
    let quadrants = quadrant_means(&pixels, width, height);
    println!("{} {} {} {}", quadrants[0], quadrants[1], quadrants[2], quadrants[3]);
    Ok(())
}

/// The mean luminance of each quadrant: top-left, top-right, bottom-left, bottom-right.
///
/// A pixel the renderer never painted is white, because that is what paper is and what
/// both renderers start from.
fn quadrant_means(rgba: &[u8], width: u32, height: u32) -> [u8; 4] {
    let mut totals = [0_u64; 4];
    let mut counts = [0_u64; 4];
    for y in 0..height {
        for x in 0..width {
            let at = ((y * width + x) * 4) as usize;
            let Some(px) = rgba.get(at..at + 4) else { continue };
            // Over white: a transparent pixel is paper, not black.
            let alpha = f64::from(px[3]) / 255.0;
            let luma = 0.114f64.mul_add(
                f64::from(px[2]),
                0.587f64.mul_add(f64::from(px[1]), 0.299 * f64::from(px[0])),
            );
            let over_white = alpha.mul_add(luma, (1.0 - alpha) * 255.0);

            let q = usize::from(x >= width / 2) + 2 * usize::from(y >= height / 2);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                totals[q] += over_white as u64;
            }
            counts[q] += 1;
        }
    }
    let mut out = [255_u8; 4];
    for q in 0..4 {
        // A quadrant with no pixels is paper, which is what 255 already says.
        if let Some(mean) = totals[q].checked_div(counts[q]) {
            out[q] = u8::try_from(mean).unwrap_or(255);
        }
    }
    out
}
