//! The content-stream backend contract.
//!
//! Interpreting a page means walking its content stream and issuing calls against a
//! [`RenderBackend`]. This crate owns that contract and the values passed across it;
//! it deliberately knows nothing about how any particular backend answers.
//!
//! Implementations live elsewhere — `fepdf-render` rasterises through Vello, while
//! text extraction and geometry collection implement the same trait with no GPU
//! present. See `ARCHITECTURE.md` Rule B: a crate that defines a contract does not
//! depend on its implementations.

pub mod path;

use fepdf_core::graphics::TextRenderingMode;
pub use fepdf_core::graphics::WindingRule;
use fepdf_core::{BlendMode, Color, PixelFormat, StrokeStyle};
use kurbo::{Affine, BezPath};
use std::sync::Arc;

pub use fepdf_core::font::FallbackFontType;

/// A soft mask accompanying an image, carrying its own dimensions and format.
#[derive(Debug, Clone)]
pub struct SMaskData {
    /// Raw mask samples.
    pub data: Vec<u8>,
    /// Mask width in samples.
    pub width: u32,
    /// Mask height in samples.
    pub height: u32,
    /// How `data` is laid out.
    pub format: PixelFormat,
}

/// One positioned glyph handed to a backend.
#[derive(Debug, Clone)]
pub struct TextGlyph {
    /// Glyph index within the resolved font program.
    pub gid: u32,
    /// Glyph name, when the encoding supplies one.
    pub name: Option<String>,
    /// The originating character code.
    pub char_code: u32,
    /// Unicode text this glyph stands for, for extraction and selection.
    pub unicode: String,
    /// Horizontal advance.
    pub width: f32,
    /// Vertical origin displacement (vertical writing modes).
    pub vx: f32,
    /// Vertical advance (vertical writing modes).
    pub vy: f32,
    /// Whether a fallback font supplied this glyph.
    pub is_fallback: bool,
}

/// Text state accompanying a `show_text` call.
#[derive(Debug, Clone, Copy)]
pub struct TextState {
    /// Character spacing (`Tc`).
    pub tc: f64,
    /// Word spacing (`Tw`).
    pub tw: f64,
    /// Horizontal scaling (`Tz`), as a ratio.
    pub th: f64,
    /// Whether the writing mode is vertical.
    pub is_vertical: bool,
}

/// The receiver of interpreted content-stream operations.
///
/// A backend decides what "drawing" means: rasterising to a GPU surface, collecting
/// text runs, or accumulating geometry. The interpreter is identical in every case.
pub trait RenderBackend {
    /// Concatenates `transform` onto the current transformation matrix.
    fn transform(&mut self, transform: Affine);
    /// Replaces the current transformation matrix.
    fn set_transform(&mut self, transform: Affine);
    /// Pushes the graphics state (`q`).
    fn push_state(&mut self);
    /// Pops the graphics state (`Q`).
    fn pop_state(&mut self);
    /// Fills `path` under the given winding rule.
    fn fill_path(&mut self, path: &BezPath, color: &Color, rule: WindingRule);
    /// Strokes `path` with the given pen.
    fn stroke_path(&mut self, path: &BezPath, color: &Color, style: &StrokeStyle);
    /// Intersects the clip region with `path`.
    fn push_clip(&mut self, path: &BezPath, rule: WindingRule);
    /// Restores the clip region saved by the matching [`RenderBackend::push_clip`].
    fn pop_clip(&mut self);
    /// Sets the fill alpha constant.
    fn set_fill_alpha(&mut self, alpha: f64);
    /// Sets the stroke alpha constant.
    fn set_stroke_alpha(&mut self, alpha: f64);
    /// Sets the current fill colour.
    fn set_fill_color(&mut self, color: Color);
    /// Sets the current stroke colour.
    fn set_stroke_color(&mut self, color: Color);
    /// Sets the current blend mode.
    fn set_blend_mode(&mut self, mode: BlendMode);
    /// Draws a decoded image, optionally masked.
    fn draw_image(
        &mut self,
        image: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
        smask: Option<SMaskData>,
    );
    /// Registers a font under `name` for subsequent [`RenderBackend::set_font`] calls.
    #[allow(clippy::too_many_arguments)]
    fn define_font(
        &mut self,
        name: &str,
        base_name: Option<&str>,
        data: Option<Arc<Vec<u8>>>,
        index: Option<usize>,
        cid_to_gid_map: Option<std::collections::BTreeMap<u32, u32>>,
        fallback_type: FallbackFontType,
        is_cid_keyed: bool,
    );
    /// Selects a previously defined font.
    fn set_font(&mut self, name: &str);
    /// Sets the text rendering mode (`Tr`).
    fn set_text_render_mode(&mut self, mode: TextRenderingMode);
    /// Sets character spacing (`Tc`).
    fn set_char_spacing(&mut self, spacing: f64);
    /// Sets word spacing (`Tw`).
    fn set_word_spacing(&mut self, spacing: f64);
    /// Emits a run of positioned glyphs.
    fn show_text(
        &mut self,
        glyphs: &[TextGlyph],
        size: f64,
        transform: Affine,
        state: TextState,
        op_index: usize,
    );
}
