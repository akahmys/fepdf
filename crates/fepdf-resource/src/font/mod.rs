//! PDF Font Engine (ISO 32000-2:2020 Clause 9)

pub use fepdf_font::{agl, cff_standard, cmap, reconstruction, rescue, subset};
/// Loads embedded font programs out of a document.
pub mod loader;
/// Glyph metrics: widths, bounding boxes and vertical advances.
pub mod metrics;
pub use fepdf_font::reconstruction::{FontInfo, FontReconstructor, ReconstructedFont};
/// Typed schema for font dictionaries.
pub mod schema;

use fepdf_core::arena::PdfArena;
use fepdf_core::handle::Handle;
use fepdf_core::object::{Object, PdfName};
use fepdf_core::PdfResult;
use self::loader::FontLoader;
use self::metrics::{detect_wmode, FontMetrics};
use std::collections::BTreeMap;
use std::sync::Arc;

pub use fepdf_content::FallbackFontType;

/// Summarised information about an embedded or referenced font.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FontSummary {
    /// PostScript name.
    pub name: String,
    /// PDF Subtype (Type1, TrueType, Type0, Type3, MMType1).
    pub font_type: String,
    /// Encoding name or `Custom`.
    pub encoding: String,
    /// Whether the font program is embedded.
    pub is_embedded: bool,
    /// Whether the embedded program is a subset.
    pub is_subset: bool,
}

/// Trace context for font glyph resolution diagnostics.
#[derive(Debug, Clone, Default)]
pub struct TraceContext;

impl TraceContext {
    /// Creates a new trace context.
    pub fn new() -> Self { Self }
    /// Starts a trace.
    pub fn start(&mut self, _cid: u32, _hint: Option<char>) {}
    /// Pushes a diagnostic step.
    pub fn push_step(&mut self, _step: impl Into<String>) {}
    /// Finishes a trace.
    pub fn finish(&mut self, _gid: Option<u32>) {}
}

/// Represents an ingested PDF Font resource dictionary.
#[derive(Debug, Clone)]
pub struct FontResource {
    /// Subtype of font dictionary.
    pub subtype: PdfName,
    /// PostScript base font name.
    pub base_font: PdfName,
    /// Font metrics (widths, missing width).
    pub metrics: FontMetrics,
    /// Raw embedded font binary program.
    pub data: Option<Arc<Vec<u8>>>,
    /// Reconstructed font binary program (SFNT).
    pub reconstructed_data: Option<Arc<Vec<u8>>>,
    /// Parsed CMap or Encoding map.
    pub encoding: Option<fepdf_font::cmap::CMap>,
    /// Unicode -> GID map.
    pub unicode_to_gid: BTreeMap<char, u32>,
    /// CID -> GID map.
    pub cid_to_gid_map: Option<BTreeMap<u32, u32>>,
    /// Name -> GID map.
    pub name_to_gid_map: Option<BTreeMap<String, u32>>,
    /// SID -> GID map.
    pub sid_to_gid_map: Option<BTreeMap<u32, u32>>,
    /// Total number of glyphs in font program.
    pub num_glyphs: usize,
    /// Whether font is CID-keyed.
    pub is_cid_keyed: bool,
    /// Unified mapping for character resolution.
    pub unified_map: BTreeMap<String, u32>,
    /// CID ordering string (e.g. "Identity").
    pub cid_ordering: Option<String>,
    /// Writing mode (0=Horizontal, 1=Vertical).
    pub wmode: i32,
    /// Vertical advance metrics.
    pub v_widths: BTreeMap<u32, (f32, f32, f32)>,
}

impl FontResource {
    /// Creates a new FontResource instance.
    #[allow(clippy::too_many_arguments)]
    pub fn new_initial(
        subtype: PdfName,
        base_font: PdfName,
        metrics: FontMetrics,
        data: Option<Vec<u8>>,
        encoding: Option<fepdf_font::cmap::CMap>,
        to_unicode: Option<fepdf_font::cmap::CMap>,
        cid_to_gid_map: Option<BTreeMap<u32, u32>>,
        cid_ordering: Option<String>,
        wmode: Option<i32>,
        is_cid_keyed: bool,
        _parent_dict: &BTreeMap<Handle<PdfName>, Object>,
        _arena: &PdfArena,
    ) -> Self {
        let data_arc = data.map(Arc::new);
        let mut res = Self {
            subtype,
            base_font,
            metrics,
            data: data_arc,
            reconstructed_data: None,
            encoding,
            unicode_to_gid: BTreeMap::new(),
            cid_to_gid_map,
            name_to_gid_map: None,
            sid_to_gid_map: None,
            num_glyphs: 0,
            is_cid_keyed,
            unified_map: BTreeMap::new(),
            cid_ordering,
            wmode: wmode.unwrap_or(0),
            v_widths: BTreeMap::new(),
        };

        if let Some(tu) = to_unicode {
            for (code, s) in tu.mappings.iter() {
                if let Ok(code_str) = std::str::from_utf8(code)
                    && let Ok(num) = code_str.parse::<u32>()
                {
                    res.unified_map.insert(s.clone(), num);
                }
            }
        }
        res
    }

