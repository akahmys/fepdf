//! A Vello/wgpu implementation of the `fepdf-content` backend contract.
//!
//! This crate implements; it does not define. The trait and the values crossing it
//! live in `fepdf-content` (ARCHITECTURE.md Rule B), so consumers that only interpret
//! content streams never link a GPU stack.

// Colour components arrive as f64 in [0,1] and leave as u8 in [0,255]; page and
// texture dimensions arrive as f64 user-space units and leave as u32 pixels. Both
// narrowings are clamped at the call site and are what rasterisation *is* — they
// occur at roughly forty places, so they are suppressed here rather than annotated
// individually.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless
)]
// Glyph and colour maths name components after the standard that defines them
// (c/m/y/k, x/y, r/g/b); expanding those hurts rather than helps.
#![allow(clippy::many_single_char_names)]
// Glyph helpers take `&mut SkrifaBridge` alongside the cache-writing paths they
// sit with; narrowing the ones that happen not to write today would flip back
// as soon as caching is added to them.
#![allow(clippy::needless_pass_by_ref_mut)]

pub mod headless;
pub mod text;

use fepdf_model::graphics::TextRenderingMode;
use fepdf_model::{
    BlendMode, Color, LineCap, LineJoin, Paint, PatternSpec, PixelFormat, ShadingSpec, StrokeStyle,
};
use kurbo::{Affine, BezPath, Cap, Join, Stroke};
use std::sync::Arc;
use vello::Scene;
use vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

// The contract this crate implements. Re-exported so existing callers keep working.
pub use fepdf_content::{
    FallbackFontType, RenderBackend, SMaskData, TextGlyph, TextState, WindingRule, path,
};

/// A [`RenderBackend`] that builds a Vello scene for GPU rasterisation.
pub struct VelloBackend {
    scene: Scene,
    state: VelloState,
    state_stack: Vec<VelloState>,
    font_cache: std::collections::BTreeMap<String, FontCacheEntry>,
    system_fonts: Arc<std::collections::BTreeMap<FallbackFontType, Arc<Vec<u8>>>>,
    skrifa_bridge: crate::text::SkrifaBridge,
    next_font_id: u64,
    /// Conclusions about the document reached while drawing it, drained by
    /// `render_page` (ARCHITECTURE §4.3). A backend sits below any `Document`, so it
    /// accumulates rather than records.
    decisions: Vec<fepdf_model::interpretation::Decision>,
    /// Open soft-mask brackets (11.6.5.2), and whether each one's mask is being honoured.
    ///
    /// A bracket is two Vello layers — one holding the content, one holding the mask —
    /// and `end_soft_mask` pops both whether or not the mask could be applied, so this
    /// exists to keep that count right rather than to remember anything about the mask.
    mask_stack: Vec<bool>,
}

#[derive(Clone)]
struct VelloState {
    transform: Affine,
    fill_color: Color,
    stroke_color: Color,
    fill_paint: Option<Paint>,
    stroke_paint: Option<Paint>,
    fill_alpha: f64,
    stroke_alpha: f64,
    blend_mode: BlendMode,
    clip_count: u32,
    font_data: Option<Arc<Vec<u8>>>,
    font_index: Option<usize>,
    cid_to_gid_map: Option<std::collections::BTreeMap<u32, u32>>,
    text_render_mode: i32,
    char_spacing: f64,
    word_spacing: f64,
    font_name: Option<String>,
    is_cid_keyed: bool,
    font_id: u64,
    is_fallback: bool,
    fallback_type: FallbackFontType,
}

struct FontCacheEntry {
    font_id: u64,
    data: Option<Arc<Vec<u8>>>,
    collection_index: Option<usize>,
    cid_to_gid_map: Option<std::collections::BTreeMap<u32, u32>>,
    base_name: Option<String>,
    fallback_type: FallbackFontType,
    is_cid_keyed: bool,
}

impl VelloBackend {
    /// Loads the bundled fallback fonts from the configured resource directory.
    pub fn load_system_fonts() -> Arc<std::collections::BTreeMap<FallbackFontType, Arc<Vec<u8>>>> {
        let mut fonts = std::collections::BTreeMap::new();
        // The same root and layout the model's loader uses
        // (`fepdf_model::resources`). The two disagreed once — `assets` here against
        // `resources` there — and this copy is the one with no platform fallback under
        // it, so the wrong default left the map empty outright for three months.
        let Some(base_path) =
            fepdf_model::resources::locate(fepdf_model::resources::Resource::Fonts)
        else {
            return Arc::new(fonts);
        };

        let mappings = [
            (FallbackFontType::Serif, "serif.ttf"),
            (FallbackFontType::SansSerif, "sans.ttf"),
            (FallbackFontType::Monospace, "mono.ttf"),
            (FallbackFontType::JapaneseSerif, "mincho.ttf"),
            (FallbackFontType::JapaneseSans, "gothic.ttf"),
        ];

        for (ftype, filename) in mappings {
            let path = base_path.join(filename);
            match std::fs::read(&path) {
                Ok(data) => {
                    let data = Arc::new(data);
                    // A face for "no preference" as well as for the shape it names, so a
                    // font resource that infers `Default` finds something. Nothing
                    // populated that key anywhere until Phase P.
                    if ftype == FallbackFontType::SansSerif {
                        fonts.insert(FallbackFontType::Default, Arc::clone(&data));
                    }
                    fonts.insert(ftype, data);
                }
                Err(e) => {
                    log::warn!(
                        "[RENDER] Failed to load system fallback font {:?} from {}: {:?}",
                        ftype,
                        path.display(),
                        e
                    );
                }
            }
        }
        Arc::new(fonts)
    }

