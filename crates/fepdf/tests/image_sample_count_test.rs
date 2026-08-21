//! What a backend is handed must match what the image dictionary describes.
//!
//! **The "headless rendering fails on a small page" entry was neither.** A 64×32 page
//! produced *"Copy at offset 0 for 8192 bytes would end up overrunning the bounds of the
//! Source buffer of size 1024"* from wgpu, and it was filed as a rendering defect and
//! worked around by enlarging a fixture. Reproducing it names the cause in arithmetic: a
//! 64×32 image at **one bit per component** decodes to 256 bytes, a backend that reads
//! one byte per pixel makes 256 pixels of RGBA out of them — 1024 bytes — and the texture
//! it is being written into is 64×32, which wants 8192. Eight times too short, which is
//! the same defect Phase M records fixing for `/DeviceGray` scans; the page size never
//! entered into it. Enlarging the fixture did not help either: 256×128 fails the same way
//! against the same code, by the same factor.
//!
//! It has been fixed since, in two independent places, and **neither had a test**. These
//! are that test, at the level the defect lives at: the bytes handed across the
//! [`RenderBackend`] contract. Asserting on those rather than on "the GPU did not crash"
//! is both faster and more precise, and it runs where there is no GPU at all.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_content::{
    BlendMode, Color, FallbackFontType, Paint, PixelFormat, RenderBackend, SMaskData, ShadingSpec,
    StrokeStyle, TextGlyph, TextState, WindingRule,
};
use fepdf_model::graphics::TextRenderingMode;
use kurbo::{Affine, BezPath};
use std::sync::Arc;

/// The image the page drew, as the backend was given it.
#[derive(Default)]
struct Drawn {
    samples: Vec<u8>,
    size: Option<(u32, u32)>,
    format: Option<PixelFormat>,
}

impl RenderBackend for Drawn {
    fn draw_image(
        &mut self,
        image: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
        _smask: Option<SMaskData>,
    ) {
        self.samples = image.to_vec();
        self.size = Some((width, height));
        self.format = Some(format);
    }
    fn transform(&mut self, _transform: Affine) {}
    fn set_transform(&mut self, _transform: Affine) {}
    fn push_state(&mut self) {}
    fn pop_state(&mut self) {}
    fn fill_path(&mut self, _path: &BezPath, _color: &Color, _rule: WindingRule) {}
    fn stroke_path(&mut self, _path: &BezPath, _color: &Color, _style: &StrokeStyle) {}
    fn push_clip(&mut self, _path: &BezPath, _rule: WindingRule) {}
    fn pop_clip(&mut self) {}
    fn set_fill_alpha(&mut self, _alpha: f64) {}
    fn set_stroke_alpha(&mut self, _alpha: f64) {}
    fn set_fill_color(&mut self, _color: Color) {}
    fn set_stroke_color(&mut self, _color: Color) {}
    fn set_fill_paint(&mut self, _paint: &Paint) {}
    fn set_stroke_paint(&mut self, _paint: &Paint) {}
    fn paint_shading(&mut self, _shading: &ShadingSpec) {}
    fn set_blend_mode(&mut self, _mode: BlendMode) {}
    fn show_text(
        &mut self,
        _glyphs: &[TextGlyph],
        _size: f64,
        _transform: Affine,
        _state: TextState,
        _op_index: usize,
    ) {
    }
    #[allow(clippy::too_many_arguments)]
    fn define_font(
        &mut self,
        _name: &str,
        _base_name: Option<&str>,
        _data: Option<Arc<Vec<u8>>>,
        _index: Option<usize>,
        _cid_to_gid_map: Option<std::collections::BTreeMap<u32, u32>>,
        _fallback_type: FallbackFontType,
        _is_cid_keyed: bool,
    ) {
    }
    fn set_font(&mut self, _name: &str) {}
    fn set_text_render_mode(&mut self, _mode: TextRenderingMode) {}
    fn set_char_spacing(&mut self, _spacing: f64) {}
    fn set_word_spacing(&mut self, _spacing: f64) {}
}

/// A page exactly the size of the image it draws, with `entries` in the image dictionary.
fn page_with_image(width: u32, height: u32, entries: &str, data: &[u8]) -> Vec<u8> {
    let content = format!("q {width} 0 0 {height} 0 0 cm /Im0 Do Q");
    let header = format!(
        "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} {entries} \
         /Length {} >>",
        data.len()
    );
    let mut out = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();
    let object = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    object(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    object(&mut out, &mut offsets, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    object(
        &mut out,
        &mut offsets,
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] \
             /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>"
        )
        .as_bytes(),
    );
    object(
        &mut out,
        &mut offsets,
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).as_bytes(),
    );
    let mut image = header.into_bytes();
    image.extend_from_slice(b"\nstream\n");
    image.extend_from_slice(data);
    image.extend_from_slice(b"\nendstream");
    object(&mut out, &mut offsets, &image);

    let table_at = out.len();
    let size = offsets.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n")
            .as_bytes(),
    );
    out
}

