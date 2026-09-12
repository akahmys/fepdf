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
use kurbo::{Rect, Shape};
use std::collections::BTreeMap;

/// Where on the page each marked-content section drew (14.7.4.2).
///
/// **Why here and not at the painting sites.** A `/MCID`'s box is the union of what was
/// drawn between its `BDC` and its `EMC`, and "what was drawn" is the same five calls the
/// optional-content guard above already stands in front of. A sixth painting operator
/// that forgets this forgets the guard too, and the guard is the one nobody gets to
/// forget.
///
/// **Why default user space.** A structure element's rectangle is read back beside
/// `/BBox` (14.8.5.4.5), which 8.3.2.3 writes in default user space. The boxes arriving
/// here are in whatever space the caller's `initial_transform` established — device
/// pixels, for a render — so [`Canvas::new`] takes that matrix and stores its inverse to
/// map back. A consumer outside the interpreter cannot undo a transform it never saw.
#[derive(Debug, Default)]
struct Marks {
    /// The `/MCID`s of the open sections that declared one, outermost first.
    ///
    /// Every one of them gets the mark, not just the innermost: a `/P` whose spans are
    /// each their own `BDC` still has to come out with the paragraph's box, and a
    /// structure element that references only the outer id is the common shape.
    open: Vec<u32>,
    /// What each id has covered so far.
    seen: BTreeMap<u32, Rect>,
}

/// A [`RenderBackend`] that withholds marks while an optional-content section is off,
/// and remembers where the ones it passes on landed.
pub struct Canvas<'a> {
    inner: &'a mut dyn RenderBackend,
    /// How many enclosing sections are hidden. A count rather than a flag, because
    /// sections nest: `/OC BDC` inside `/OC BDC` closes with two `EMC`s, and the first
    /// of them must not bring the page back.
    hidden: usize,
    /// The matrix the interpreter last handed down, which is `initial × CTM`.
    device: Affine,
    /// `initial_transform` inverted, which takes a device box back to user space.
    to_user: Affine,
    /// The open sections and their boxes.
    marks: Marks,
}

impl<'a> Canvas<'a> {
    pub fn new(inner: &'a mut dyn RenderBackend, initial_transform: Affine) -> Self {
        Self {
            inner,
            hidden: 0,
            device: initial_transform,
            to_user: initial_transform.inverse(),
            marks: Marks::default(),
        }
    }

    /// Opens a marked-content section that declared a `/MCID`.
    pub fn open_mark(&mut self, mcid: u32) {
        self.marks.open.push(mcid);
    }

    /// Closes the innermost one. Saturating in the same sense as [`Canvas::reveal`]: a
    /// stream with more `EMC`s than `BDC`s must not take the enclosing section down.
    pub fn close_mark(&mut self) {
        self.marks.open.pop();
    }

    /// Takes the boxes, in default user space, leaving none behind.
    pub fn take_mark_bounds(&mut self) -> BTreeMap<u32, Rect> {
        std::mem::take(&mut self.marks.seen)
    }

    /// Records that `device` — a box in the space `set_transform` established — was
    /// covered by every open section.
    fn note(&mut self, device: Rect) {
        if self.marks.open.is_empty() {
            return;
        }
        let user = self.to_user.transform_rect_bbox(device);
        // A degenerate `initial_transform` inverts to infinities, and a content stream
        // may scale a path to one; either way a box that is not a number belongs to no
        // section. Checked on the way out rather than on the way in, because that is
        // where both causes arrive.
        if !user.is_finite() {
            return;
        }
        for mcid in &self.marks.open {
            self.marks
                .seen
                .entry(*mcid)
                .and_modify(|seen| *seen = seen.union(user))
                .or_insert(user);
        }
    }

    /// The box a path covers, in the current device space.
    fn path_box(&self, path: &BezPath) -> Rect {
        self.device.transform_rect_bbox(path.bounding_box())
    }

    /// The box a run of glyphs covers, in the current device space.
    ///
    /// **Exact across, approximate down.** The advances are the font's own, so the
    /// length of the run is the length the run has. Its height is not: no glyph metric
    /// reaches this layer — [`TextGlyph`] carries an advance and no bounding box, and the
    /// font program stops at [`RenderBackend::define_font`] — so the em is split at the
    /// baseline, a quarter below and three quarters above. A box derived here is
    /// therefore right about *which column and which line* and wrong by a few points
    /// about the ascender. `/FontBBox` (9.8.1) is what would make it exact, and reading
    /// it is a change to the trait rather than to this function.
    fn text_box(&self, glyphs: &[TextGlyph], size: f64, transform: Affine, vertical: bool) -> Rect {
        let em = size.abs();
        let run = if vertical {
            let down: f64 = glyphs.iter().map(|g| f64::from(g.vy).abs()).sum::<f64>() / 1000.0 * em;
            Rect::new(-em / 2.0, 0.0, em / 2.0, -down)
        } else {
            let across: f64 = glyphs.iter().map(|g| f64::from(g.width)).sum::<f64>() / 1000.0 * em;
            Rect::new(0.0, -em / 4.0, across, em * 0.75)
        };
        (self.device * transform).transform_rect_bbox(run.abs())
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
            self.note(self.path_box(path));
            self.inner.fill_path(path, color, rule);
        }
    }

    fn stroke_path(&mut self, path: &BezPath, color: &Color, style: &StrokeStyle) {
        if self.paints() {
            // The pen's width is not added: it is set in user space and this box is not,
            // and half a line width is below what a structure rectangle is read at.
            self.note(self.path_box(path));
            self.inner.stroke_path(path, color, style);
        }
    }

    /// `sh` contributes no box: 8.7.4.5.2 paints it across the current clip region, and
    /// the clip is the backend's to know — nothing about the region reaches here.
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
            // An image occupies the unit square of image space (8.9.5.2); the CTM is what
            // gives it a size and a place.
            self.note(self.device.transform_rect_bbox(Rect::new(0.0, 0.0, 1.0, 1.0)));
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
            self.note(self.text_box(glyphs, size, transform, state.is_vertical));
            self.inner.show_text(glyphs, size, transform, state, op_index);
        }
    }

    // --- everything else: state, which a hidden section still changes ----------------

    fn receive_mark_bounds(&mut self, bounds: BTreeMap<u32, Rect>) {
        self.inner.receive_mark_bounds(bounds);
    }

    fn transform(&mut self, transform: Affine) {
        self.device *= transform;
        self.inner.transform(transform);
    }

    fn set_transform(&mut self, transform: Affine) {
        self.device = transform;
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