    /// Creates a backend that draws with the supplied fallback fonts.
    pub fn new(
        system_fonts: Arc<std::collections::BTreeMap<FallbackFontType, Arc<Vec<u8>>>>,
    ) -> Self {
        Self {
            scene: Scene::new(),
            state: VelloState {
                transform: Affine::IDENTITY,
                fill_color: Color::Gray(0.0),
                stroke_color: Color::Gray(0.0),
                fill_paint: None,
                stroke_paint: None,
                fill_alpha: 1.0,
                stroke_alpha: 1.0,
                blend_mode: BlendMode::Normal,
                clip_count: 0,
                font_data: None,
                font_index: None,
                cid_to_gid_map: None,
                text_render_mode: 0,
                char_spacing: 0.0,
                word_spacing: 0.0,
                font_name: None,
                is_cid_keyed: false,
                font_id: 0,
                is_fallback: false,
                fallback_type: FallbackFontType::Default,
            },
            state_stack: Vec::new(),
            font_cache: std::collections::BTreeMap::new(),
            system_fonts,
            skrifa_bridge: crate::text::SkrifaBridge::new(),
            next_font_id: 1,
            decisions: Vec::new(),
            mask_stack: Vec::new(),
        }
    }

    /// Borrows the scene accumulated so far.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Renders a single glyph to the Vello scene.
    ///
    /// Handles both horizontal and vertical writing modes, correctly interpreting
    /// signed vertical advances (where negative moves characters DOWN).
    fn resolve_font_and_glyph<'a>(
        system_fonts: &'a Arc<std::collections::BTreeMap<FallbackFontType, Arc<Vec<u8>>>>,
        state: &VelloState,
        glyph: &TextGlyph,
        ctx: &GlyphRenderContext<'a>,
    ) -> (bool, bool, &'a [u8], bool, u32) {
        let is_cid = state.is_cid_keyed;
        let is_japanese = state.font_name.as_ref().is_some_and(|n| {
            let lower = n.to_lowercase();
            lower.contains("mincho")
                || lower.contains("gothic")
                || lower.contains("hira")
                || lower.contains("koz")
                || n.contains("明朝")
                || n.contains("ゴシック")
                || is_cid
        });

        let mut font_data = ctx.data_ref;
        let is_space = glyph.unicode == " " || glyph.unicode == "\u{3000}";

        let mut is_fallback = state.is_fallback || glyph.is_fallback;
        let mut gid = glyph.gid;

        if (glyph.is_fallback || (is_fallback && is_space)) && !ctx.data_ref.is_empty() && gid == 0
        {
            let _ = system_fonts.get(&state.fallback_type).map(|sys_data| {
                font_data = sys_data;
                is_fallback = true;
            });
        } else if glyph.is_fallback {
            let _ = system_fonts.get(&state.fallback_type).map(|sys_data| {
                font_data = sys_data;
                gid = 0;
                is_fallback = true;
            });
        }
        (is_cid, is_japanese, font_data, is_fallback, gid)
    }

    fn calculate_glyph_transform(
        skrifa_bridge: &mut crate::text::SkrifaBridge,
        font_data: &[u8],
        glyph: &TextGlyph,
        ctx: &GlyphRenderContext,
    ) -> Affine {
        let upem = skrifa_bridge.get_units_per_em(font_data).unwrap_or(1000);
        let scale = ctx.size / f64::from(upem);

        let h_scale = if ctx.is_vertical { 1.0 } else { ctx.th };
        let v_scale = if ctx.is_vertical { ctx.th } else { 1.0 };

        let local_to_pt = Affine::scale_non_uniform(scale * h_scale, scale * v_scale)
            * Affine::translate(kurbo::Vec2::new(f64::from(-glyph.vx), f64::from(-glyph.vy)));

        let adv_vec = if ctx.is_vertical {
            kurbo::Vec2::new(0.0, ctx.advance_offset)
        } else {
            kurbo::Vec2::new(ctx.advance_offset, 0.0)
        };

        ctx.transform * Affine::translate(adv_vec) * local_to_pt
    }

    /// Renders a single glyph to the Vello scene.
    ///
    /// Handles both horizontal and vertical writing modes, correctly interpreting
    /// signed vertical advances (where negative moves characters DOWN).
    #[allow(clippy::collapsible_if)]
    fn render_single_glyph(
        scene: &mut Scene,
        skrifa_bridge: &mut crate::text::SkrifaBridge,
        system_fonts: &Arc<std::collections::BTreeMap<FallbackFontType, Arc<Vec<u8>>>>,
        state: &VelloState,
        glyph: &TextGlyph,
        ctx: &GlyphRenderContext,
    ) -> (f64, bool) {
        let (is_cid, is_japanese, font_data, is_fallback, gid) =
            Self::resolve_font_and_glyph(system_fonts, state, glyph, ctx);

        let skrifa_ctx = crate::text::GlyphExtractionContext {
            font_id: state.font_id,
            data: font_data,
            gid,
            char_code: glyph.char_code,
            cid_to_gid_map: state.cid_to_gid_map.as_ref(),
            is_vertical: ctx.is_vertical,
            unicode_fallback: glyph.unicode.chars().next(),
            is_japanese,
            is_cid,
            collection_index: state.font_index.unwrap_or(0) as u32,
            is_fallback,
        };

        let next_advance = Self::calculate_next_advance(
            glyph,
            ctx.size,
            ctx.advance_offset,
            ctx.tc,
            ctx.tw,
            ctx.th,
            ctx.is_vertical,
        );

        if let Some(path) = skrifa_bridge.extract_path(&skrifa_ctx) {
            let t = Self::calculate_glyph_transform(skrifa_bridge, font_data, glyph, ctx);
            scene.fill(vello::peniko::Fill::NonZero, t, ctx.brush, None, &path);
            (next_advance, true)
        } else {
            (next_advance, false)
        }
    }

    /// Calculates the next cumulative advance after rendering a glyph.
    ///
    /// For vertical writing mode, positive character/word spacing is subtracted
    /// from the natively negative vertical advance to move characters further DOWN.
    fn calculate_next_advance(
        glyph: &TextGlyph,
        size: f64,
        current_advance: f64,
        tc: f64,
        tw: f64,
        th: f64,
        is_vertical: bool,
    ) -> f64 {
        let char_width = f64::from(glyph.width) / 1000.0 * size;
        let advance = if !is_vertical {
            let mut adv = (char_width + tc) * th;
            if glyph.char_code == 0x20 {
                adv = tw.mul_add(th, adv);
            }
            adv
        } else {
            // In vertical writing mode, Tz (th) applies to the y dimension.
            // Spacing Tc and Tw are subtracted from the natively negative vertical advance.
            let mut adv = (char_width * th) - tc;
            if glyph.char_code == 0x20 {
                adv -= tw;
            }
            adv
        };
        current_advance + advance
    }
}

