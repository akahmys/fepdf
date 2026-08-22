//! Glyph outline extraction, bridging `skrifa` to `kurbo` paths.
//!
//! See `lib.rs` for why `&mut SkrifaBridge` is kept on non-writing helpers.

use kurbo::{BezPath, Point};
use read_fonts::TableProvider;
use read_fonts::types::GlyphId;
use skrifa::instance::Size as SkrifaSize;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::prelude::LocationRef;
use skrifa::{FontRef, MetadataProvider};
use std::collections::BTreeMap;

/// An `OutlinePen` that accumulates glyph outlines into a `kurbo` path.
pub struct KurboPen {
    path: BezPath,
}

impl Default for KurboPen {
    fn default() -> Self {
        Self::new()
    }
}

impl KurboPen {
    /// Starts an empty pen.
    #[must_use]
    pub fn new() -> Self {
        Self { path: BezPath::new() }
    }
    /// Consumes the pen and yields the accumulated outline.
    #[must_use]
    pub fn finish(self) -> BezPath {
        self.path
    }
}

impl OutlinePen for KurboPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(Point::new(f64::from(x), f64::from(y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(Point::new(f64::from(x), f64::from(y)));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.quad_to(
            Point::new(f64::from(x1), f64::from(y1)),
            Point::new(f64::from(x), f64::from(y)),
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path.curve_to(
            Point::new(f64::from(x1), f64::from(y1)),
            Point::new(f64::from(x2), f64::from(y2)),
            Point::new(f64::from(x), f64::from(y)),
        );
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}

/// Spacing parameters applied while laying out a text run.
pub struct TextLayoutOptions {
    /// Font size in text-space units.
    pub font_size: f32,
    /// Additional space after each glyph (`Tc`).
    pub char_spacing: f32,
    /// Additional space after each space glyph (`Tw`).
    pub word_spacing: f32,
    /// Horizontal scaling percentage (`Tz`).
    pub horizontal_scaling: f32,
}

impl Default for TextLayoutOptions {
    fn default() -> Self {
        Self { font_size: 1.0, char_spacing: 0.0, word_spacing: 0.0, horizontal_scaling: 100.0 }
    }
}

/// Caches resolved glyph outlines across a render pass.
pub struct SkrifaBridge {
    glyph_cache: BTreeMap<(u64, u32, u32), BezPath>,
    /// Glyphs whose outline the font program would not yield, drained by the backend.
    decisions: Vec<fepdf_model::interpretation::Decision>,
}

impl Default for SkrifaBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl SkrifaBridge {
    /// Creates a bridge with an empty glyph cache.
    #[must_use]
    pub fn new() -> Self {
        Self { glyph_cache: BTreeMap::new(), decisions: Vec::new() }
    }

    /// Takes the decisions reached about glyphs since the last call.
    pub fn take_decisions(&mut self) -> Vec<fepdf_model::interpretation::Decision> {
        std::mem::take(&mut self.decisions)
    }

    /// Reads the font's units-per-em, the scale its outlines are expressed in.
    pub fn get_units_per_em(&self, data: &[u8]) -> Option<u16> {
        if let Ok(font) = FontRef::from_index(data, 0) {
            return Some(font.head().ok()?.units_per_em());
        }
        None
    }
}

/// Everything needed to resolve one glyph to an outline.
// Each flag selects a distinct lookup path; folding them into an enum would
// need one variant per combination.
#[allow(clippy::struct_excessive_bools)]
pub struct GlyphExtractionContext<'a> {
    /// Cache key identifying the font program.
    pub font_id: u64,
    /// The raw font program.
    pub data: &'a [u8],
    /// Glyph index within the font program.
    pub gid: u32,
    /// Originating character code, part of the cache key.
    pub char_code: u32,
    /// Explicit CID-to-GID mapping, when the font supplies one.
    pub cid_to_gid_map: Option<&'a BTreeMap<u32, u32>>,
    /// Whether the writing mode is vertical.
    pub is_vertical: bool,
    /// Character to look up if the glyph index does not resolve.
    pub unicode_fallback: Option<char>,
    /// Whether Japanese fallback handling applies.
    pub is_japanese: bool,
    /// Whether the font is CID-keyed.
    pub is_cid: bool,
    /// Index within a font collection (TTC).
    pub collection_index: u32,
    /// Whether this glyph came from a fallback font.
    pub is_fallback: bool,
}

impl SkrifaBridge {
    /// Resolves a glyph to its outline, consulting the cache first.
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
            Some('\u{0020}' | '\u{00A0}' | '\u{2000}'..='\u{200F}' | '\u{3000}' | '\u{202F}')
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

    fn draw_glyph_path(
        font: &FontRef,
        final_gid: GlyphId,
        decisions: &mut Vec<fepdf_model::interpretation::Decision>,
    ) -> Option<BezPath> {
        let upem = font.head().map_or(1000, |h| h.units_per_em());
        let mut pen = KurboPen::new();
        let glyph = font.outline_glyphs().get(final_gid)?;
        if let Err(e) = glyph.draw(
            DrawSettings::unhinted(SkrifaSize::new(f32::from(upem)), LocationRef::default()),
            &mut pen,
        ) {
            // 9.9: the glyph is in the font and its outline will not build, so the
            // character is dropped while its advance is kept — a hole in the line rather
            // than a missing line. Fires on none of the nine conforming samples.
            decisions.push(fepdf_model::interpretation::Decision::violation(
                "9.9",
                format!("glyph {final_gid} has an outline the font program will not yield: {e:?}"),
                "drew nothing for it; the glyph's advance is still applied",
            ));
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

        let path = Self::draw_glyph_path(&font, final_gid, &mut self.decisions)?;
        let seg_count = path.segments().count();
        if seg_count == 0 && !self.is_blank_char(unicode) {
            return None;
        }
        Some(path)
    }
}
