//! One `RenderBackend` for the workspace's tests and examples.
//!
//! **It was thirteen hand-written implementations on 2026-09-08**, 725 lines of which 241
//! were methods written only to say nothing. Each one observed between two and eight of
//! the trait's twenty required methods and stubbed the rest, so adding a method to
//! `RenderBackend` meant editing thirteen files to write `{}` twelve more times.
//!
//! It lived under `crates/fepdf/tests/` first, which left the thirteenth out:
//! `crates/fepdf/examples/glyph_loss.rs` cannot declare a module under `tests/`.
//!
//! This records every call as an ordered [`Event`] instead. A test reads the events it
//! cares about and ignores the others, which is what each of the thirteen was doing by
//! hand.
//!
//! Two things this deliberately does *not* do:
//!
//! - It does not override `set_fill_paint` or `set_stroke_paint`. Every one of the
//!   thirteen did, with an empty body, which suppresses the trait's own delegation to
//!   `set_fill_color`. No fixture took the `scn` path, so nothing was wrong today — but a
//!   recorder that counts fill colours and silently drops the ones arriving as a `Paint`
//!   is a trap, and leaving the default in place removes it for every caller at once.
//! - It does not become a default implementation on `RenderBackend` itself. A real
//!   backend that forgets `fill_path` must not compile; a trait default would let it
//!   draw nothing instead.

use std::sync::Arc;

use fepdf_content::{
    BlendMode, Color, FallbackFontType, PixelFormat, RenderBackend, SMaskData, ShadingSpec,
    SoftMaskSpec, StrokeStyle, TextGlyph, TextState, WindingRule,
};
use fepdf_model::graphics::TextRenderingMode;
use kurbo::{Affine, BezPath, Rect, Shape};

/// One call the interpreter made, in the order it made it.
#[derive(Debug, Clone)]
pub enum Event {
    /// `cm` — the matrix concatenated, not the resulting CTM.
    Transform(Affine),
    /// The CTM replaced outright, as `render_page` does before a page's content.
    SetTransform(Affine),
    /// `q`.
    PushState,
    /// `Q`.
    PopState,
    /// A filled path.
    Fill {
        /// The path, in user space.
        path: BezPath,
        /// The fill colour in force.
        color: Color,
        /// Nonzero or even-odd.
        rule: WindingRule,
        /// The CTM in force when the path arrived — the page position is `ctm * path`.
        ctm: Affine,
    },
    /// A stroked path.
    Stroke {
        /// The path, in user space.
        path: BezPath,
        /// The stroke colour in force.
        color: Color,
        /// The pen: width, caps, joins, dashes.
        style: StrokeStyle,
        /// The CTM in force when the path arrived.
        ctm: Affine,
    },
    /// The clip region intersected with a path (`W`, `W*`).
    PushClip {
        /// The clip path, in user space.
        path: BezPath,
        /// Nonzero or even-odd.
        rule: WindingRule,
        /// The CTM in force when the path arrived.
        ctm: Affine,
    },
    /// The clip region restored.
    PopClip,
    /// `/ca`.
    FillAlpha(f64),
    /// `/CA`.
    StrokeAlpha(f64),
    /// The fill colour set — including through a solid `Paint`, which the trait's own
    /// default delegates here.
    FillColor(Color),
    /// The stroke colour set, by the same two routes.
    StrokeColor(Color),
    /// `sh` — a shading painted across the clip region.
    Shading(ShadingSpec),
    /// The opening of a soft-mask bracket; what follows is the content to be masked.
    BeginMaskedContent,
    /// The mask's own drawing begins, under this specification.
    BeginSoftMask(SoftMaskSpec),
    /// The bracket closes and the mask applies.
    EndSoftMask,
    /// `/BM`.
    Blend(BlendMode),
    /// A decoded image.
    Image {
        /// The decoded samples.
        samples: Vec<u8>,
        /// Width in samples.
        width: u32,
        /// Height in samples.
        height: u32,
        /// How the samples are laid out.
        format: PixelFormat,
        /// The soft mask handed down with it, when there is one.
        smask: Option<SMaskData>,
    },
    /// A font registered for later selection.
    Font {
        /// The resource name it was registered under.
        name: String,
        /// `/BaseFont`, when the dictionary carried one.
        base: Option<String>,
        /// Which bundled font stands in if the embedded program is unusable.
        fallback: FallbackFontType,
        /// Whether codes are CIDs rather than single bytes.
        cid_keyed: bool,
    },
    /// A marked-content section declaring the text it stands for (14.9.4).
    BeginActualText(String),
    /// That section closes.
    EndActualText,
    /// `Tf` — a previously defined font selected.
    SetFont(String),
    /// `Tr`.
    TextRenderMode(TextRenderingMode),
    /// `Tc`.
    CharSpacing(f64),
    /// `Tw`.
    WordSpacing(f64),
    /// A run of positioned glyphs.
    Text {
        /// The glyphs as the interpreter emitted them — each with the code it was drawn
        /// for, the name the encoding gave it, and which route found its character.
        ///
        /// **Kept whole rather than joined into a string.** A glyph that reached no
        /// character is the subject of `crates/fepdf/examples/glyph_loss.rs`, and joining
        /// the run throws away exactly the glyph it is counting.
        glyphs: Vec<TextGlyph>,
        /// The font size in force.
        size: f64,
        /// The text matrix the run was placed with.
        transform: Affine,
        /// The CTM in force when the run arrived — the page position is `ctm * transform`.
        ///
        /// **`Fill` and `Stroke` carried this and `Text` did not**, so a test reading a
        /// run's position off `transform` alone was right only on a page with no `cm`.
        /// `bokutokitan.pdf` opens with `1 0 0 1 72 249.02 cm`, and every run of it came
        /// back short by exactly that.
        ctm: Affine,
    },
}