struct GlyphRenderContext<'a> {
    size: f64,
    transform: Affine,
    tc: f64,
    tw: f64,
    th: f64,
    is_vertical: bool,
    advance_offset: f64,
    data_ref: &'a [u8],
    brush: &'a vello::peniko::Brush,
}

fn convert_image_pixels(
    fill_color: &Color,
    fill_alpha: f64,
    image_data: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
) -> Vec<u8> {
    match format {
        PixelFormat::Rgba8 => image_data.to_vec(),
        PixelFormat::Gray8 => convert_gray8(image_data),
        PixelFormat::Rgb8 => convert_rgb8(image_data),
        PixelFormat::Cmyk8 => convert_cmyk8(image_data),
        PixelFormat::MonoMask => {
            convert_mono_mask(fill_color, fill_alpha, image_data, width, height, false)
        }
        PixelFormat::MonoMaskInverted => {
            convert_mono_mask(fill_color, fill_alpha, image_data, width, height, true)
        }
    }
}

fn convert_gray8(image_data: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(image_data.len() * 4);
    for &g in image_data {
        data.extend_from_slice(&[g, g, g, 255]);
    }
    data
}

fn convert_rgb8(image_data: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(image_data.len() / 3 * 4);
    for chunk in image_data.as_chunks::<3>().0 {
        data.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
    }
    data
}

fn convert_cmyk8(image_data: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(image_data.len() / 4 * 4);
    for chunk in image_data.as_chunks::<4>().0 {
        let c = f64::from(chunk[0]) / 255.0;
        let m = f64::from(chunk[1]) / 255.0;
        let y = f64::from(chunk[2]) / 255.0;
        let k = f64::from(chunk[3]) / 255.0;
        // 10.4.2.5, through the one implementation of it (`Color::to_rgb`), so a
        // CMYK image and a CMYK fill cannot drift apart.
        let Color::Rgb(r, g, b) = Color::Cmyk(c, m, y, k).to_rgb() else {
            continue;
        };
        let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0) as u8;
        data.extend_from_slice(&[byte(r), byte(g), byte(b), 255]);
    }
    data
}