/// Interprets the page and reports what the backend got, with the decisions taken.
fn draw(file: Vec<u8>) -> (Drawn, Vec<String>) {
    let doc = PdfDocument::open_with_options(file.into(), &IngestionOptions::default())
        .expect("the fixture opens");
    let mut drawn = Drawn::default();
    doc.render_page(0, &mut drawn, Affine::IDENTITY).expect("the page interprets");
    let decisions = doc.decisions().iter().map(|d| format!("{} {}", d.clause, d.found)).collect();
    (drawn, decisions)
}

/// A 1-bit `/DeviceGray` image, black in the top-left quarter — the commonest image in a
/// scanned document, at the size that produced the wgpu failure.
fn one_bit_gray(width: u32, height: u32) -> Vec<u8> {
    let stride = (width as usize).div_ceil(8);
    let mut out = vec![0_u8; stride * height as usize];
    for y in 0..height {
        for x in 0..width {
            if x >= width / 2 || y >= height / 2 {
                // 1 is white in `/DeviceGray`; the top-left quarter stays 0, which is black.
                out[y as usize * stride + (x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
    }
    out
}

/// The failure, at the size it was reported at. 64×32 at one bit is 256 bytes; the
/// backend must be handed 2048, one per pixel, or the RGBA buffer it builds is 1024 bytes
/// against a texture wanting 8192.
#[test]
fn a_one_bit_gray_image_reaches_the_backend_one_byte_per_pixel() {
    let (drawn, decisions) = draw(page_with_image(
        64,
        32,
        "/ColorSpace /DeviceGray /BitsPerComponent 1",
        &one_bit_gray(64, 32),
    ));
    assert_eq!(drawn.size, Some((64, 32)));
    assert_eq!(drawn.format, Some(PixelFormat::Gray8));
    assert_eq!(
        drawn.samples.len(),
        64 * 32,
        "the samples are still packed eight to a byte, which is the 8x shortfall that \
         killed the process"
    );
    assert_eq!(drawn.samples[0], 0, "the top-left quarter is black");
    assert_eq!(drawn.samples[63], 255, "the top-right is white");
    assert!(decisions.is_empty(), "expanding conforming samples is not a departure: {decisions:?}");
}

/// The page size never entered into it: the same image at the size the fixture was
/// enlarged to fails and passes for exactly the same reason.
#[test]
fn the_same_holds_at_the_size_the_fixture_was_enlarged_to() {
    let (drawn, _) = draw(page_with_image(
        256,
        128,
        "/ColorSpace /DeviceGray /BitsPerComponent 1",
        &one_bit_gray(256, 128),
    ));
    assert_eq!(drawn.samples.len(), 256 * 128);
}

/// Four bits per component is the other sub-byte depth a scan uses, and a width that is
/// not a whole number of bytes is where a stride calculation goes wrong.
#[test]
fn a_sub_byte_depth_that_does_not_divide_the_width_still_expands() {
    let width = 5_u32;
    let height = 2_u32;
    // Two samples per byte, three bytes per row for five samples: the last nibble is padding.
    let data = [0x0F, 0x0F, 0x00, 0x0F, 0x0F, 0x00];
    let (drawn, _) =
        draw(page_with_image(width, height, "/ColorSpace /DeviceGray /BitsPerComponent 4", &data));
    assert_eq!(drawn.samples.len(), (width * height) as usize, "row padding was read as samples");
    assert_eq!(drawn.samples[0], 0, "the first sample is 0 of 15");
    assert_eq!(drawn.samples[1], 255, "the second is 15 of 15");
}

/// The second guard, independent of the first. An image whose data is short for what its
/// dictionary describes is skipped and recorded — because the alternative was handing the
/// GPU a buffer it refused, and the process died over a document defect.
#[test]
fn an_image_shorter_than_its_dictionary_describes_is_skipped_and_recorded() {
    let (drawn, decisions) =
        draw(page_with_image(64, 32, "/ColorSpace /DeviceGray /BitsPerComponent 8", &[0_u8; 100]));
    assert!(drawn.size.is_none(), "a short image reached the backend: {:?}", drawn.samples.len());
    assert!(
        decisions.iter().any(|d| d.starts_with("8.9.5.1") && d.contains("2048")),
        "the shortfall was not reported: {decisions:?}"
    );
}