impl Event {
    /// The operation's name alone, for asserting on the *order* of a sequence.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Transform(_) => "transform",
            Self::SetTransform(_) => "set_transform",
            Self::PushState => "push_state",
            Self::PopState => "pop_state",
            Self::Fill { .. } => "fill",
            Self::Stroke { .. } => "stroke",
            Self::PushClip { .. } => "push_clip",
            Self::PopClip => "pop_clip",
            Self::FillAlpha(_) => "fill_alpha",
            Self::StrokeAlpha(_) => "stroke_alpha",
            Self::FillColor(_) => "fill_color",
            Self::StrokeColor(_) => "stroke_color",
            Self::Shading(_) => "shading",
            Self::BeginMaskedContent => "begin_masked_content",
            Self::BeginSoftMask(_) => "begin_soft_mask",
            Self::EndSoftMask => "end_soft_mask",
            Self::Blend(_) => "blend",
            Self::Image { .. } => "image",
            Self::Font { .. } => "font",
            Self::BeginActualText(_) => "begin_actual_text",
            Self::EndActualText => "end_actual_text",
            Self::SetFont(_) => "set_font",
            Self::TextRenderMode(_) => "text_render_mode",
            Self::CharSpacing(_) => "char_spacing",
            Self::WordSpacing(_) => "word_spacing",
            Self::Text { .. } => "text",
        }
    }
}

/// What the page handed the backend for one image.
///
/// Borrowed from the [`Event`] rather than copied: the samples of a page-sized image are
/// the largest thing a fixture produces, and a test that asserts on two bytes of it should
/// not pay to clone the rest.
#[derive(Debug)]
pub struct ImageDrawn<'a> {
    /// The decoded samples.
    pub samples: &'a [u8],
    /// Width and height in samples.
    pub size: (u32, u32),
    /// How the samples are laid out.
    pub format: PixelFormat,
    /// The soft mask handed down with it, when there is one.
    pub smask: &'a Option<SMaskData>,
}

/// Records what the interpreter asked a backend to draw.
///
/// Carries the CTM so a test can ask where a path landed on the page. `q` and `Q` save and
/// restore it, as they do in a backend that draws.
#[derive(Debug, Default)]
pub struct Recorder {
    /// Every call, in the order the interpreter made it.
    pub events: Vec<Event>,
    ctm: Affine,
    saved: Vec<Affine>,
}

