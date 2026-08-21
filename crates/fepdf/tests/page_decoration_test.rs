//! What adding a decoration must not cost the page it is added to.
//!
//! Overlaying a header, a footer or a Bates number needs Helvetica in the page's
//! resources, and the code that put it there reached for `/Resources` on the page
//! dictionary alone — building a fresh empty one when it was not found. Two shapes the
//! standard allows break under that:
//!
//! - `/Resources` written as an indirect reference. `as_dict_handle` does not resolve
//!   one, so the page's resources were **replaced** by the new empty dictionary.
//! - `/Resources` inherited from the page tree (7.7.3.4), which a document with uniform
//!   pages normally does. The fresh dictionary on the page **shadowed** it.
//!
//! Either way the fonts and XObjects the page's own content stream names stopped
//! resolving, and the page came out blank with a header on it. These fixtures are the two
//! shapes; what they assert is that the text that was there is still drawn.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_content::{
    BlendMode, Color, FallbackFontType, Paint, PixelFormat, RenderBackend, SMaskData, ShadingSpec,
    StrokeStyle, TextGlyph, TextState, WindingRule,
};
use fepdf_doc::operation::{DecorationPosition, Operation, PageSelection};
use fepdf_model::graphics::TextRenderingMode;
use kurbo::{Affine, BezPath};
use std::sync::Arc;

/// Collects the text the page draws.
#[derive(Default)]
struct Text(String);

impl RenderBackend for Text {
    fn show_text(
        &mut self,
        glyphs: &[TextGlyph],
        _size: f64,
        _transform: Affine,
        _state: TextState,
        _op_index: usize,
    ) {
        for glyph in glyphs {
            self.0.push_str(&glyph.unicode);
        }
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
    fn draw_image(
        &mut self,
        _image: &[u8],
        _width: u32,
        _height: u32,
        _format: PixelFormat,
        _smask: Option<SMaskData>,
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

/// A one-page file drawing `ORIGINAL`, with its resources placed by `resources_on_page`
/// and `resources_on_tree` — the two ways 7.7.3.4 lets a page reach them.
fn page_drawing_original(resources_on_page: &str, resources_on_tree: &str) -> Vec<u8> {
    let content = "BT /F1 24 Tf 1 0 0 1 40 700 Tm (ORIGINAL) Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Kids [3 0 R] /Count 1 {resources_on_tree} >>"),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] {resources_on_page} \
             /Contents 4 0 R >>"
        ),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Font << /F1 6 0 R >> >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let table_at = out.len();
    let size = bodies.len() + 1;
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

/// Decorates page 1 and reports the text the page then draws.
fn text_after_decorating(file: Vec<u8>) -> String {
    let mut doc = PdfDocument::open_with_options(file.into(), &IngestionOptions::default())
        .expect("the fixture opens");
    doc.apply(Operation::AddPageDecoration {
        pages: PageSelection::All,
        text: "HEADER".to_string(),
        position: DecorationPosition::TopCenter,
        layer: None,
    })
    .expect("the decoration applies");
    let mut text = Text::default();
    doc.render_page(0, &mut text, Affine::IDENTITY).expect("the page interprets");
    text.0
}

#[test]
fn a_page_whose_resources_are_indirect_keeps_them() {
    let drawn = text_after_decorating(page_drawing_original("/Resources 5 0 R", ""));
    assert!(drawn.contains("ORIGINAL"), "the page's own text was lost: {drawn:?}");
    assert!(drawn.contains("HEADER"), "the decoration was not added: {drawn:?}");
}

#[test]
fn a_page_that_inherits_its_resources_keeps_them() {
    let drawn = text_after_decorating(page_drawing_original("", "/Resources 5 0 R"));
    assert!(drawn.contains("ORIGINAL"), "the inherited resources were shadowed: {drawn:?}");
    assert!(drawn.contains("HEADER"), "the decoration was not added: {drawn:?}");
}

/// The shape that already worked, kept so a fix to the two above cannot quietly break it.
#[test]
fn a_page_carrying_its_own_resources_keeps_them() {
    let drawn =
        text_after_decorating(page_drawing_original("/Resources << /Font << /F1 6 0 R >> >>", ""));
    assert!(drawn.contains("ORIGINAL"), "the page's own text was lost: {drawn:?}");
    assert!(drawn.contains("HEADER"), "the decoration was not added: {drawn:?}");
}
