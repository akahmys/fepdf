pub mod headless;
pub mod path;
pub mod text;

use ferruginous_core::graphics::TextRenderingMode;
pub use ferruginous_core::graphics::WindingRule;
use ferruginous_core::{BlendMode, Color, LineCap, LineJoin, PixelFormat, StrokeStyle};
use kurbo::{Affine, BezPath, Cap, Join, Stroke};
use std::sync::Arc;
use vello::Scene;
use vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

// Re-export core types for convenience
pub use ferruginous_core::font::FallbackFontType;

#[derive(Debug, Clone)]
pub struct SMaskData {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
}

pub trait RenderBackend {
    fn transform(&mut self, transform: Affine);
    fn set_transform(&mut self, transform: Affine);
    fn push_state(&mut self);
    fn pop_state(&mut self);
    fn fill_path(&mut self, path: &BezPath, color: &Color, rule: WindingRule);
    fn stroke_path(&mut self, path: &BezPath, color: &Color, style: &StrokeStyle);
    fn push_clip(&mut self, path: &BezPath, rule: WindingRule);
    fn pop_clip(&mut self);
    fn set_fill_alpha(&mut self, alpha: f64);
    fn set_stroke_alpha(&mut self, alpha: f64);
    fn set_fill_color(&mut self, color: Color);
    fn set_stroke_color(&mut self, color: Color);
    fn set_blend_mode(&mut self, mode: BlendMode);
    fn draw_image(
        &mut self,
        image: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
        smask: Option<SMaskData>,
    );
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
    fn set_font(&mut self, name: &str);
    fn set_text_render_mode(&mut self, mode: TextRenderingMode);
    fn set_char_spacing(&mut self, spacing: f64);
    fn set_word_spacing(&mut self, spacing: f64);
    fn show_text(
        &mut self,
        glyphs: &[TextGlyph],
        size: f64,
        transform: kurbo::Affine,
        state: TextState,
        op_index: usize,
    );
}