impl Recorder {
    /// A recorder with an identity CTM and nothing recorded yet.
    pub fn new() -> Self {
        Self { events: Vec::new(), ctm: Affine::IDENTITY, saved: Vec::new() }
    }

    /// The text of every run, concatenated — what the page reads as.
    pub fn text(&self) -> String {
        self.events
            .iter()
            .filter_map(|e| {
                if let Event::Text { glyphs, .. } = e {
                    Some(glyphs.iter().map(|g| g.unicode.as_str()).collect::<String>())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Each filled path's bounding box in user space, in order.
    pub fn fills(&self) -> Vec<Rect> {
        self.filled(|path, _| path.bounding_box())
    }

    /// Each filled path's bounding box on the page, with the CTM in force applied.
    pub fn device_fills(&self) -> Vec<Rect> {
        self.filled(|path, ctm| (ctm * path.clone()).bounding_box())
    }

    fn filled(&self, shape: impl Fn(&BezPath, Affine) -> Rect) -> Vec<Rect> {
        self.events
            .iter()
            .filter_map(|e| {
                if let Event::Fill { path, ctm, .. } = e { Some(shape(path, *ctm)) } else { None }
            })
            .collect()
    }

    /// Where each run of glyphs was placed on the page, in the order the page drew them.
    ///
    /// **This is how a test sees a run's position**, which extraction cannot always give:
    /// the span collector does not honour word or character spacing, so a page whose
    /// placement depends on `Tw` or `Tc` reads the same either way through it.
    ///
    /// Named `device_` for the same reason [`Self::device_fills`] is: the CTM in force is
    /// applied, so this is where the ink lands rather than where the text object put it.
    pub fn device_text_origins(&self) -> Vec<(f64, f64)> {
        self.events
            .iter()
            .filter_map(|e| {
                if let Event::Text { transform, ctm, .. } = e {
                    let coeffs = (*ctm * *transform).as_coeffs();
                    Some((coeffs[4], coeffs[5]))
                } else {
                    None
                }
            })
            .collect()
    }

    /// The matrix each run of glyphs was placed by, with the CTM in force applied.
    ///
    /// [`Self::device_text_origins`] answers where a run starts and says nothing about
    /// how it is set. A test that a move keeps a run's scale or rotation needs the rest
    /// of the matrix, and dropping those from a moved run shifts no origin at all.
    pub fn device_text_matrices(&self) -> Vec<Affine> {
        self.events
            .iter()
            .filter_map(|e| {
                if let Event::Text { transform, ctm, .. } = e {
                    Some(*ctm * *transform)
                } else {
                    None
                }
            })
            .collect()
    }

    /// What each run of glyphs advances by, as the interpreter measured it.
    ///
    /// The unit is the glyph's own: `TextGlyph::width`, summed over the run. A test that
    /// wants a page-space distance has to scale it the way the run is scaled.
    pub fn text_advances(&self) -> Vec<f64> {
        self.events
            .iter()
            .filter_map(|e| {
                if let Event::Text { glyphs, .. } = e {
                    Some(glyphs.iter().map(|g| f64::from(g.width)).sum())
                } else {
                    None
                }
            })
            .collect()
    }

    /// The last image the page drew, when it drew one.
    pub fn last_image(&self) -> Option<ImageDrawn<'_>> {
        self.events.iter().rev().find_map(|e| {
            let Event::Image { samples, width, height, format, smask } = e else {
                return None;
            };
            Some(ImageDrawn { samples, size: (*width, *height), format: *format, smask })
        })
    }

    /// How many events of this name were recorded.
    pub fn count(&self, name: &str) -> usize {
        self.events.iter().filter(|e| e.name() == name).count()
    }
}

impl RenderBackend for Recorder {
    fn transform(&mut self, transform: Affine) {
        self.ctm *= transform;
        self.events.push(Event::Transform(transform));
    }

    fn set_transform(&mut self, transform: Affine) {
        self.ctm = transform;
        self.events.push(Event::SetTransform(transform));
    }

    fn push_state(&mut self) {
        self.saved.push(self.ctm);
        self.events.push(Event::PushState);
    }

    fn pop_state(&mut self) {
        if let Some(previous) = self.saved.pop() {
            self.ctm = previous;
        }
        self.events.push(Event::PopState);
    }

    fn fill_path(&mut self, path: &BezPath, color: &Color, rule: WindingRule) {
        self.events.push(Event::Fill { path: path.clone(), color: *color, rule, ctm: self.ctm });
    }

    fn stroke_path(&mut self, path: &BezPath, color: &Color, style: &StrokeStyle) {
        self.events.push(Event::Stroke {
            path: path.clone(),
            color: *color,
            style: style.clone(),
            ctm: self.ctm,
        });
    }

    fn push_clip(&mut self, path: &BezPath, rule: WindingRule) {
        self.events.push(Event::PushClip { path: path.clone(), rule, ctm: self.ctm });
    }

    fn pop_clip(&mut self) {
        self.events.push(Event::PopClip);
    }

    fn set_fill_alpha(&mut self, alpha: f64) {
        self.events.push(Event::FillAlpha(alpha));
    }

    fn set_stroke_alpha(&mut self, alpha: f64) {
        self.events.push(Event::StrokeAlpha(alpha));
    }

    fn set_fill_color(&mut self, color: Color) {
        self.events.push(Event::FillColor(color));
    }

    fn set_stroke_color(&mut self, color: Color) {
        self.events.push(Event::StrokeColor(color));
    }

    fn paint_shading(&mut self, shading: &ShadingSpec) {
        self.events.push(Event::Shading(shading.clone()));
    }

    fn begin_masked_content(&mut self) {
        self.events.push(Event::BeginMaskedContent);
    }

    fn begin_soft_mask(&mut self, spec: &SoftMaskSpec) {
        self.events.push(Event::BeginSoftMask(spec.clone()));
    }

    fn end_soft_mask(&mut self) {
        self.events.push(Event::EndSoftMask);
    }

    fn set_blend_mode(&mut self, mode: BlendMode) {
        self.events.push(Event::Blend(mode));
    }

    fn draw_image(
        &mut self,
        image: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
        smask: Option<SMaskData>,
    ) {
        self.events.push(Event::Image { samples: image.to_vec(), width, height, format, smask });
    }

    #[allow(clippy::too_many_arguments)]
    fn define_font(
        &mut self,
        name: &str,
        base_name: Option<&str>,
        _data: Option<Arc<Vec<u8>>>,
        _index: Option<usize>,
        _cid_to_gid_map: Option<std::collections::BTreeMap<u32, u32>>,
        fallback_type: FallbackFontType,
        is_cid_keyed: bool,
    ) {
        self.events.push(Event::Font {
            name: name.to_string(),
            base: base_name.map(str::to_string),
            fallback: fallback_type,
            cid_keyed: is_cid_keyed,
        });
    }

    fn begin_actual_text(&mut self, text: &str) {
        self.events.push(Event::BeginActualText(text.to_string()));
    }

    fn end_actual_text(&mut self) {
        self.events.push(Event::EndActualText);
    }

    fn set_font(&mut self, name: &str) {
        self.events.push(Event::SetFont(name.to_string()));
    }

    fn set_text_render_mode(&mut self, mode: TextRenderingMode) {
        self.events.push(Event::TextRenderMode(mode));
    }

    fn set_char_spacing(&mut self, spacing: f64) {
        self.events.push(Event::CharSpacing(spacing));
    }

    fn set_word_spacing(&mut self, spacing: f64) {
        self.events.push(Event::WordSpacing(spacing));
    }

    fn show_text(
        &mut self,
        glyphs: &[TextGlyph],
        size: f64,
        transform: Affine,
        _state: TextState,
        _op_index: usize,
    ) {
        self.events.push(Event::Text { glyphs: glyphs.to_vec(), size, transform, ctm: self.ctm });
    }
}
