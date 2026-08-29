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

mod canvas;
/// The content stream interpreter that translates PDF operators into [RenderBackend] calls.
pub mod interpreter;
pub mod path;

pub use interpreter::{Interpreter, Type3Advance};

use fepdf_model::graphics::TextRenderingMode;
pub use fepdf_model::graphics::WindingRule;
pub use fepdf_model::graphics::{SoftMaskKind, SoftMaskSpec};
pub use fepdf_model::{
    AxialShading, BlendMode, Color, ColorStop, Paint, PatternSpec, PixelFormat, RadialShading,
    ShadingSpec, StrokeStyle,
};
use kurbo::{Affine, BezPath};
use std::sync::Arc;

pub use fepdf_model::font::FallbackFontType;

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
    /// Which route named it, or why none did (9.10.2).
    ///
    /// Carried beside the text rather than derived from it: an empty `unicode` says a
    /// glyph could not be named and this says *what would have named it*, which is the
    /// difference between a count and a direction.
    pub source: fepdf_model::font::UnicodeSource,
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
    /// Takes the decisions this backend reached about the document while drawing it.
    ///
    /// A backend sits below any `Document` — it is handed paths and glyphs, not a file —
    /// so it cannot call `Document::record` itself. What it *can* see is a font program
    /// whose glyph will not draw or a font the interpreter selected and it never
    /// received, and both change what reaches the page. `render_page` drains this after
    /// interpretation and records what comes back (ARCHITECTURE §4.3).
    ///
    /// Defaulted to empty: the text-extraction and collector backends reach no such
    /// conclusion, and a trait method they must all implement to say "none" is noise.
    fn take_decisions(&mut self) -> Vec<fepdf_model::interpretation::Decision> {
        Vec::new()
    }

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
    /// Sets the current fill paint (Solid or Pattern/Shading).
    fn set_fill_paint(&mut self, paint: &Paint) {
        if let Paint::Solid(col) = paint {
            self.set_fill_color(*col);
        }
    }
    /// Sets the current stroke paint (Solid or Pattern/Shading).
    fn set_stroke_paint(&mut self, paint: &Paint) {
        if let Paint::Solid(col) = paint {
            self.set_stroke_color(*col);
        }
    }
    /// Paints a shading directly across the current clip region (`sh` operator, ISO 32000-2 8.7.4.5.2).
    fn paint_shading(&mut self, _shading: &ShadingSpec) {}

    /// Opens content that a soft mask will cover (11.6.5.2).
    ///
    /// The three soft-mask methods are one bracket: this, then the content, then
    /// [`RenderBackend::begin_soft_mask`], then the mask group's own drawing, then
    /// [`RenderBackend::end_soft_mask`]. **The content comes before the mask** because
    /// that is the order a mask can be applied in without holding the content somewhere
    /// first — the mask is a property of what has already been drawn into this bracket.
    ///
    /// Defaulted to nothing, and that is a real answer rather than a placeholder: a
    /// backend that records calls or extracts text has no compositing step for a mask to
    /// modify, and the content inside the bracket is exactly what it should see.
    fn begin_masked_content(&mut self) {}

    /// Opens the mask's own drawing. What follows until
    /// [`RenderBackend::end_soft_mask`] defines the mask rather than appearing on the
    /// page.
    ///
    /// `spec` says how the drawing becomes an alpha — which channel, over which
    /// backdrop, through which transfer function. A backend that cannot honour all of it
    /// should record a [`Decision`] saying which part it dropped rather than applying a
    /// mask that is not the one asked for.
    ///
    /// [`Decision`]: fepdf_model::interpretation::Decision
    fn begin_soft_mask(&mut self, _spec: &SoftMaskSpec) {}

    /// Closes the bracket, applying the mask to the content inside it.
    fn end_soft_mask(&mut self) {}
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
    /// The text a marked-content section stands for (14.9.4), when it declares one.
    ///
    /// **What is drawn and what is meant are allowed to differ**, and a document that
    /// says so is the only reason anyone can read some pages. `volvo_xc90.pdf` draws its
    /// Chinese and Thai regulatory notices as 414 `.notdef` glyphs on one page and puts
    /// the real characters here; it also draws `/` and `-` with codes whose `/ToUnicode`
    /// says `U+0000`, and puts those here too. Extraction takes this in place of the
    /// glyphs between here and `end_actual_text`; rendering ignores it, because the
    /// glyphs are still what appears on the page.
    fn begin_actual_text(&mut self, _text: &str) {}
    /// Closes the section opened by `begin_actual_text`.
    fn end_actual_text(&mut self) {}
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
