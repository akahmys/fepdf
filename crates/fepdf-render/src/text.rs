use kurbo::{BezPath, Point};
use read_fonts::TableProvider;
use read_fonts::types::GlyphId;
use skrifa::instance::Size as SkrifaSize;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::LocationRef;
use skrifa::{FontRef, MetadataProvider};
use std::collections::BTreeMap;

pub struct KurboPen {
    path: BezPath,
}

impl Default for KurboPen {
    fn default() -> Self {
        Self::new()
    }
}

impl KurboPen {
    pub fn new() -> Self {
        Self { path: BezPath::new() }
    }
    pub fn finish(self) -> BezPath {
        self.path
    }
}

impl OutlinePen for KurboPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(Point::new(x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(Point::new(x as f64, y as f64));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.quad_to(Point::new(x1 as f64, y1 as f64), Point::new(x as f64, y as f64));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path.curve_to(
            Point::new(x1 as f64, y1 as f64),
            Point::new(x2 as f64, y2 as f64),
            Point::new(x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}

pub struct TextLayoutOptions {
    pub font_size: f32,
    pub char_spacing: f32,
    pub word_spacing: f32,
    pub horizontal_scaling: f32,
}

impl Default for TextLayoutOptions {
    fn default() -> Self {
        Self { font_size: 1.0, char_spacing: 0.0, word_spacing: 0.0, horizontal_scaling: 100.0 }
    }
}

pub struct SkrifaBridge {
    glyph_cache: BTreeMap<(u64, u32, u32), BezPath>,
}

impl Default for SkrifaBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl SkrifaBridge {
    pub fn new() -> Self {
        Self { glyph_cache: BTreeMap::new() }
    }

    pub fn get_units_per_em(&self, data: &[u8]) -> Option<u16> {
        if let Ok(font) = FontRef::from_index(data, 0) {
            return Some(font.head().ok()?.units_per_em());
        }
        None
    }
}

pub struct GlyphExtractionContext<'a> {
    pub font_id: u64,
    pub data: &'a [u8],
    pub gid: u32,
    pub char_code: u32,
    pub cid_to_gid_map: Option<&'a BTreeMap<u32, u32>>,
    pub is_vertical: bool,
    pub unicode_fallback: Option<char>,
    pub is_japanese: bool,
    pub is_cid: bool,
    pub collection_index: u32,
    pub is_fallback: bool,
}

impl SkrifaBridge {
    pub fn extract_path(&mut self, ctx: &GlyphExtractionContext) -> Option<BezPath> {
        let cache_key = (ctx.font_id, ctx.gid, ctx.char_code);
        if let Some(path) = self.glyph_cache.get(&cache_key) {
            return Some(path.clone());
        }

        let final_gid = ctx.gid;
        let unicode = ctx.unicode_fallback;

        let path = self.try_extract_from_data(
            ctx.data,
            ctx.font_id,
            final_gid,
            ctx.char_code,
            ctx.is_cid,
            ctx.collection_index,
            unicode,
            ctx.is_fallback,
            ctx.cid_to_gid_map,
        );

        if let Some(ref p) = path
            && p.segments().count() > 0
        {
            self.glyph_cache.insert((ctx.font_id, ctx.gid, ctx.char_code), p.clone());
        }
        path
    }

    fn is_blank_char(&self, u: Option<char>) -> bool {
        matches!(
            u,
            Some('\u{0020}')
                | Some('\u{00A0}')
                | Some('\u{2000}'..='\u{200F}')
                | Some('\u{3000}')
                | Some('\u{202F}')
        )
    }

    fn resolve_glyph_id(
        font: &FontRef,
        final_gid_in: u32,
        is_fallback: bool,
        is_cid: bool,
        unicode: Option<char>,
        char_code: u32,
        cid_to_gid_map: Option<&BTreeMap<u32, u32>>,
    ) -> GlyphId {
        let mut final_gid = GlyphId::new(final_gid_in);

        if is_fallback
            || (final_gid.to_u32() == 0
                && !is_cid
                && unicode.is_some()
                && unicode.and_then(|u| font.charmap().map(u)).is_some())
        {
            if is_fallback
                && let Some(u) = unicode
                && let Some(gid) = font.charmap().map(u)
            {
                final_gid = gid;
            } else if final_gid.to_u32() == 0
                && !is_cid
                && let Some(u) = unicode
                && let Some(gid) = font.charmap().map(u)
            {
                final_gid = gid;
            }
        }

        if final_gid.to_u32() == 0
            && is_cid
            && let Some(map) = cid_to_gid_map
            && let Some(&gid) = map.get(&char_code)
        {
            final_gid = GlyphId::new(gid);
        }

        final_gid
    }

    fn draw_glyph_path(font: &FontRef, final_gid: GlyphId) -> Option<BezPath> {
        let upem = font.head().map(|h| h.units_per_em()).unwrap_or(1000);
        let mut pen = KurboPen::new();
        let glyph = font.outline_glyphs().get(final_gid)?;
        if let Err(e) = glyph.draw(
            DrawSettings::unhinted(SkrifaSize::new(upem as f32), LocationRef::default()),
            &mut pen,
        ) {
            log::warn!("[SKRIFA] Drawing failed for GID {}: {:?}", final_gid, e);
            return None;
        }
        Some(pen.finish())
    }

    #[allow(clippy::too_many_arguments)]
    fn try_extract_from_data(
        &mut self,
        data: &[u8],
        _font_id: u64,
        final_gid_in: u32,
        char_code: u32,
        is_cid: bool,
        collection_index: u32,
        unicode: Option<char>,
        is_fallback: bool,
        cid_to_gid_map: Option<&BTreeMap<u32, u32>>,
    ) -> Option<BezPath> {
        if self.is_blank_char(unicode) {
            return Some(BezPath::new());
        }
        if data.is_empty() {
            return None;
        }

        let Ok(font) = FontRef::from_index(data, collection_index) else {
            return None;
        };

        let final_gid = Self::resolve_glyph_id(
            &font,
            final_gid_in,
            is_fallback,
            is_cid,
            unicode,
            char_code,
            cid_to_gid_map,
        );

        if final_gid.to_u32() == 0 {
            if self.is_blank_char(unicode) {
                return Some(kurbo::BezPath::new());
            }
            return None;
        }

        let path = Self::draw_glyph_path(&font, final_gid)?;
        let seg_count = path.segments().count();
        if seg_count == 0 && !self.is_blank_char(unicode) {
            return None;
        }
        Some(path)
    }
}