fn convert_mono_mask(
    fill_color: &Color,
    fill_alpha: f64,
    image_data: &[u8],
    width: u32,
    height: u32,
    inverted: bool,
) -> Vec<u8> {
    let fill_rgb = fill_color.to_rgb();
    let (r, g, b) = match fill_rgb {
        Color::Rgb(rv, gv, bv) => (
            (rv * 255.0).clamp(0.0, 255.0) as u8,
            (gv * 255.0).clamp(0.0, 255.0) as u8,
            (bv * 255.0).clamp(0.0, 255.0) as u8,
        ),
        // `to_rgb` always yields `Rgb`; the remaining arms exist so that adding a
        // colour space forces this conversion to be revisited rather than silently
        // painting black.
        Color::Gray(_) | Color::Cmyk(..) | Color::Lab(..) => (0, 0, 0),
    };
    let alpha = (fill_alpha * 255.0).clamp(0.0, 255.0) as u8;

    let bytes_per_row = (width as usize).div_ceil(8);
    let mut data = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        let row_start = y as usize * bytes_per_row;
        for x in 0..width {
            let byte_idx = row_start + (x as usize / 8);
            let bit_idx = 7 - (x as usize % 8);
            let byte_val = image_data.get(byte_idx).copied().unwrap_or(0);
            let bit = (byte_val >> bit_idx) & 1;

            let condition = if inverted { bit == 0 } else { bit == 1 };
            if condition {
                data.extend_from_slice(&[r, g, b, alpha]);
            } else {
                data.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    data
}

/// Multiplies the image's alpha by a soft mask (8.9.5.4).
///
/// **The mask is scaled to the image**, which the clause requires and this did not do:
/// it was applied only when the two agreed on both dimensions, and skipped in silence
/// otherwise. A soft mask smaller than the image it masks is not an edge case — it is
/// how producers keep the file small — so the common shape was the one that went
/// missing, and a half-transparent logo came out opaque with nothing said.
///
/// Nearest neighbour, deliberately. The alternative is interpolating alpha, which
/// invents coverage the file did not state and softens the very edges a mask exists to
/// make sharp.
///
/// Every read of `mask.data` is bounds-checked. A mask whose stream is shorter than its
/// own `/Width × /Height` is a malformed file, not a reason to panic in a renderer.
fn apply_image_smask(rgba_data: &mut [u8], width: u32, height: u32, mask: &SMaskData) {
    if mask.width == 0 || mask.height == 0 || width == 0 || height == 0 {
        return;
    }
    for (i, chunk) in rgba_data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (i as u32 % width, i as u32 / width);
        if y >= height {
            break;
        }
        // Nearest neighbour: the sample this pixel's centre falls on.
        let mx = (x * mask.width / width).min(mask.width - 1) as usize;
        let my = (y * mask.height / height).min(mask.height - 1) as usize;
        let at = my * mask.width as usize + mx;
        let sample = |n: usize| mask.data.get(n).copied().unwrap_or(255);

        let mask_val = match mask.format {
            PixelFormat::Gray8 => sample(at),
            PixelFormat::Rgba8 => sample(at * 4 + 3),
            PixelFormat::Rgb8 => {
                let r = f64::from(sample(at * 3));
                let g = f64::from(sample(at * 3 + 1));
                let b = f64::from(sample(at * 3 + 2));
                0.114f64.mul_add(b, 0.587f64.mul_add(g, 0.299 * r)) as u8
            }
            // Formats carrying no usable alpha channel leave the pixel opaque.
            PixelFormat::Cmyk8 | PixelFormat::MonoMask | PixelFormat::MonoMaskInverted => 255,
        };
        chunk[3] = ((f64::from(chunk[3]) * f64::from(mask_val)) / 255.0) as u8;
    }
}

fn has_move_to(path: &BezPath) -> bool {
    path.elements().iter().any(|el| matches!(el, kurbo::PathEl::MoveTo(_)))
}

impl RenderBackend for VelloBackend {
    /// Both halves: what this backend concluded, and what the glyph bridge under it did.
    fn begin_masked_content(&mut self) {
        // The content goes into a layer of its own so that the mask, which arrives after
        // it, has something bounded to apply to. `Mix::Normal` and alpha 1 leave the
        // compositing alone: this layer exists for the mask and changes nothing by itself.
        self.scene.push_layer(
            vello::peniko::Fill::NonZero,
            vello::peniko::Mix::Normal,
            1.0f32,
            Affine::IDENTITY,
            &UNBOUNDED,
        );
    }

    fn begin_soft_mask(&mut self, spec: &fepdf_content::SoftMaskSpec) {
        // Vello takes a luminance mask and nothing else, so the plain case is exact and
        // the rest are not expressible here at all. **The group is still swallowed in
        // both branches**: a mask group draws the mask, never the page, and letting its
        // marks through because the mask could not be applied would put a black rectangle
        // where a document asked for a gradient.
        if spec.is_plain_luminosity() {
            self.scene.push_luminance_mask_layer(
                vello::peniko::Fill::NonZero,
                1.0f32,
                Affine::IDENTITY,
                &UNBOUNDED,
            );
            self.mask_stack.push(true);
            return;
        }
        self.decisions.push(fepdf_model::interpretation::Decision::violation(
            "11.6.5.2",
            format!("a soft mask this renderer cannot express: {}", describe(spec)),
            "drew the content unmasked and discarded the group; Vello composites a              luminance mask and has no form for the other three, which need the mask              computed into a buffer first",
        ));
        self.scene.push_clip_layer(vello::peniko::Fill::NonZero, Affine::IDENTITY, &EMPTY);
        self.mask_stack.push(false);
    }

    fn end_soft_mask(&mut self) {
        // Two layers either way: the one holding the mask, then the one holding the
        // content it applies to.
        if self.mask_stack.pop().is_some() {
            self.scene.pop_layer();
        }
        self.scene.pop_layer();
    }

    fn take_decisions(&mut self) -> Vec<fepdf_model::interpretation::Decision> {
        let mut taken = std::mem::take(&mut self.decisions);
        taken.extend(self.skrifa_bridge.take_decisions());
        taken
    }

    fn transform(&mut self, transform: Affine) {
        self.state.transform *= transform;
    }
    fn set_transform(&mut self, transform: Affine) {
        self.state.transform = transform;
    }
    fn push_state(&mut self) {
        self.state_stack.push(self.state.clone());
    }
    fn pop_state(&mut self) {
        if let Some(s) = self.state_stack.pop() {
            self.state = s;
        }
    }

    fn fill_path(&mut self, path: &BezPath, color: &Color, rule: WindingRule) {
        if !has_move_to(path) {
            return;
        }
        let brush = if let Some(ref paint) = self.state.fill_paint {
            to_vello_paint_brush(paint, self.state.fill_alpha as f32)
        } else {
            to_vello_brush(color, self.state.fill_alpha as f32)
        };
        let vello_rule = match rule {
            WindingRule::NonZero => vello::peniko::Fill::NonZero,
            WindingRule::EvenOdd => vello::peniko::Fill::EvenOdd,
        };
        let mut closed_path = path.clone();
        closed_path.close_path();
        self.scene.fill(vello_rule, self.state.transform, &brush, None, &closed_path);
    }

    fn stroke_path(&mut self, path: &BezPath, color: &Color, style: &StrokeStyle) {
        if !has_move_to(path) {
            return;
        }
        let brush = if let Some(ref paint) = self.state.stroke_paint {
            to_vello_paint_brush(paint, self.state.stroke_alpha as f32)
        } else {
            to_vello_brush(color, self.state.stroke_alpha as f32)
        };
        let mut stroke = Stroke::new(style.width);
        let cap = match style.cap {
            LineCap::Butt => Cap::Butt,
            LineCap::Round => Cap::Round,
            LineCap::Square => Cap::Square,
        };
        stroke.start_cap = cap;
        stroke.end_cap = cap;
        stroke.join = match style.join {
            LineJoin::Miter => Join::Miter,
            LineJoin::Round => Join::Round,
            LineJoin::Bevel => Join::Bevel,
        };
        stroke.miter_limit = style.miter_limit;
        self.scene.stroke(&stroke, self.state.transform, &brush, None, path);
    }

    fn push_clip(&mut self, path: &BezPath, rule: WindingRule) {
        if !has_move_to(path) {
            return;
        }
        let vello_rule = match rule {
            WindingRule::NonZero => vello::peniko::Fill::NonZero,
            WindingRule::EvenOdd => vello::peniko::Fill::EvenOdd,
        };

        let mut closed_path = path.clone();
        closed_path.close_path();

        // `push_clip_layer` and not `push_layer`: a PDF clip is a clip, and vello has an
        // entry point for exactly that. The general one asks for a *blend* layer, which
        // costs blend memory per nesting level — the one resource `Scene::bump_estimate`
        // does not model, so nothing warns before the GPU runs out.
        self.scene.push_clip_layer(vello_rule, self.state.transform, &closed_path);
        self.state.clip_count += 1;
    }

    fn pop_clip(&mut self) {
        if self.state.clip_count > 0 {
            self.scene.pop_layer();
            self.state.clip_count -= 1;
        }
    }

    fn set_fill_alpha(&mut self, alpha: f64) {
        self.state.fill_alpha = alpha;
    }
    fn set_stroke_alpha(&mut self, alpha: f64) {
        self.state.stroke_alpha = alpha;
    }
    fn set_fill_color(&mut self, color: Color) {
        self.state.fill_color = color;
        self.state.fill_paint = None;
    }
    fn set_stroke_color(&mut self, color: Color) {
        self.state.stroke_color = color;
        self.state.stroke_paint = None;
    }
    fn set_fill_paint(&mut self, paint: &Paint) {
        self.state.fill_paint = Some(paint.clone());
        if let Paint::Solid(c) = paint {
            self.state.fill_color = *c;
        }
    }
    fn set_stroke_paint(&mut self, paint: &Paint) {
        self.state.stroke_paint = Some(paint.clone());
        if let Paint::Solid(c) = paint {
            self.state.stroke_color = *c;
        }
    }
    fn paint_shading(&mut self, shading: &ShadingSpec) {
        // A mesh is painted as itself rather than through a brush: Vello fills a path
        // with one colour, so a Gouraud triangle has no brush, and the model hands back
        // flat pieces small enough that the difference is under a pixel.
        if let ShadingSpec::Mesh(mesh) = shading {
            let alpha = self.state.fill_alpha as f32;
            let bleed = seam_bleed(self.state.transform);
            for triangle in mesh.flatten() {
                let brush = vello::peniko::Brush::Solid(to_peniko_color(&triangle.color, alpha));
                self.scene.fill(
                    vello::peniko::Fill::NonZero,
                    self.state.transform,
                    &brush,
                    None,
                    &grown_triangle(triangle.points, bleed),
                );
            }
            return;
        }
        let brush = to_vello_shading_brush(shading, self.state.fill_alpha as f32);
        let rect = kurbo::Rect::new(-10000.0, -10000.0, 10000.0, 10000.0);
        self.scene.fill(vello::peniko::Fill::NonZero, self.state.transform, &brush, None, &rect);
    }
    fn set_blend_mode(&mut self, mode: BlendMode) {
        self.state.blend_mode = mode;
    }

    fn draw_image(
        &mut self,
        image_data: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
        smask: Option<SMaskData>,
    ) {
        let mut rgba_data = convert_image_pixels(
            &self.state.fill_color,
            self.state.fill_alpha,
            image_data,
            width,
            height,
            format,
        );

        if let Some(mask) = &smask {
            apply_image_smask(&mut rgba_data, width, height, mask);
        }

        let image = ImageData {
            data: Blob::new(std::sync::Arc::new(rgba_data)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width,
            height,
        };

        let m = self.state.transform
            * Affine::translate(kurbo::Vec2::new(0.0, 1.0))
            * Affine::scale_non_uniform(1.0 / f64::from(width), -1.0 / f64::from(height));

        self.scene.draw_image(&image, m);
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
        log::debug!(
            "[RENDER] define_font: {} (id {}), has_data: {}, is_cid: {}, has_map: {}",
            name,
            self.next_font_id,
            data.is_some(),
            is_cid_keyed,
            cid_to_gid_map.is_some()
        );
        self.font_cache.insert(
            name.to_string(),
            FontCacheEntry {
                font_id: self.next_font_id,
                data,
                collection_index: index,
                cid_to_gid_map,
                is_cid_keyed,
                base_name: base_name.map(|s| s.to_string()),
                fallback_type,
            },
        );
        self.next_font_id += 1;
    }

    fn set_font(&mut self, name: &str) {
        if let Some(entry) = self.font_cache.get(name) {
            let is_fallback = entry.data.is_none();
            self.state.font_data = entry.data.clone().or_else(|| {
                // Fallback to system font if no embedded data
                self.system_fonts.get(&entry.fallback_type).cloned()
            });
            log::debug!(
                "[RENDER] set_font: {} (id {}), has_data: {}, is_fallback: {}, is_cid: {}",
                name,
                entry.font_id,
                self.state.font_data.is_some(),
                is_fallback,
                entry.is_cid_keyed
            );
            self.state.font_index = entry.collection_index;
            self.state.cid_to_gid_map = entry.cid_to_gid_map.clone();
            self.state.is_cid_keyed = entry.is_cid_keyed;
            self.state.font_name = entry.base_name.clone();
            self.state.font_id = entry.font_id;
            self.state.is_fallback = is_fallback;
            self.state.fallback_type = entry.fallback_type;
        } else {
            // 9.6: the interpreter selected a font this backend was never given, so the
            // text that follows is drawn in whatever font preceded it — wrong glyphs at
            // the right positions, which reads as a rendering bug rather than as a font
            // that failed to load. Fires on none of the nine conforming samples.
            self.decisions.push(fepdf_model::interpretation::Decision::violation(
                "9.6",
                format!("Tf selected /{name}, which never reached the renderer's font cache"),
                "left the previous font in place; the text is drawn with the wrong glyphs",
            ));
        }
    }

    fn set_text_render_mode(&mut self, mode: TextRenderingMode) {
        self.state.text_render_mode = mode as i32;
    }
    fn set_char_spacing(&mut self, spacing: f64) {
        self.state.char_spacing = spacing;
    }
    fn set_word_spacing(&mut self, spacing: f64) {
        self.state.word_spacing = spacing;
    }

    fn show_text(
        &mut self,
        glyphs: &[TextGlyph],
        size: f64,
        transform: kurbo::Affine,
        text_state: TextState,
        _op_index: usize,
    ) {
        let data_arc = self.state.font_data.clone();
        let data_ref = data_arc.as_deref().map_or(&[][..], |v| v.as_slice());
        let brush = to_vello_brush(&self.state.fill_color, self.state.fill_alpha as f32);
        let mut advance_offset = 0.0;
        let mut painted = 0_usize;
        for glyph in glyphs {
            let ctx = GlyphRenderContext {
                size,
                transform: self.state.transform * transform,
                tc: text_state.tc,
                tw: text_state.tw,
                th: text_state.th,
                is_vertical: text_state.is_vertical,
                advance_offset,
                data_ref,
                brush: &brush,
            };
            let (new_advance, drew) = Self::render_single_glyph(
                &mut self.scene,
                &mut self.skrifa_bridge,
                &self.system_fonts,
                &self.state,
                glyph,
                &ctx,
            );
            painted += usize::from(drew);
            advance_offset = new_advance;
        }

        // A run that laid out characters and painted none of them. **Per run, not per
        // glyph**: one glyph with an empty outline is ordinary — `samples/volvo_xc90.pdf`
        // has two, a CID glyph that draws to nothing — while a whole run drawing nothing
        // means the text is on the page, correctly spaced, and invisible.
        //
        // That is precisely what a standard-14 font did before Phase P, on every run of
        // every page, and nothing above the backend could tell: `show_text` took the
        // success flag from each glyph and bound it to `_success`.
        if painted == 0 && !glyphs.is_empty() {
            self.decisions.push(fepdf_model::interpretation::Decision::violation(
                "9.6",
                format!(
                    "a run of {} glyphs in /{} yielded no outline at all",
                    glyphs.len(),
                    self.state.font_name.as_deref().unwrap_or("(unnamed)")
                ),
                "advanced the text position and drew nothing; the text is laid out and \
                 invisible",
            ));
        }
    }
}

fn to_vello_brush(color: &Color, alpha: f32) -> vello::peniko::Brush {
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
    match color {
        Color::Gray(g) => {
            let v = (g.clamp(0.0, 1.0) * 255.0) as u8;
            vello::peniko::Brush::Solid(vello::peniko::Color::from_rgba8(v, v, v, a))
        }
        Color::Rgb(r, g, b) => {
            let r_u8 = (r.clamp(0.0, 1.0) * 255.0) as u8;
            let g_u8 = (g.clamp(0.0, 1.0) * 255.0) as u8;
            let b_u8 = (b.clamp(0.0, 1.0) * 255.0) as u8;
            vello::peniko::Brush::Solid(vello::peniko::Color::from_rgba8(r_u8, g_u8, b_u8, a))
        }
        Color::Cmyk(..) => {
            // Was a second, different implementation of CMYK to RGB — the naive
            // `(1 − c)(1 − k)` — sitting beside `Color::to_rgb`'s. Two conversions for
            // one clause means fixing 10.4.2.5 in one place leaves the other wrong, so
            // this defers rather than repeats.
            to_vello_brush(&color.to_rgb(), alpha)
        }
        Color::Lab(..) => to_vello_brush(&color.to_rgb(), alpha),
    }
}

fn to_peniko_color(color: &Color, alpha: f32) -> vello::peniko::Color {
    let rgb = color.to_rgb();
    let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
    if let Color::Rgb(r, g, b) = rgb {
        let r_u8 = (r.clamp(0.0, 1.0) * 255.0) as u8;
        let g_u8 = (g.clamp(0.0, 1.0) * 255.0) as u8;
        let b_u8 = (b.clamp(0.0, 1.0) * 255.0) as u8;
        vello::peniko::Color::from_rgba8(r_u8, g_u8, b_u8, a)
    } else {
        vello::peniko::Color::from_rgba8(0, 0, 0, a)
    }
}

fn to_vello_shading_brush(shading: &ShadingSpec, alpha: f32) -> vello::peniko::Brush {
    match shading {
        ShadingSpec::Axial(axial) => {
            let p0 = kurbo::Point::new(axial.coords[0], axial.coords[1]);
            let p1 = kurbo::Point::new(axial.coords[2], axial.coords[3]);
            let stops: Vec<vello::peniko::ColorStop> = axial
                .stops
                .iter()
                .map(|s| vello::peniko::ColorStop {
                    offset: s.offset,
                    color: to_peniko_color(&s.color, alpha).into(),
                })
                .collect();
            let mut grad = vello::peniko::Gradient::new_linear(p0, p1);
            grad.stops = stops.as_slice().into();
            vello::peniko::Brush::Gradient(grad)
        }
        ShadingSpec::Radial(radial) => {
            let p0 = kurbo::Point::new(radial.coords[0], radial.coords[1]);
            let r0 = radial.coords[2] as f32;
            let p1 = kurbo::Point::new(radial.coords[3], radial.coords[4]);
            let r1 = radial.coords[5] as f32;
            let stops: Vec<vello::peniko::ColorStop> = radial
                .stops
                .iter()
                .map(|s| vello::peniko::ColorStop {
                    offset: s.offset,
                    color: to_peniko_color(&s.color, alpha).into(),
                })
                .collect();
            let mut grad = vello::peniko::Gradient::new_two_point_radial(p0, r0, p1, r1);
            grad.stops = stops.as_slice().into();
            vello::peniko::Brush::Gradient(grad)
        }
        // A mesh has a colour per vertex and a brush has one colour, so there is no
        // brush that represents it. `paint_shading` draws the triangles instead; this
        // arm is only reached through a *pattern* fill, where one brush is all the caller
        // can take, and the mean is a better answer than the black that used to be here.
        ShadingSpec::Mesh(mesh) => {
            vello::peniko::Brush::Solid(to_peniko_color(&mesh_average(mesh), alpha))
        }
    }
}

/// How far, in user space, to grow each mesh triangle so neighbours overlap.
///
/// **Adjacent triangles antialias against each other and leave white between them.** Each
/// covers about half of the pixels along a shared edge, and the two halves composite over
/// the page rather than over one another, so every internal edge is a pale seam. Measured
/// on `target/mesh/type4.pdf`, whose quadrant should read 127: the seams took it to 137,
/// and 170 on the patch types, which subdivide far more finely. Setting the subdivision
/// to zero gave exactly 127 on both — which is how the seams were told apart from a
/// decoding error, since either one produces a number that is merely wrong.
///
/// Half a device pixel, converted back through the CTM. Growing an opaque fill into its
/// neighbour is invisible when the two differ by less than the tolerance that produced
/// them; leaving the gap is not.
fn seam_bleed(transform: Affine) -> f64 {
    let [a, b, c, d, _, _] = transform.as_coeffs();
    let scale = b.mul_add(-c, a * d).abs().sqrt();
    if scale > f64::EPSILON { 0.5 / scale } else { 0.0 }
}

/// One triangle, with each corner pushed out from the centroid by `bleed`.
fn grown_triangle(points: [(f64, f64); 3], bleed: f64) -> kurbo::BezPath {
    let centre = (
        (points[0].0 + points[1].0 + points[2].0) / 3.0,
        (points[0].1 + points[1].1 + points[2].1) / 3.0,
    );
    let grown = points.map(|(x, y)| {
        let (dx, dy) = (x - centre.0, y - centre.1);
        let len = dx.hypot(dy);
        if len <= f64::EPSILON {
            kurbo::Point::new(x, y)
        } else {
            kurbo::Point::new((dx / len).mul_add(bleed, x), (dy / len).mul_add(bleed, y))
        }
    });
    let mut path = kurbo::BezPath::new();
    path.move_to(grown[0]);
    path.line_to(grown[1]);
    path.line_to(grown[2]);
    path.close_path();
    path
}

/// The mean of every corner colour in a mesh, for the one place a mesh must become a
/// single brush.
fn mesh_average(mesh: &fepdf_model::graphics::TriangleMesh) -> Color {
    let mut total = (0.0_f64, 0.0_f64, 0.0_f64);
    let mut count = 0.0_f64;
    for triangle in &mesh.triangles {
        for color in &triangle.colors {
            if let Color::Rgb(r, g, b) = color.to_rgb() {
                total = (total.0 + r, total.1 + g, total.2 + b);
                count += 1.0;
            }
        }
    }
    if count == 0.0 {
        return Color::Gray(0.0);
    }
    Color::Rgb(total.0 / count, total.1 / count, total.2 / count)
}

fn to_vello_paint_brush(paint: &Paint, alpha: f32) -> vello::peniko::Brush {
    match paint {
        Paint::Solid(col) => to_vello_brush(col, alpha),
        Paint::Pattern(PatternSpec::Shading(shading)) => to_vello_shading_brush(shading, alpha),
        Paint::Pattern(PatternSpec::Tiling { .. }) => vello::peniko::Brush::Solid(
            vello::peniko::Color::from_rgba8(0, 0, 0, (alpha.clamp(0.0, 1.0) * 255.0) as u8),
        ),
    }
}

/// A rectangle large enough to bound any page, for a layer that is not meant to clip.
const UNBOUNDED: kurbo::Rect = kurbo::Rect::new(-1.0e7, -1.0e7, 1.0e7, 1.0e7);

/// A rectangle enclosing nothing, for a layer whose drawing must not reach the page.
const EMPTY: kurbo::Rect = kurbo::Rect::new(0.0, 0.0, 0.0, 0.0);

/// Which part of a soft mask this renderer could not express, for the decision that says
/// so. Named entries only: a caller acting on this wants to know whether it was the
/// channel, the backdrop or the transfer function.
fn describe(spec: &fepdf_content::SoftMaskSpec) -> String {
    let mut parts = Vec::new();
    if spec.kind == fepdf_content::SoftMaskKind::Alpha {
        parts.push("/S /Alpha".to_string());
    }
    if spec.backdrop.is_some() {
        parts.push("/BC".to_string());
    }
    if spec.transfer.is_some() {
        parts.push("/TR".to_string());
    }
    parts.join(" and ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque(width: u32, height: u32) -> Vec<u8> {
        vec![255; (width * height * 4) as usize]
    }

    /// A soft mask smaller than its image is **scaled onto it**, not skipped.
    ///
    /// This was applied only when the two agreed on both dimensions, and dropped in
    /// silence otherwise — so the common case, a small mask over a large image, did
    /// nothing at all. 8.9.5.4 says the mask is scaled to the image.
    #[test]
    fn a_mask_of_a_different_size_is_scaled_onto_the_image() {
        // Two mask samples across: the left half transparent, the right half opaque.
        let mask =
            SMaskData { data: vec![0, 255], width: 2, height: 1, format: PixelFormat::Gray8 };
        let (w, h) = (4, 2);
        let mut rgba = opaque(w, h);
        apply_image_smask(&mut rgba, w, h, &mask);

        let alpha = |x: u32, y: u32| rgba[((y * w + x) * 4 + 3) as usize];
        assert_eq!((alpha(0, 0), alpha(1, 0)), (0, 0), "the left half takes the first sample");
        assert_eq!((alpha(2, 0), alpha(3, 0)), (255, 255), "the right half takes the second");
        assert_eq!((alpha(0, 1), alpha(3, 1)), (0, 255), "and every row does the same");
    }

    /// The same size still works, which is what it used to do and all it used to do.
    #[test]
    fn a_mask_of_the_same_size_is_applied_sample_for_sample() {
        let mask = SMaskData {
            data: vec![0, 64, 128, 255],
            width: 2,
            height: 2,
            format: PixelFormat::Gray8,
        };
        let mut rgba = opaque(2, 2);
        apply_image_smask(&mut rgba, 2, 2, &mask);
        let alphas: Vec<u8> = rgba.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
        assert_eq!(alphas, vec![0, 64, 128, 255]);
    }

    /// A mask whose data is shorter than it claims does not panic the renderer.
    ///
    /// It is a malformed file, and every read is bounds-checked so that it stays one.
    #[test]
    fn a_mask_shorter_than_it_claims_leaves_the_rest_opaque() {
        let mask = SMaskData { data: vec![0], width: 4, height: 4, format: PixelFormat::Gray8 };
        let mut rgba = opaque(4, 4);
        apply_image_smask(&mut rgba, 4, 4, &mask);
        let alphas: Vec<u8> = rgba.as_chunks::<4>().0.iter().map(|p| p[3]).collect();
        assert_eq!(alphas[0], 0, "the one sample it has");
        assert!(alphas[1..].iter().all(|a| *a == 255), "the rest stay opaque");
    }

    /// A mask with no area is ignored rather than dividing by zero.
    #[test]
    fn a_mask_with_no_area_changes_nothing() {
        let mask = SMaskData { data: vec![], width: 0, height: 0, format: PixelFormat::Gray8 };
        let mut rgba = opaque(2, 2);
        apply_image_smask(&mut rgba, 2, 2, &mask);
        assert!(rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }
}