#[derive(Debug, Clone)]
pub struct TextGlyph {
    pub gid: u32,
    pub name: Option<String>,
    pub char_code: u32,
    pub unicode: String,
    pub width: f32,
    pub vx: f32,
    pub vy: f32,
    pub is_fallback: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct TextState {
    pub tc: f64,
    pub tw: f64,
    pub th: f64,
    pub is_vertical: bool,
}

pub struct VelloBackend {
    scene: Scene,
    state: VelloState,
    state_stack: Vec<VelloState>,
    font_cache: std::collections::BTreeMap<String, FontCacheEntry>,
    system_fonts: Arc<std::collections::BTreeMap<FallbackFontType, Arc<Vec<u8>>>>,
    skrifa_bridge: crate::text::SkrifaBridge,
    next_font_id: u64,
}

#[derive(Clone)]
struct VelloState {
    transform: Affine,
    fill_color: Color,
    stroke_color: Color,
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
    pub fn load_system_fonts() -> Arc<std::collections::BTreeMap<FallbackFontType, Arc<Vec<u8>>>> {
        let mut fonts = std::collections::BTreeMap::new();
        let resource_dir =
            std::env::var("FERRUGINOUS_RESOURCES").unwrap_or_else(|_| "resources".to_string());
        let base_path = std::path::Path::new(&resource_dir).join("fonts");

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
                    fonts.insert(ftype, Arc::new(data));
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

    pub fn new(
        system_fonts: Arc<std::collections::BTreeMap<FallbackFontType, Arc<Vec<u8>>>>,
    ) -> Self {
        Self {
            scene: Scene::new(),
            state: VelloState {
                transform: Affine::IDENTITY,
                fill_color: Color::Gray(0.0),
                stroke_color: Color::Gray(0.0),
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
        }
    }

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
        let is_japanese = state
            .font_name
            .as_ref()
            .map(|n| {
                let lower = n.to_lowercase();
                lower.contains("mincho")
                    || lower.contains("gothic")
                    || lower.contains("hira")
                    || lower.contains("koz")
                    || n.contains("明朝")
                    || n.contains("ゴシック")
                    || is_cid
            })
            .unwrap_or(false);

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
        let scale = ctx.size / upem as f64;

        let h_scale = if ctx.is_vertical { 1.0 } else { ctx.th };
        let v_scale = if ctx.is_vertical { ctx.th } else { 1.0 };

        let local_to_pt = Affine::scale_non_uniform(scale * h_scale, scale * v_scale)
            * Affine::translate(kurbo::Vec2::new(-glyph.vx as f64, -glyph.vy as f64));

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
                adv += tw * th;
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
    for chunk in image_data.chunks_exact(3) {
        data.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
    }
    data
}

fn convert_cmyk8(image_data: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(image_data.len() / 4 * 4);
    for chunk in image_data.chunks_exact(4) {
        let c = f64::from(chunk[0]) / 255.0;
        let m = f64::from(chunk[1]) / 255.0;
        let y = f64::from(chunk[2]) / 255.0;
        let k = f64::from(chunk[3]) / 255.0;
        let r = ((1.0 - c) * (1.0 - k) * 255.0) as u8;
        let g = ((1.0 - m) * (1.0 - k) * 255.0) as u8;
        let b = ((1.0 - y) * (1.0 - k) * 255.0) as u8;
        data.extend_from_slice(&[r, g, b, 255]);
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
        _ => (0, 0, 0),
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

fn apply_image_smask(rgba_data: &mut [u8], mask: &SMaskData) {
    for (i, chunk) in rgba_data.chunks_exact_mut(4).enumerate() {
        let mask_val = match mask.format {
            PixelFormat::Gray8 => mask.data[i],
            PixelFormat::Rgba8 => mask.data[i * 4 + 3],
            PixelFormat::Rgb8 => {
                let r = f64::from(mask.data[i * 3]);
                let g = f64::from(mask.data[i * 3 + 1]);
                let b = f64::from(mask.data[i * 3 + 2]);
                ((0.299 * r) + (0.587 * g) + (0.114 * b)) as u8
            }
            _ => 255,
        };
        chunk[3] = ((f64::from(chunk[3]) * f64::from(mask_val)) / 255.0) as u8;
    }
}

fn has_move_to(path: &BezPath) -> bool {
    path.elements().iter().any(|el| matches!(el, kurbo::PathEl::MoveTo(_)))
}

impl RenderBackend for VelloBackend {
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
        let brush = to_vello_brush(color, self.state.fill_alpha as f32);
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
        let brush = to_vello_brush(color, self.state.stroke_alpha as f32);
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

        self.scene.push_layer(
            vello_rule,
            vello::peniko::Mix::Normal,
            1.0f32,
            self.state.transform,
            &closed_path,
        );
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
    }
    fn set_stroke_color(&mut self, color: Color) {
        self.state.stroke_color = color;
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

        if let Some(mask) = &smask
            && mask.width == width
            && mask.height == height
        {
            apply_image_smask(&mut rgba_data, mask);
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
            log::warn!("[RENDER] set_font: {} NOT FOUND in cache", name);
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
        let _is_fallback = self.state.is_fallback;

        let data_arc = self.state.font_data.clone();
        let data_ref = data_arc.as_deref().map(|v| v.as_slice()).unwrap_or(&[]);
        let brush = to_vello_brush(&self.state.fill_color, self.state.fill_alpha as f32);
        let mut advance_offset = 0.0;
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
            let (new_advance, _success) = Self::render_single_glyph(
                &mut self.scene,
                &mut self.skrifa_bridge,
                &self.system_fonts,
                &self.state,
                glyph,
                &ctx,
            );
            advance_offset = new_advance;
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
        Color::Cmyk(c, m, y, k) => {
            // Simple CMYK to RGB conversion
            let r = (1.0 - c) * (1.0 - k);
            let g = (1.0 - m) * (1.0 - k);
            let b = (1.0 - y) * (1.0 - k);
            let r_u8 = (r.clamp(0.0, 1.0) * 255.0) as u8;
            let g_u8 = (g.clamp(0.0, 1.0) * 255.0) as u8;
            let b_u8 = (b.clamp(0.0, 1.0) * 255.0) as u8;
            vello::peniko::Brush::Solid(vello::peniko::Color::from_rgba8(r_u8, g_u8, b_u8, a))
        }
        Color::Lab(..) => to_vello_brush(&color.to_rgb(), alpha),
    }
}
