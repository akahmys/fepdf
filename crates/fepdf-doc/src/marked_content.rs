//! Where a page's marked content landed (14.7.4.2), and nothing else.
//!
//! A structure element says which marks are its own by number — `/K` holds `/MCID`s, and
//! the numbers mean nothing outside the content stream that wrote them. The interpreter
//! measures those marks while it draws; this is the backend that keeps the answer and
//! throws the drawing away.
//!
//! **Why a backend of its own.** The two that exist collect text: one composes it,
//! the other places it in spans. Asking either for geometry it does not need would make
//! every caller of `extract_text` pay for a map it never reads, and the structure tree
//! wants the boxes of images and rules as much as those of glyphs.

use fepdf_content::{FallbackFontType, RenderBackend, SMaskData, TextGlyph, TextState};
use fepdf_model::graphics::{
    BlendMode, Color, PixelFormat, StrokeStyle, TextRenderingMode, WindingRule,
};
use kurbo::{Affine, BezPath};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A [`RenderBackend`] that draws nothing and remembers where each `/MCID` drew.
#[derive(Debug, Default)]
pub struct MarkBoundsBackend {
    /// The boxes, in default user space, once the page has finished.
    bounds: BTreeMap<u32, kurbo::Rect>,
}

impl MarkBoundsBackend {
    /// A backend with no page measured yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The boxes of the page just drawn.
    #[must_use]
    pub fn into_bounds(self) -> BTreeMap<u32, kurbo::Rect> {
        self.bounds
    }
}

impl RenderBackend for MarkBoundsBackend {
    fn receive_mark_bounds(&mut self, bounds: BTreeMap<u32, kurbo::Rect>) {
        self.bounds = bounds;
    }

    // Everything below is the trait's required surface, answered with silence. The
    // geometry this backend exists for does not pass through any of them — it is composed
    // one layer up, by the canvas that guards them.
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
        _cid_to_gid_map: Option<BTreeMap<u32, u32>>,
        _fallback_type: FallbackFontType,
        _is_cid_keyed: bool,
    ) {
    }
    fn set_font(&mut self, _name: &str) {}
    fn set_text_render_mode(&mut self, _mode: TextRenderingMode) {}
    fn set_char_spacing(&mut self, _spacing: f64) {}
    fn set_word_spacing(&mut self, _spacing: f64) {}
    fn show_text(
        &mut self,
        _glyphs: &[TextGlyph],
        _size: f64,
        _transform: Affine,
        _state: TextState,
        _op_index: usize,
    ) {
    }
}
