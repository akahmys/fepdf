//! The backend, plus whether what is about to be drawn is meant to be seen.
//!
//! **Why a wrapper and not a check at each painting site.** Optional content (8.11) is
//! honoured by *not calling* five of [`RenderBackend`]'s methods while a hidden section
//! is open. Guarding those five call sites in the interpreter works until a sixth
//! painting site is added, and nothing would fail when the guard is forgotten — the page
//! would simply show a layer that is off, which is the defect this whole change exists to
//! remove. Putting the guard behind the trait makes the omission unrepresentable: a new
//! painting operator reaches the backend through here or not at all.
//!
//! Everything that is *not* painting is forwarded unconditionally, and that is
//! deliberate. `q`, `Q`, `cm`, the clip stack, the colour and the font all keep running
//! inside a hidden section, because the operators after `EMC` inherit the graphics state
//! the hidden ones left — a viewer that skipped them would come out of the section with
//! the wrong CTM. Only the marks on the page are withheld.

use crate::{
    Affine, Arc, BezPath, BlendMode, Color, FallbackFontType, Paint, PixelFormat, RenderBackend,
    SMaskData, ShadingSpec, StrokeStyle, TextGlyph, TextRenderingMode, TextState, WindingRule,
};

/// A [`RenderBackend`] that withholds marks while an optional-content section is off.
pub struct Canvas<'a> {
    inner: &'a mut dyn RenderBackend,
    /// How many enclosing sections are hidden. A count rather than a flag, because
    /// sections nest: `/OC BDC` inside `/OC BDC` closes with two `EMC`s, and the first
    /// of them must not bring the page back.
    hidden: usize,
}

impl<'a> Canvas<'a> {
    pub fn new(inner: &'a mut dyn RenderBackend) -> Self {
        Self { inner, hidden: 0 }
    }

    /// Enters a section whose optional content group is off.
    pub fn hide(&mut self) {
        self.hidden = self.hidden.saturating_add(1);
    }

    /// Leaves one. Saturating, because a content stream may carry more `EMC`s than it
    /// opened sections and an underflow there would hide the rest of the page.
    pub fn reveal(&mut self) {
        self.hidden = self.hidden.saturating_sub(1);
    }

    /// How many hidden sections are open, so a nested content stream can be run with the
    /// depth it inherited and restored to it afterwards.
    pub fn hidden_depth(&self) -> usize {
        self.hidden
    }

    /// Restores a depth taken from [`Canvas::hidden_depth`].
    pub fn restore_hidden_depth(&mut self, depth: usize) {
        self.hidden = depth;
    }

    /// Whether marks reach the page.
    pub fn paints(&self) -> bool {
        self.hidden == 0
    }
}

impl RenderBackend for Canvas<'_> {
    // --- the five that put marks on the page ---------------------------------------

    fn fill_path(&mut self, path: &BezPath, color: &Color, rule: WindingRule) {
        if self.paints() {
            self.inner.fill_path(path, color, rule);
        }
    }

    fn stroke_path(&mut self, path: &BezPath, color: &Color, style: &StrokeStyle) {
        if self.paints() {
            self.inner.stroke_path(path, color, style);
        }
    }

    fn paint_shading(&mut self, shading: &ShadingSpec) {
        if self.paints() {
            self.inner.paint_shading(shading);
        }
    }

    // --- the soft-mask bracket ------------------------------------------------------
    //
    // **Forwarded unconditionally, and not behind `paints()`.** The three are a bracket:
    // withholding the opening call while the closing one goes through would leave the
    // backend's layer stack unbalanced, and a mask is not a mark on the page — an
    // optional-content section that is off withholds what the mask covers, which is
    // already handled by the calls that draw.
    //
    // These forward at all because the trait defaults them to nothing. A wrapper that
    // inherits a default silently drops what it was meant to pass on, which is what
    // happened here: the interpreter emitted the bracket, `Canvas` swallowed it, and the
    // test that asked for the sequence saw an empty list.

    fn begin_masked_content(&mut self) {
        self.inner.begin_masked_content();
    }

    fn begin_soft_mask(&mut self, spec: &fepdf_model::graphics::SoftMaskSpec) {
        self.inner.begin_soft_mask(spec);
    }

    fn end_soft_mask(&mut self) {
        self.inner.end_soft_mask();
    }

    fn draw_image(
        &mut self,
        image: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
        smask: Option<SMaskData>,
    ) {
        if self.paints() {
            self.inner.draw_image(image, width, height, format, smask);
        }
    }

    fn show_text(
        &mut self,
        glyphs: &[TextGlyph],
        size: f64,
        transform: Affine,
        state: TextState,
        op_index: usize,
    ) {
        if self.paints() {
            self.inner.show_text(glyphs, size, transform, state, op_index);
        }
    }

    // --- everything else: state, which a hidden section still changes ----------------

    fn transform(&mut self, transform: Affine) {
        self.inner.transform(transform);
    }

    fn set_transform(&mut self, transform: Affine) {
        self.inner.set_transform(transform);
    }

    fn push_state(&mut self) {
        self.inner.push_state();
    }

    fn pop_state(&mut self) {
        self.inner.pop_state();
    }

    fn push_clip(&mut self, path: &BezPath, rule: WindingRule) {
        self.inner.push_clip(path, rule);
    }

    fn pop_clip(&mut self) {
        self.inner.pop_clip();
    }

    fn set_fill_alpha(&mut self, alpha: f64) {
        self.inner.set_fill_alpha(alpha);
    }

    fn set_stroke_alpha(&mut self, alpha: f64) {
        self.inner.set_stroke_alpha(alpha);
    }

    fn set_fill_color(&mut self, color: Color) {
        self.inner.set_fill_color(color);
    }

    fn set_stroke_color(&mut self, color: Color) {
        self.inner.set_stroke_color(color);
    }

    fn set_fill_paint(&mut self, paint: &Paint) {
        self.inner.set_fill_paint(paint);
    }

    fn set_stroke_paint(&mut self, paint: &Paint) {
        self.inner.set_stroke_paint(paint);
    }

    fn set_blend_mode(&mut self, mode: BlendMode) {
        self.inner.set_blend_mode(mode);
    }

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
    ) {
        self.inner.define_font(
            name,
            base_name,
            data,
            index,
            cid_to_gid_map,
            fallback_type,
            is_cid_keyed,
        );
    }

    /// Forwarded even while hidden: a hidden section's `EMC` still has to close whatever
    /// its `BDC` opened, and an unbalanced `end_actual_text` would swallow the rest of
    /// the page's text.
    fn begin_actual_text(&mut self, text: &str) {
        self.inner.begin_actual_text(text);
    }

    fn end_actual_text(&mut self) {
        self.inner.end_actual_text();
    }

    fn set_font(&mut self, name: &str) {
        self.inner.set_font(name);
    }

    fn set_text_render_mode(&mut self, mode: TextRenderingMode) {
        self.inner.set_text_render_mode(mode);
    }

    fn set_char_spacing(&mut self, spacing: f64) {
        self.inner.set_char_spacing(spacing);
    }

    fn set_word_spacing(&mut self, spacing: f64) {
        self.inner.set_word_spacing(spacing);
    }
}