    /// Loads a FontResource out of a Document's font dictionary object.
    pub fn load<F>(dict_obj: &Object, arena: &PdfArena, decode_stream: F) -> PdfResult<Self>
    where
        F: Fn(&Object) -> Option<Vec<u8>>,
    {
        let dict = Object::resolve(dict_obj, arena).as_dict_handle().and_then(|h| arena.get_dict(h))
            .ok_or_else(|| fepdf_core::PdfError::Ingestion { context: "Font Load".into(), message: "Expected font dict".into() })?;

        let subtype = dict.get(&arena.name("Subtype"))
            .and_then(|o| Object::resolve(o, arena).as_name())
            .and_then(|h| arena.get_name(h))
            .unwrap_or_else(|| PdfName::new("Type1"));

        let base_font = dict.get(&arena.name("BaseFont"))
            .and_then(|o| Object::resolve(o, arena).as_name())
            .and_then(|h| arena.get_name(h))
            .unwrap_or_else(|| PdfName::new("Helvetica"));

        let is_cid = subtype.as_str() == "Type0" || subtype.as_str() == "CIDFontType0" || subtype.as_str() == "CIDFontType2";
        let metrics = if is_cid { FontMetrics::parse_cid(&dict, arena) } else { FontMetrics::parse_standard(&dict, arena) };
        let font_data = dict.get(&arena.name("FontDescriptor")).and_then(|fd| FontLoader::extract_data(fd, arena, &decode_stream, Some(&dict)));

        let mut res = Self::new_initial(
            subtype,
            base_font,
            metrics,
            font_data.map(|fd| fd.data),
            None,
            None,
            None,
            None,
            Some(detect_wmode(&dict, arena)),
            is_cid,
            &dict,
            arena,
        );

        if let Some(ref raw_data) = res.data
            && let Ok(reconstructed) = FontReconstructor::reconstruct(&res, raw_data)
        {
            res.reconstructed_data = Some(Arc::new(reconstructed.data));
            res.cid_to_gid_map = reconstructed.cid_to_gid_map;
            res.name_to_gid_map = reconstructed.name_to_gid_map;
            res.sid_to_gid_map = reconstructed.sid_to_gid_map;
            if let Some(ng) = reconstructed.num_glyphs {
                res.num_glyphs = ng as usize;
            }
        }

        Ok(res)
    }

    /// Whether this font is a CJK font.
    pub fn is_cjk(&self) -> bool {
        let name = self.base_font.as_str();
        name.contains("Mincho")
            || name.contains("Gothic")
            || name.contains("MS-")
            || name.contains("SimSun")
            || name.contains("SimHei")
            || name.contains("MingLiU")
            || name.contains("KozMin")
            || name.contains("KozGo")
    }

    /// Gets width of a glyph by GID.
    pub fn glyph_width_by_gid(&self, gid: u32) -> f32 {
        self.metrics.widths.get(&gid).copied().unwrap_or(self.metrics.default_width)
    }

    /// Resolves GID for a CID or hint name.
    pub fn to_gid(&self, cid: u32, _trace: Option<&mut TraceContext>) -> u32 {
        if let Some(ref map) = self.cid_to_gid_map
            && let Some(&gid) = map.get(&cid)
        {
            return gid;
        }
        cid
    }
}

impl FontInfo for FontResource {
    fn base_font(&self) -> &str {
        self.base_font.as_str()
    }
    fn subtype(&self) -> &str {
        self.subtype.as_str()
    }
    fn is_cid_keyed(&self) -> bool {
        self.is_cid_keyed
    }
    fn is_cjk(&self) -> bool {
        self.is_cjk()
    }
    fn cid_ordering(&self) -> Option<&str> {
        self.cid_ordering.as_deref()
    }
    fn cid_to_gid_map(&self) -> Option<&BTreeMap<u32, u32>> {
        self.cid_to_gid_map.as_ref()
    }
    fn unified_map(&self) -> &BTreeMap<String, u32> {
        &self.unified_map
    }
    fn encoding(&self) -> Option<&fepdf_font::cmap::CMap> {
        self.encoding.as_ref()
    }
    fn glyph_width_by_gid(&self, gid: u32) -> f32 {
        self.glyph_width_by_gid(gid)
    }
    fn to_gid_hint(&self, cid: u32, _hint_name: Option<&str>) -> u32 {
        self.to_gid(cid, None)
    }
}
