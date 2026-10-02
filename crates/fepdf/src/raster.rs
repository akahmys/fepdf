//! A page as pixels: rendered to a file, to a region, or to an image of a given
//! resolution.

use super::{PdfDocument, page_display_transform, pixels_across};
use fepdf_model::{PdfError, PdfResult};
use fepdf_render::{VelloBackend, headless::Rasteriser};
use std::path::Path;
use std::sync::Arc;

impl PdfDocument {
    /// Renders a specific page to an image file, detecting format from extension.
    ///
    /// Requires the `render` feature, which pulls in the Vello + wgpu stack.
    #[cfg(feature = "render")]
    pub fn render_page_to_file(&self, index: usize, output_path: &Path) -> PdfResult<()> {
        self.render_page_to_file_with(index, output_path, Rasteriser::Gpu)
    }

    /// A rectangle of a page, rasterised, as RGBA pixels and the size they came out.
    ///
    /// **The snapshot Acrobat calls スナップショット is a read**, not an `Operation`:
    /// nothing about the document changes, so it belongs here beside `extract_text` and
    /// `render_page` rather than in the vocabulary Rule D governs.
    ///
    /// `keep` is in the page's own space and `scale` is the caller's. A snapshot taken at
    /// whatever the screen happens to be showing is a snapshot nobody can ask for twice,
    /// so the resolution is asked for rather than inferred — `4.0 / 3.0` is the 96 DPI
    /// `render_page_to_file` uses, and twice that is twice the detail.
    ///
    /// **`/UserUnit` is not applied here** and is where `render_page_to_file` does apply
    /// it (Table 31). A caller asking for a region of a page in that page's coordinates
    /// has already said what it wants in those coordinates; multiplying by the unit would
    /// answer a rectangle it did not ask about. A caller that wants the page's own sense
    /// of scale multiplies `scale` by [`Self::get_page_user_unit`].
    ///
    /// # Errors
    /// Fails when the page is not there, when the rectangle has no area, when the scale
    /// is not a positive finite number, or when the rasteriser does.
    #[cfg(feature = "render")]
    pub fn render_region(
        &self,
        index: usize,
        keep: (f64, f64, f64, f64),
        scale: f64,
    ) -> PdfResult<(Vec<u8>, u32, u32)> {
        self.render_region_with(index, keep, scale, Rasteriser::Gpu)
    }

    /// [`Self::render_region`], naming which rasteriser runs.
    ///
    /// # Errors
    /// The same as [`Self::render_region`].
    #[cfg(feature = "render")]
    pub fn render_region_with(
        &self,
        index: usize,
        keep: (f64, f64, f64, f64),
        scale: f64,
        rasteriser: Rasteriser,
    ) -> PdfResult<(Vec<u8>, u32, u32)> {
        let (wide, tall) = (keep.2 - keep.0, keep.3 - keep.1);
        if !scale.is_finite() || scale <= 0.0 {
            return Err(PdfError::refused("render", format!("a scale of {scale} draws nothing")));
        }
        if !(wide.is_finite() && tall.is_finite()) || wide <= 0.0 || tall <= 0.0 {
            return Err(PdfError::refused(
                "render",
                format!("a region of {wide} by {tall} points has no area"),
            ));
        }
        let (width, height) = pixels_across(wide, tall, scale)?;

        let mut backend = VelloBackend::new(Arc::clone(&self.inner.system_fonts));
        // The page is drawn whole and the region is what the frame is put around: the
        // transform puts `keep`'s lower-left corner at the image's lower-left, and the
        // flip is the one every render of a page does — a page counts up from its foot
        // and an image down from its head.
        let transform =
            kurbo::Affine::new([scale, 0.0, 0.0, -scale, -keep.0 * scale, keep.3 * scale]);
        self.render_page(index, &mut backend, transform)?;

        let pixels = pollster::block_on(fepdf_render::headless::render_to_bytes_with(
            backend.scene(),
            width,
            height,
            rasteriser,
        ))
        .map_err(|e: Box<dyn std::error::Error>| PdfError::internal(e.to_string()))?;
        Ok((pixels, width, height))
    }

