//! The images a redaction region lies over, with the pixels under it blanked (ROADMAP
//! Y-10).
//!
//! **Covered is not removed.** A fill drawn over an image leaves every pixel under it in
//! the file, decodable by anyone who reads the stream. Here each image the page's content
//! draws with `Do` and a region meets is replaced, for this drawing, by a copy whose
//! pixels under the region are blanked ([`super::image_crop::blanked`]); the original,
//! drawn nowhere else from these resources, is taken out of them, so the writer does not
//! write it.
//!
//! **Reached: image XObjects the page's own content draws.** Inline images and the
//! contents of form XObjects are later parts of Y-10.

use super::image_crop::{self, Fraction};
use super::text::GlyphBox;
use fepdf_model::lexer::Token;
use fepdf_model::{Document, Handle, Object, PdfError, PdfResult};
use kurbo::{Affine, Point, Rect};
use std::collections::BTreeMap;

/// One image the page's content draws.
struct Drawn {
    /// Where the name before `Do` sits among the tokens.
    name_at: usize,
    /// Where the `Do` sits.
    do_at: usize,
    /// The image.
    image: Handle<Object>,
    /// The transform it is drawn with, which takes its unit square onto the page.
    ctm: Affine,
}

/// Where on the page each image the regions meet would be blanked: each region cut to the
/// image's own box.
///
/// # Errors
/// Fails when the page is not there or its content cannot be read.
pub fn blanked_areas(
    doc: &Document,
    page: usize,
    regions: &[GlyphBox],
) -> PdfResult<Vec<GlyphBox>> {
    let Some(data) = super::text::page_content(doc, page)? else { return Ok(Vec::new()) };
    let tokens = image_crop::tokens_of(&data);
    let mut areas = Vec::new();
    for drawn in images_drawn(doc, page, &tokens)? {
        let on_page = drawn.ctm.transform_rect_bbox(Rect::new(0.0, 0.0, 1.0, 1.0));
        for region in regions {
            let cut = on_page.intersect(Rect::new(region.0, region.1, region.2, region.3));
            if cut.width() > 0.0 && cut.height() > 0.0 {
                areas.push((cut.x0, cut.y0, cut.x1, cut.y1));
            }
        }
    }
    Ok(areas)
}

/// Blanks the pixels `regions` lie over in every image the page's content draws.
///
/// # Errors
/// Refuses when an image a region meets cannot be decoded, since its pixels cannot be
/// blanked; fails when the page is not there or its content cannot be read.
pub fn blank_images(doc: &Document, page: usize, regions: &[GlyphBox]) -> PdfResult<()> {
    let Some(data) = super::text::page_content(doc, page)? else { return Ok(()) };
    let tokens = image_crop::tokens_of(&data);
    let mut replaced: BTreeMap<usize, (usize, Vec<u8>)> = BTreeMap::new();
    for drawn in images_drawn(doc, page, &tokens)? {
        let blocks = fractions(drawn.ctm, regions);
        if blocks.is_empty() {
            continue;
        }
        let Some(blank) = image_crop::blanked(doc, drawn.image, &blocks) else {
            return Err(PdfError::refused(
                "redact",
                "an image under the region cannot be decoded, so its pixels cannot be blanked; \
                 nothing was redacted",
            ));
        };
        let name = image_crop::name_in_page(doc, page, blank)?;
        replaced.insert(drawn.name_at, (drawn.do_at, format!("/{name} Do ").into_bytes()));
    }
    if replaced.is_empty() {
        return Ok(());
    }
    let out = super::path_crop::rewritten(&tokens, &replaced);
    super::text::write_page_content(doc, page, out)?;
    image_crop::drop_undrawn_images(doc, &[page])
}

/// Every image XObject `tokens` draw, with the transform each is drawn with.
fn images_drawn(doc: &Document, page: usize, tokens: &[Token]) -> PdfResult<Vec<Drawn>> {
    let images = image_crop::images_of(doc, page)?;
    let (mut ctm, mut saved, mut drawn) = (Affine::IDENTITY, Vec::new(), Vec::new());
    let mut operands_from = 0;
    for (index, token) in tokens.iter().enumerate() {
        let Token::Keyword(op) = token else { continue };
        let operands = &tokens[operands_from..index];
        operands_from = index + 1;
        match op.as_str() {
            "q" => saved.push(ctm),
            "Q" => ctm = saved.pop().unwrap_or(ctm),
            "cm" => ctm *= image_crop::matrix_of(operands),
            "Do" => {
                let Some(Token::Name(name)) = operands.last() else { continue };
                if let Some(image) = images.get(&String::from_utf8_lossy(name).to_string()) {
                    drawn.push(Drawn { name_at: index - 1, do_at: index, image: *image, ctm });
                }
            }
            _ => {}
        }
    }
    Ok(drawn)
}

/// Each region taken into the unit square of an image drawn with `ctm`, cut to it; the
/// ones that miss it are left out. The box round a region's corners there is a little more
/// than the region for an image drawn turned, never less.
fn fractions(ctm: Affine, regions: &[GlyphBox]) -> Vec<Fraction> {
    if ctm.determinant().abs() < 1e-12 {
        return Vec::new();
    }
    let inverse = ctm.inverse();
    regions
        .iter()
        .filter_map(|&(x0, y0, x1, y1)| {
            let corners =
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].map(|(x, y)| inverse * Point::new(x, y));
            let low =
                |f: fn(&Point) -> f64| corners.iter().map(f).fold(f64::MAX, f64::min).max(0.0);
            let high =
                |f: fn(&Point) -> f64| corners.iter().map(f).fold(f64::MIN, f64::max).min(1.0);
            let fraction = (low(|p| p.x), low(|p| p.y), high(|p| p.x), high(|p| p.y));
            (fraction.0 < fraction.2 && fraction.1 < fraction.3).then_some(fraction)
        })
        .collect()
}
