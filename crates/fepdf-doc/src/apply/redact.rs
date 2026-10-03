//! Removing what a region of a page holds, and filling it (12.5.6.23, ROADMAP Y-10).
//!
//! **Removed, not covered.** The route this replaces wrote `[REDACTED]` over every string
//! of each show-text operator touching a rectangle, covered or not, and drew nothing: the
//! window called that 黒塗り. Here a glyph goes when its box meets a region at all — the
//! rule MuPDF documents — and the glyphs outside keep their places, as a crop's do
//! ([`super::text::remove_glyphs`]). Then the region is filled.
//!
//! **What will go can be read first**: [`what_redaction_removes`] runs the same test and
//! writes nothing, so a frontend can show it before the reader commits to it. The overlap
//! rule takes a glyph that only touches a region's edge, and this is where that shows.

use super::text::{self, GlyphBox};
use crate::operation::Redaction;
use fepdf_model::interpretation::Decision;
use fepdf_model::{Document, PdfError, PdfResult};

/// How far under its baseline a glyph's box reaches, in em, for a redaction.
///
/// **Deeper than a text face's descender**, which reaches 0.207 em in Helvetica, 0.217 in
/// Times, 0.157 in Courier and 0.141 in MS Mincho: a region over the lower half of a line
/// meets the glyphs whose tails it covers. A box too deep takes a glyph the region only
/// nears, which a preview shows; one too shallow leaves ink inside the region.
const DESCENT: f64 = 0.3;

/// What a redaction will remove, read without removing it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Removal {
    /// The box of each glyph that goes, on the page: left, bottom, right, top.
    pub glyphs: Vec<(f64, f64, f64, f64)>,
}

/// What `redaction` would remove, with nothing written.
///
/// # Errors
/// Refuses a redaction naming no region or a region with no area, or a page with text
/// whose glyphs cannot be placed; fails when the page is not there or its content cannot
/// be read.
pub fn what_redaction_removes(doc: &Document, redaction: &Redaction) -> PdfResult<Removal> {
    let regions = regions_of(redaction)?;
    let glyphs = text::glyph_boxes(doc, redaction.page, DESCENT)?;
    Ok(Removal { glyphs: glyphs.into_iter().filter(|g| inside_any(*g, &regions)).collect() })
}

/// Removes what `redaction` names and fills its regions black.
///
/// **Black is this engine's choice, and is recorded as one.** A region a `/Redact`
/// annotation names is filled as Table 195 says, and left transparent when it says
/// nothing; a region a caller names has no annotation to say, so the fill is decided here.
///
/// # Errors
/// As [`what_redaction_removes`]; also refuses a page with text drawn in a font it does
/// not name, whose glyphs cannot be placed.
pub fn apply_redact(doc: &Document, redaction: &Redaction) -> PdfResult<()> {
    // Read first, so that a page this cannot read is refused before anything is written.
    let _ = what_redaction_removes(doc, redaction)?;
    let regions = regions_of(redaction)?;
    text::remove_glyphs(doc, redaction.page, DESCENT, &|g| inside_any(g, &regions))?;
    fill(doc, redaction.page, &regions)?;
    doc.decisions.push(Decision::ambiguity(
        "12.5.6.23",
        format!(
            "{} redaction regions on page {} were named with no /Redact annotation to say how to fill them",
            regions.len(),
            redaction.page
        ),
        "filled them black, which is this engine's choice",
    ));
    Ok(())
}

/// The regions, each put in order as left, bottom, right, top; refused when there are
/// none, or one has no area or a coordinate that is not a number.
fn regions_of(redaction: &Redaction) -> PdfResult<Vec<GlyphBox>> {
    if redaction.regions.is_empty() {
        return Err(PdfError::refused("redact", "no region was named"));
    }
    redaction
        .regions
        .iter()
        .map(|&(x0, y0, x1, y1)| {
            let region = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
            let finite = [x0, y0, x1, y1].iter().all(|v| v.is_finite());
            if finite && region.2 > region.0 && region.3 > region.1 {
                Ok(region)
            } else {
                Err(PdfError::refused("redact", format!("the region {region:?} has no area")))
            }
        })
        .collect()
}

/// Whether `glyph` meets any of `regions`.
fn inside_any(glyph: GlyphBox, regions: &[GlyphBox]) -> bool {
    regions.iter().any(|region| text::meets(glyph, *region))
}

/// Puts a black rectangle over each region, drawn after everything else in the page's
/// default user space: the page's own content is wrapped in `q` and `Q` first, so what it
/// leaves the transform saying does not move the fill.
fn fill(doc: &Document, page: usize, regions: &[GlyphBox]) -> PdfResult<()> {
    let mut content = b"q\n".to_vec();
    if let Some(data) = text::page_content(doc, page)? {
        content.extend_from_slice(&data);
    }
    content.extend_from_slice(b"\nQ\nq 0 g\n");
    for (left, bottom, right, top) in regions {
        content.extend_from_slice(
            format!("{left} {bottom} {} {} re\n", right - left, top - bottom).as_bytes(),
        );
    }
    content.extend_from_slice(b"f\nQ\n");
    text::write_page_content(doc, page, content)
}