    /// [`PdfDocument::render_page_to_file`], naming which rasteriser runs.
    ///
    /// **A caller wanting the same image twice must ask for `Cpu`.** The engine encodes a
    /// byte-identical scene for a given page every time, and vello's GPU pipeline turns
    /// that one scene into more than one image — three distinct images in eight renders of
    /// `samples/constitution.pdf` page 1, one isolated pixel apart at a channel delta of 1. The
    /// CPU shaders give one ([ADR-0043]).
    ///
    /// `scripts/visual_regression.py` is deliberately **not** this caller: it tolerates a
    /// delta of 1, which is the whole size of the difference, and rendering it on the CPU
    /// would stop it exercising the pipeline a user actually gets. What needs this is a
    /// caller for whom "the same" means the same bytes — a hash, a cache key, a signature
    /// over a rendering.
    ///
    /// [ADR-0043]: https://github.com/akahmys/fepdf/blob/main/docs/adr/0043-the-scene-repeats-and-the-rasteriser-does-not.md
    #[cfg(feature = "render")]
    pub fn render_page_to_file_with(
        &self,
        index: usize,
        output_path: &Path,
        rasteriser: Rasteriser,
    ) -> PdfResult<()> {
        self.render_page_to_file_at(index, output_path, 96.0, rasteriser)
    }

    /// Where page `index`'s user space lands in an image of it at `dpi`: the transform
    /// from default user space to pixels, whose origin is the image's top left corner,
    /// and the image's size.
    ///
    /// Its inverse takes a box an OCR engine found in the image back to the page.
    ///
    /// # Errors
    /// Fails when the page is not there or `dpi` is not a positive number.
    pub fn page_to_pixels(&self, index: usize, dpi: f64) -> PdfResult<(kurbo::Affine, u32, u32)> {
        if !(dpi > 0.0 && dpi.is_finite()) {
            return Err(PdfError::refused("render", format!("{dpi} dots per inch is no image")));
        }
        let r = self.get_page_box(index)?;
        let rot = self.get_page_rotation(index)?;

        // The DPI, times whatever a user space unit is worth on this page. `/UserUnit` is
        // how a drawing exceeds the 14,400-unit limit a box can express (Table 31): the
        // coordinates stay as written and each one is worth more of an inch, so honouring
        // it is a matter of the scale and nothing else — the content is drawn unchanged.
        let scale = dpi / 72.0 * self.get_page_user_unit(index)?;
        let (transform, display_w, display_h) = page_display_transform(r, rot, scale);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (width, height) =
            ((display_w * scale).round() as u32, (display_h * scale).round() as u32);
        Ok((transform, width, height))
    }

    /// [`PdfDocument::render_page_to_file_with`], at `dpi` rather than 96.
    ///
    /// # Errors
    /// Fails when the page is not there, `dpi` is not a positive number, the extension
    /// names no format this writes, or the rasteriser does.
    #[cfg(feature = "render")]
    pub fn render_page_to_file_at(
        &self,
        index: usize,
        output_path: &Path,
        dpi: f64,
        rasteriser: Rasteriser,
    ) -> PdfResult<()> {
        let (initial_transform, width, height) = self.page_to_pixels(index, dpi)?;
        let mut backend = VelloBackend::new(Arc::clone(&self.inner.system_fonts));
        self.render_page(index, &mut backend, initial_transform)?;

        let format = match output_path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .as_deref()
        {
            Some("png") => image::ImageFormat::Png,
            Some("jpg" | "jpeg") => image::ImageFormat::Jpeg,
            _ => return Err(PdfError::refused(
                "render",
                "Unsupported image format. Only PNG and JPEG (.png, .jpg, .jpeg) are supported."
                    .to_string(),
            )),
        };

        // Finalize rendering using the headless bridge
        let scene = backend.scene();
        pollster::block_on(fepdf_render::headless::render_to_image_with(
            scene,
            width,
            height,
            output_path,
            format,
            rasteriser,
        ))
        .map_err(|e: Box<dyn std::error::Error>| {
            PdfError::Io(std::io::Error::other(e.to_string()))
        })?;

        Ok(())
    }
}
