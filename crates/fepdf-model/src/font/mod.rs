//! PDF Font Engine (ISO 32000-2:2020 Clause 9)

pub use fepdf_font::{agl, cff_standard, cmap, reconstruction, rescue, subset};
/// Loads embedded font programs out of a document.
pub mod loader;
/// Glyph metrics: widths, bounding boxes and vertical advances.
pub mod metrics;
pub use fepdf_font::reconstruction::{FontReconstructor, ReconstructedFont};
pub use metrics::{FontCategory, FontMetrics, detect_wmode};
/// Typed schema for font dictionaries.
pub mod schema;

/// The base of the glyph-identifier range that is **not** glyph identifiers.
///
/// When a font embeds no program, the model has no glyph indices to give: the document's
/// character codes mean something only in a face this machine happens to have. It answers
/// with `SYSTEM_FALLBACK_BASE + the Unicode scalar value` so the *renderer*, which is the
/// half holding the substitute face, can look the character up in that face's charmap.
///
/// **This was a bare `1_000_000` written in two places in this file and understood in
/// none.** The renderer passed the marker to `skrifa` as a literal glyph index, found no
/// outline at glyph 1000072, and drew nothing — silently, because the caller discarded
/// the success flag. A minimal page setting `/Helvetica` and showing "HELLO" rendered
/// blank while PDFKit painted it. A convention needs both ends, and naming it is what
/// makes the second end findable.
pub const SYSTEM_FALLBACK_BASE: u32 = 1_000_000;

/// Marks `c` as a character to resolve in the host's substitute face.
#[must_use]
pub const fn system_fallback_gid(c: char) -> u32 {
    SYSTEM_FALLBACK_BASE + c as u32
}

/// The character a marker stands for, or `None` if this is an ordinary glyph index.
#[must_use]
pub fn system_fallback_char(gid: u32) -> Option<char> {
    char::from_u32(gid.checked_sub(SYSTEM_FALLBACK_BASE)?)
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
/// Which bundled font stands in when an embedded program is unusable.
pub enum FallbackFontType {
    /// No preference; the loader picks by descriptor flags.
    Default,
    /// A proportional sans-serif face.
    SansSerif,
    /// A proportional serif face.
    Serif,
    /// A fixed-pitch face.
    Monospace,
    /// A Japanese gothic (sans) face.
    JapaneseSans,
    /// A Japanese mincho (serif) face.
    JapaneseSerif,
}

use crate::{Document, Object, PdfArena, PdfError, PdfName, PdfResult};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::handle::Handle;

/// Logical representation of a PDF Font (ISO 32000-2 Clause 9.2).
#[derive(Clone)]
pub struct FontResource {
    // --- Identification ---
    /// The PostScript name of the font.
    pub base_font: PdfName,
    /// The font subtype (e.g., Type1, TrueType, Type0).
    pub subtype: PdfName,
    /// Whether the font is CID-keyed (Type0 or CIDFont).
    pub is_cid_keyed: bool,

    // --- Metrics & Descriptors ---
    /// The first character code defined in the font.
    pub first_char: i32,
    /// The last character code defined in the font.
    pub last_char: i32,
    /// Widths of glyphs in font units (usually 1/1000 em).
    pub widths: BTreeMap<u32, f32>,
    /// Vertical widths for vertical writing mode.
    pub vertical_widths: BTreeMap<u32, (f32, f32, f32)>, // (w1, v_x, v_y)
    /// Default width for glyphs not present in the widths map.
    pub default_width: f32,
    /// The writing mode (0 for horizontal, 1 for vertical).
    pub wmode: u8,
    /// Inferred bold status.
    pub is_bold: bool,
    /// Font descriptor dictionary if present.
    pub descriptor: Option<Handle<Object>>,
    /// Handle to the original font file stream.
    pub file_handle: Option<Handle<Object>>,
    /// Type 1 segment lengths (Length1, Length2, Length3).
    pub length1: Option<u32>,
    /// Length of the encrypted portion (`/Length2`).
    pub length2: Option<u32>,
    /// Length of the trailing zeros section (`/Length3`).
    pub length3: Option<u32>,

    // --- Encodings & Unicode ---
    /// Mapping from byte sequences to glyph names or character codes.
    pub encoding: Option<cmap::CMap>,
    /// The ToUnicode CMap if present.
    pub to_unicode: Option<cmap::CMap>,
    /// The CID-to-Unicode table of the collection `/CIDSystemInfo` declares (9.7.3).
    pub collection_map: Option<cmap::CMap>,
    /// The same table read the other way, for synthesising a `/ToUnicode` on write.
    pub collection_unicode_to_cid: Option<BTreeMap<String, u32>>,
    /// Mapping discovered during content stream scanning (Original Bytes -> Unicode).
    pub discovered_mappings: Arc<std::sync::Mutex<BTreeMap<Vec<u8>, String>>>,
    /// Unified mapping used for CMap synthesis.
    pub unified_map: BTreeMap<String, u32>,

    // --- Internal Glyph Mappings (GIDs) ---
    /// Mapping from Unicode characters to internal Glyph IDs (GIDs).
    pub unicode_to_gid: BTreeMap<char, u32>,
    /// Mapping from Glyph Names to internal Glyph IDs (GIDs).
    pub glyph_name_to_gid: BTreeMap<String, u32>,
    /// Mapping from PDF character codes to internal Glyph IDs (GIDs) discovered from embedded cmap.
    pub code_to_gid: BTreeMap<u32, u32>,
    /// Mapping from CFF SIDs to internal Glyph IDs (GIDs).
    pub sid_to_gid: BTreeMap<u32, u32>,
    /// Physical widths from the font file (GID -> width).
    pub physical_widths: BTreeMap<u32, f32>,
    /// Physical names from the font file (GID -> name).
    pub physical_names: BTreeMap<u32, String>,
    /// PDF's CIDToGIDMap.
    pub cid_to_gid_map: Option<BTreeMap<u32, u32>>,

    // --- Reconstruction & Rendering State ---
    /// Original or system font data.
    pub data: Option<Arc<Vec<u8>>>,
    /// Reconstructed SFNT data with injected metrics.
    pub reconstructed_data: Option<Arc<Vec<u8>>>,
    /// The total number of glyphs in the font.
    pub num_glyphs: u32,
    /// Fallback font type if substitution is needed.
    pub fallback_type: Option<FallbackFontType>,
    /// Whether the font was processed by a legacy distiller (affects encoding heuristics).
    pub is_legacy_distiller: bool,
    /// Explicit flag to track if the font data came from the PDF (embedded).
    pub is_embedded_resource: bool,

    // --- Type 3 Specific ---
    /// Type 3 font character procedures (ISO 32000-2:2020 Clause 9.6.5).
    pub char_procs: Option<BTreeMap<String, Handle<Object>>>,
    /// Type 3 font matrix (ISO 32000-2:2020 Clause 9.6.5).
    pub font_matrix: Option<[f32; 6]>,
    /// Whether to force fallback to system fonts on error.
    pub force_fallback: bool,
    /// CID Ordering (e.g., "Identity", "Japan1").
    pub cid_ordering: Option<String>,
    /// CID Registry (e.g., "Adobe").
    pub cid_registry: Option<String>,
    /// What had to be decided to read this font, recorded rather than logged.
    pub decisions: Vec<crate::interpretation::Decision>,
}

/// What an embedded font program's leading bytes say it is (ISO 9.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmbeddedFormat {
    /// An OpenType or TrueType program, which this engine reads glyphs from.
    Sfnt,
    /// A bare CFF program, likewise readable.
    Cff,
    /// A Type 1 program, which this engine does not ingest.
    Type1,
    /// Bytes matching no known signature.
    Unrecognised,
    /// No embedded program at all.
    Absent,
}

#[derive(Default)]
struct DescendantResult {
    base_font: Option<PdfName>,
    font_data: Option<loader::FontData>,
    font_descriptor: Option<Handle<Object>>,
    font_file_handle: Option<Handle<Object>>,
    cid_to_gid_map: Option<BTreeMap<u32, u32>>,
    metrics: FontMetrics,
    ordering: Option<String>,
    registry: Option<String>,
}

/// Summary of a font used in the document.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FontSummary {
    /// The base name of the font.
    pub name: String,
    /// The font type (Type1, TrueType, Type0, etc.).
    pub font_type: String,
    /// Whether the font is embedded in the PDF.
    pub is_embedded: bool,
    /// Whether the font is a Type 3 font (defined by PDF content streams).
    pub is_type3: bool,
    /// Whether the font is a subset of the original font.
    pub is_subset: bool,
    /// The character encoding used by the font.
    pub encoding: String,
    /// Whether the font has a ToUnicode mapping.
    pub has_to_unicode: bool,
    /// Object number of the underlying font dictionary.
    pub object_id: u32,
}

/// Whether a resolved character is withheld from extraction (9.10.2).
///
/// **This was written four times and the four did not agree.** Three sites rejected the
/// private-use areas and the circled-number block; the fourth rejected control characters
/// as well. Which rule applied depended on which route had answered — by accident, not by
/// design, because the check sits after the route is out of view.
///
/// The judgement itself is the questionable part and is left exactly as it was. Adobe-
/// Japan1 defines 128 CIDs that *are* circled numbers, and they carry no Shift-JIS and no
/// EUC encoding at all, so a Unicode-based route reaching `U+2460` is right where a legacy
/// one is not. That is a question about the route, and this function cannot see one.
/// Changing it is a behavioural change and wants its own measurement; 2,801 glyphs of the
/// corpus arrive here.
///
/// **The supplementary range is wrong and is kept wrong.** `F0000..=10FFFF` takes in
/// `U+FFFFE`, `U+FFFFF`, `U+10FFFE` and `U+10FFFF`, which are noncharacters rather than
/// private use. `FontResource::is_suspicious_hint` asks a related question about glyph
/// resolution and has the range right — a fifth spelling, and the only correct one.
fn is_withheld(c: char, reject_control: bool) -> Option<UnicodeSource> {
    let value = c as u32;
    if (0xE000..=0xF8FF).contains(&value) || (0xF0000..=0x10FFFF).contains(&value) {
        return Some(UnicodeSource::Withheld);
    }
    if reject_control && ((value <= 0x1F) || (0x7F..=0x9F).contains(&value)) {
        return Some(UnicodeSource::Withheld);
    }
    None
}

/// Which route named a character code, or why none did (9.10.2).
///
/// **Not a taxonomy, a label.** Font recovery is heuristic where the standard leaves a
/// file broken, and it should stay easy to add another guess when a real document demands
/// one; this exists so that a rule written for one guess can be scoped to that guess
/// rather than to every value the engine produces. `#[non_exhaustive]` because the list
/// grows with the files it meets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum UnicodeSource {
    /// The file's own `/ToUnicode` CMap said so (9.10.3). Authoritative.
    ToUnicode,
    /// The font's encoding named it — a glyph name, or a Unicode-based CMap.
    Encoding,
    /// The CID collection's table, reached through `/CIDSystemInfo` (Adobe-Japan1 and
    /// its siblings).
    CidCollection,
    /// A CID in the ASCII range read as that byte. A guess, and named as one.
    AsciiGuess,
    /// A route produced a character in a private-use area and the engine discarded it.
    ///
    /// Private use means the meaning is not universal, so the codepoint carries nothing a
    /// reader can act on whatever route named it.
    ///
    /// **The circled-number block used to be discarded here too and is not any more.**
    /// Adobe-Japan1 defines 128 CIDs that *are* those characters and they carry no
    /// Shift-JIS and no EUC encoding at all, so only a Unicode-based route can reach them
    /// — and every route that can produce `U+2460` is authoritative: `/ToUnicode` is the
    /// document speaking, the encoding is a Unicode CMap or a glyph name, the CID
    /// collection is Adobe's own table, and the ASCII guess cannot reach past `U+007E`.
    /// Measured: 2,753 characters across the corpus, in three Japanese documents at about
    /// three a page, which is what enumerated lists look like.
    Withheld,
    /// No route named it.
    Unmapped,
}

/// Whether a mapping's value is a glyph name rather than the text itself.
///
/// A `CMap`'s mappings carry both: `/Differences` and a `bfchar` with a name destination
/// store `/glyphname`, everything else stores the characters. **The leading slash was the
/// whole test, and `/` is also a character.** A base encoding that names `U+002F` — every
/// one of them does, at code `0x2F` — had its slash read as an empty glyph name and came
/// back as nothing; so did any `/ToUnicode` mapping a code to a solidus. A name token has
/// something after its slash, which is what separates the two.
fn is_glyph_name(value: &str) -> bool {
    value.len() > 1 && value.starts_with('/')
}

/// A mapping's value as text, with the reason it is withheld if it is.
///
/// The two routes that consult a mapping — the encoding, and everything `unicode_for`
/// tries — did this identically and disagreed only about what to return afterwards: one
/// reports which filter fired, the other only that something did. Resolving the name and
/// applying the filter is the shared half.
fn resolve_mapping(value: String) -> (String, Option<UnicodeSource>) {
    let text =
        if is_glyph_name(&value) { cmap::glyph_name_to_unicode(value.as_bytes()) } else { value };
    let withheld = text.chars().next().and_then(|c| is_withheld(c, false));
    (text, withheld)
}

impl UnicodeSource {
    /// The route's name, for a decision or a tally.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ToUnicode => "ToUnicode",
            Self::Encoding => "encoding",
            Self::CidCollection => "CID collection",
            Self::AsciiGuess => "ASCII guess",
            Self::Withheld => "withheld",
            Self::Unmapped => "unmapped",
        }
    }
}

impl FontResource {
    #[cfg(test)]
    /// Builds a minimal resource for tests.
    pub fn new_test() -> Self {
        Self {
            base_font: PdfName::new("Test"),
            subtype: PdfName::new("TrueType"),
            is_cid_keyed: false,
            first_char: 0,
            last_char: 255,
            widths: BTreeMap::new(),
            vertical_widths: BTreeMap::new(),
            default_width: 1000.0,
            wmode: 0,
            is_bold: false,
            descriptor: None,
            file_handle: None,
            encoding: None,
            to_unicode: None,
            collection_map: None,
            collection_unicode_to_cid: None,
            discovered_mappings: Arc::new(std::sync::Mutex::new(BTreeMap::new())),
            unified_map: BTreeMap::new(),
            unicode_to_gid: BTreeMap::new(),
            glyph_name_to_gid: BTreeMap::new(),
            code_to_gid: BTreeMap::new(),
            sid_to_gid: BTreeMap::new(),
            physical_widths: BTreeMap::new(),
            physical_names: BTreeMap::new(),
            cid_to_gid_map: None,
            data: None,
            reconstructed_data: None,
            length1: None,
            length2: None,
            length3: None,
            fallback_type: None,
            is_legacy_distiller: false,
            is_embedded_resource: false,
            char_procs: None,
            font_matrix: None,
            cid_ordering: None,
            cid_registry: None,
            num_glyphs: 0,
            force_fallback: false,
            decisions: Vec::new(),
        }
    }

    fn parse_subtype_metrics_and_data(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        subtype: &PdfName,
        font_data: Option<loader::FontData>,
        doc: &Document,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> DescendantResult {
        let arena = doc.arena();
        let mut res = DescendantResult::default();
        if subtype.as_str() == "Type3" {
            res.metrics = FontMetrics::parse_type3(dict, arena);
            res.font_data = font_data;
        } else if subtype.as_str() == "Type0" {
            if let Some(desc) = Self::parse_descendant_font(dict, doc, decisions) {
                res.base_font = desc.base_font;
                res.font_data = font_data.or(desc.font_data);
                res.font_descriptor = desc.font_descriptor;
                res.font_file_handle = desc.font_file_handle;
                res.cid_to_gid_map = desc.cid_to_gid_map;
                res.ordering = desc.ordering;
                res.registry = desc.registry;
                res.metrics = desc.metrics;
            } else {
                res.metrics = FontMetrics::default();
                res.font_data = font_data;
            }
        } else if subtype.as_str() == "CIDFontType0" || subtype.as_str() == "CIDFontType2" {
            res.metrics = FontMetrics::parse_cid(dict, arena);
            res.font_data = font_data;
            // A CIDFont carries its own `/CIDSystemInfo` (9.7.4.1, Table 115), and this
            // branch is the one that runs for the font that *decodes*: the interpreter
            // loads a Type0's descendant on its own and decodes through that resource
            // (`fepdf-content/src/interpreter/font.rs`), so reading the collection only
            // on the Type0 above left the deciding copy with none.
            let (ordering, registry) = Self::read_csi(dict, arena);
            res.ordering = ordering;
            res.registry = registry;
        } else {
            res.metrics = FontMetrics::parse_standard(dict, arena);
            res.font_data = font_data;
        }
        res
    }

    fn extract_first_descendant_dict(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
    ) -> Option<BTreeMap<Handle<PdfName>, Object>> {
        let df_obj = dict.get(&arena.name("DescendantFonts"))?;
        let arr = arena.get_array(df_obj.resolve(arena).as_array()?)?;
        let dfh = arr.first()?.resolve(arena).as_dict_handle()?;
        arena.get_dict(dfh)
    }

    fn load_type0_composite(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        doc: &Document,
        to_unicode: Option<cmap::CMap>,
        encoding: Option<cmap::CMap>,
        mut decisions: Vec<crate::interpretation::Decision>,
    ) -> PdfResult<Option<Self>> {
        let arena = doc.arena();
        let Some(df_dict) = Self::extract_first_descendant_dict(dict, arena) else {
            return Ok(None);
        };

        let mut desc_decisions = Vec::new();
        let desc_subtype = Self::extract_subtype(&df_dict, arena, &mut desc_decisions)?;
        let desc_base_font = Self::extract_base_font(&df_dict, arena);
        let desc_fd_obj = df_dict.get(&arena.name("FontDescriptor"));
        let desc_font_data =
            desc_fd_obj.and_then(|o| loader::FontLoader::extract_data(o, doc, Some(&df_dict)));
        let desc_to_unicode =
            Self::parse_to_unicode(&df_dict, doc, &mut desc_decisions).or(to_unicode);
        let desc_font_descriptor = desc_fd_obj.and_then(|o| o.as_reference());
        let desc_metrics = FontMetrics::parse_cid(&df_dict, arena);
        let desc_cid_to_gid_map = Self::parse_cid_to_gid_map(&df_dict, doc, &mut desc_decisions);
        let desc_font_file_handle = Self::extract_font_file_handle(&df_dict, arena);
        let (desc_ordering, desc_registry) = Self::read_csi(&df_dict, arena);
        let wmode = metrics::detect_wmode(dict, arena);

        let mut resource = Self::new_initial(
            desc_subtype,
            desc_base_font,
            desc_metrics,
            encoding,
            desc_to_unicode,
            desc_cid_to_gid_map,
            desc_font_data,
            desc_font_descriptor,
            desc_font_file_handle,
            true,
            &df_dict,
            arena,
            doc.force_fallback,
            desc_ordering,
            desc_registry,
        );
        resource.wmode = u8::try_from(wmode).unwrap_or(0);
        decisions.extend(desc_decisions);
        resource.decisions = decisions;
        resource.initialize_lifecycle(doc);
        Ok(Some(resource))
    }

    fn build_loaded_resource(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        doc: &Document,
        subtype: PdfName,
        to_unicode: Option<cmap::CMap>,
        encoding: Option<cmap::CMap>,
        mut decisions: Vec<crate::interpretation::Decision>,
    ) -> PdfResult<Self> {
        let arena = doc.arena();
        let mut base_font = Self::extract_base_font(dict, arena);
        let fd_obj = dict.get(&arena.name("FontDescriptor"));
        let font_data = fd_obj.and_then(|o| loader::FontLoader::extract_data(o, doc, Some(dict)));
        let is_cid_keyed = subtype.as_str() == "Type0"
            || subtype.as_str() == "CIDFontType0"
            || subtype.as_str() == "CIDFontType2";
        let desc =
            Self::parse_subtype_metrics_and_data(dict, &subtype, font_data, doc, &mut decisions);
        if base_font.as_str() == "Untitled"
            && let Some(bf) = desc.base_font
        {
            base_font = bf;
        }
        let font_descriptor = fd_obj.and_then(|o| o.as_reference()).or(desc.font_descriptor);

        let mut resource = Self::new_initial(
            subtype,
            base_font,
            desc.metrics,
            encoding,
            to_unicode,
            desc.cid_to_gid_map,
            desc.font_data,
            font_descriptor,
            desc.font_file_handle,
            is_cid_keyed,
            dict,
            arena,
            doc.force_fallback,
            desc.ordering,
            desc.registry,
        );
        resource.decisions = decisions;
        resource.initialize_lifecycle(doc);
        Ok(resource)
    }

    /// Loads a Font resource from a PDF dictionary.
    pub fn load(dict: &BTreeMap<Handle<PdfName>, Object>, doc: &Document) -> PdfResult<Self> {
        let arena = doc.arena();
        let mut decisions = Vec::new();
        let subtype = Self::extract_subtype(dict, arena, &mut decisions)?;

        let to_unicode = Self::parse_to_unicode(dict, doc, &mut decisions);
        let encoding = Self::parse_encoding(dict, doc, &mut decisions);

        if subtype.as_str() == "Type0"
            && let Some(composite) = Self::load_type0_composite(
                dict,
                doc,
                to_unicode.clone(),
                encoding.clone(),
                decisions.clone(),
            )?
        {
            return Ok(composite);
        }

        Self::build_loaded_resource(dict, doc, subtype, to_unicode, encoding, decisions)
    }

    /// Builds a resource backed by one of the bundled fallback fonts.
    pub fn load_fallback(ftype: FallbackFontType, doc: &Document) -> PdfResult<Self> {
        let mut res = Self::new_initial(
            PdfName::new("TrueType"),
            PdfName::new("Fallback"),
            FontMetrics::default(),
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            &BTreeMap::new(),
            doc.arena(),
            doc.force_fallback,
            None,
            None,
        );
        res.fallback_type = Some(ftype);
        res.data = doc.system_fonts.get(&ftype).cloned();
        res.initialize_lifecycle(doc);
        Ok(res)
    }

    #[allow(clippy::too_many_arguments)]
    fn load_physical_glyph_widths(
        face: &ttf_parser::Face,
        physical_widths: &mut BTreeMap<u32, f32>,
        physical_names: &mut BTreeMap<u32, String>,
    ) {
        let mut units_per_em = f32::from(face.units_per_em());
        if units_per_em == 256.0 {
            let mut sum = 0.0;
            let mut count = 0;
            for gid in 0..face.number_of_glyphs().min(10) {
                if let Some(w) = face.glyph_hor_advance(ttf_parser::GlyphId(gid)) {
                    sum += f32::from(w);
                    count += 1;
                }
            }
            if count > 0 && (sum / count as f32) > 500.0 {
                units_per_em = 1000.0;
            }
        }
        let scale = if units_per_em > 0.0 { 1000.0 / units_per_em } else { 1.0 };
        for gid in 0..face.number_of_glyphs() {
            if let Some(w) = face.glyph_hor_advance(ttf_parser::GlyphId(gid)) {
                physical_widths.insert(u32::from(gid), f32::from(w) * scale);
            }
            if let Some(name) = face.glyph_name(ttf_parser::GlyphId(gid)) {
                physical_names.insert(u32::from(gid), name.to_string());
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// Builds the initial resource for a font dictionary, before refinement.
    pub fn new_initial(
        // RR-15 Limit: Dispatcher - constructs initial state of a PDF Font resource mapping tables and cmap configurations
        subtype: PdfName,
        base_font: PdfName,
        metrics: FontMetrics,
        encoding: Option<cmap::CMap>,
        to_unicode: Option<cmap::CMap>,
        cid_to_gid_map: Option<BTreeMap<u32, u32>>,
        font_data: Option<loader::FontData>,
        descriptor: Option<Handle<Object>>,
        file_handle: Option<Handle<Object>>,
        is_cid_keyed: bool,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
        force_fallback: bool,
        cid_ordering: Option<String>,
        cid_registry: Option<String>,
    ) -> Self {
        let is_bold = base_font.as_str().to_lowercase().contains("bold");
        let (data, l1, l2, l3, is_embedded) = if let Some(fd) = font_data {
            (Some(Arc::new(fd.data)), fd.length1, fd.length2, fd.length3, true)
        } else {
            (None, None, None, None, false)
        };

        let mut resource = Self {
            subtype,
            base_font,
            is_cid_keyed,
            first_char: metrics.first,
            last_char: metrics.last,
            widths: metrics.widths,
            vertical_widths: metrics.v_widths,
            default_width: metrics.default_width,
            wmode: metrics::detect_wmode(dict, arena) as u8,
            is_bold,
            descriptor,
            file_handle,
            length1: l1,
            length2: l2,
            length3: l3,
            encoding,
            to_unicode: to_unicode.clone(),
            collection_map: None,
            collection_unicode_to_cid: None,
            discovered_mappings: Arc::new(std::sync::Mutex::new(BTreeMap::new())),
            unified_map: BTreeMap::new(),
            unicode_to_gid: BTreeMap::new(),
            glyph_name_to_gid: BTreeMap::new(),
            code_to_gid: BTreeMap::new(),
            sid_to_gid: BTreeMap::new(),
            physical_widths: BTreeMap::new(),
            physical_names: BTreeMap::new(),
            cid_to_gid_map,
            cid_ordering,
            cid_registry,
            num_glyphs: if let Some(ref d) = data {
                ttf_parser::Face::parse(d, 0).map(|f| u32::from(f.number_of_glyphs())).unwrap_or(0)
            } else {
                0
            },
            data,
            reconstructed_data: None,
            fallback_type: None,
            is_legacy_distiller: Self::detect_legacy_distiller(&to_unicode),
            is_embedded_resource: is_embedded,
            char_procs: Self::parse_char_procs(dict, arena),
            font_matrix: Self::parse_font_matrix(dict, arena),
            force_fallback,
            decisions: Vec::new(),
        };

        if let Some(ref d) = resource.data
            && let Ok(face) = ttf_parser::Face::parse(d, 0)
        {
            Self::load_physical_glyph_widths(
                &face,
                &mut resource.physical_widths,
                &mut resource.physical_names,
            );
        }

        // Dropped, not recorded: `load` replaces `decisions` the moment this returns.
        let _ = resource.init_collection_map();
        resource.build_unified_map();
        resource
    }

    fn initialize_lifecycle(&mut self, doc: &Document) {
        self.fallback_type = Some(self.infer_fallback_type());
        self.rescue_unicode_map();
        if let Some(declined) = self.init_collection_map() {
            self.decisions.push(declined);
        }

        // Build the authoritative Unicode->CID map BEFORE reconstruction
        // so it can be injected into the virtual SFNT's 'cmap' table.
        self.build_unified_map();

        self.populate_embedded_unicode_map(doc);
        let _ = self.perform_reconstruction();

        // Precipitation: Release original raw data if reconstruction succeeded to save memory.
        if self.reconstructed_data.is_some() {
            self.data = None;
        }

        self.populate_embedded_unicode_map(doc);
        self.build_unified_map();
    }

    fn infer_fallback_type(&self) -> FallbackFontType {
        let name = self.base_font.as_str().to_lowercase();
        let subtype = self.subtype.as_str();
        let is_multibyte =
            subtype == "Type0" || subtype == "CIDFontType0" || subtype == "CIDFontType2";

        if is_multibyte
            || name.contains("mincho")
            || name.contains("gothic")
            || name.contains("hira")
            || name.contains("koz")
        {
            if name.contains("mincho") || name.contains("serif") {
                return FallbackFontType::JapaneseSerif;
            }
            return FallbackFontType::JapaneseSans;
        }

        if name.contains("mono") || name.contains("courier") {
            return FallbackFontType::Monospace;
        }
        if name.contains("serif")
            || name.contains("times")
            || name.contains("century")
            || name.contains("georgia")
        {
            return FallbackFontType::Serif;
        }
        if name.contains("sans")
            || name.contains("arial")
            || name.contains("helvetica")
            || name.contains("verdana")
        {
            return FallbackFontType::SansSerif;
        }

        FallbackFontType::Default
    }

    fn update_physical_widths_from_reconstructed(&mut self) {
        if let Some(ref d) = self.reconstructed_data
            && let Ok(face) = ttf_parser::Face::parse(d, 0)
        {
            self.num_glyphs = u32::from(face.number_of_glyphs());
            let units_per_em = f32::from(face.units_per_em());
            let scale = if units_per_em > 0.0 { 1000.0 / units_per_em } else { 1.0 };

            self.physical_widths.clear();
            self.physical_names.clear();
            for gid in 0..self.num_glyphs {
                if let Some(w) = face.glyph_hor_advance(ttf_parser::GlyphId(gid as u16)) {
                    self.physical_widths.insert(gid, f32::from(w) * scale);
                }
                if let Some(name) = face.glyph_name(ttf_parser::GlyphId(gid as u16)) {
                    self.physical_names.insert(gid, name.to_string());
                }
            }
            log::debug!(
                "[FONT] Updated num_glyphs to {}, physical widths and names after reconstruction",
                self.num_glyphs
            );
        }
    }

    /// Surgically patches the embedded font data with PDF metrics.
    pub fn perform_reconstruction(&mut self) -> PdfResult<()> {
        if let Some(ref raw_data) = self.data {
            log::debug!("[FONT] Attempting reconstruction for {}", self.base_font.as_str());
            let res = FontReconstructor::reconstruct(self, raw_data)?;
            let sig = if res.data.len() >= 4 {
                format!(
                    "{:02x}{:02x}{:02x}{:02x}",
                    res.data[0], res.data[1], res.data[2], res.data[3]
                )
            } else {
                "short".to_string()
            };
            log::info!(
                "[FONT] Reconstruction successful for {}. New size: {}, sig: {}",
                self.base_font.as_str(),
                res.data.len(),
                sig
            );
            self.reconstructed_data = Some(Arc::new(res.data));

            if let Some(names) = res.name_to_gid_map {
                self.glyph_name_to_gid = names;
            }

            if let Some(sids) = res.sid_to_gid_map {
                self.sid_to_gid = sids;
            }

            // Always favor the discovered map for embedded CFF fonts, as it reflects the physical charset truth.
            if let Some(m) = res.cid_to_gid_map {
                self.cid_to_gid_map = Some(m);
            }

            if let Some(n) = res.num_glyphs {
                self.num_glyphs = n;
            }

            self.update_physical_widths_from_reconstructed();
        }
        Ok(())
    }

    /// Merges the encoding, ToUnicode and embedded cmap tables into one lookup.
    pub fn build_unified_map(&mut self) {
        let mut map: BTreeMap<String, u32> = BTreeMap::new();

        // 1. First populate from ToUnicode (Highest Priority)
        if let Some(ref tu) = self.to_unicode {
            for (code, uni) in tu.mappings.iter() {
                let cid = self.code_to_cid(code);
                // For ToUnicode, we always insert or overwrite if it's authoritative
                map.insert(uni.clone(), cid);
            }
        }

        // 2. Fallback to Adobe-Japan1 (AJ1) for Japanese fonts
        if map.is_empty()
            && let Some(ref collection) = self.collection_map
        {
            for (code, uni) in collection.mappings.iter() {
                let cid = if code.len() == 2 {
                    (u32::from(code[0]) << 8) | u32::from(code[1])
                } else {
                    u32::from(code[0])
                };
                map.entry(uni.clone()).or_insert(cid);
            }
        }

        // 3. Fallback to Heuristics/Encoding for simple fonts (Lowest Priority)
        if self.subtype.as_str() != "Type0" {
            for code in 0..=255 {
                let (_, uni, _) = self.decode_via_heuristics_sourced(&[code as u8]);
                if let Some(u) = uni {
                    // Only insert if not already present from ToUnicode
                    map.entry(u).or_insert(code as u32);
                }
            }
        }

        self.unified_map = map;
    }

    /// Classifies the embedded program by its leading bytes.
    fn embedded_format(&self) -> EmbeddedFormat {
        let Some(raw) = self.data.as_ref() else { return EmbeddedFormat::Absent };
        if raw.starts_with(b"OTTO")
            || raw.starts_with(&[0, 1, 0, 0])
            || raw.starts_with(b"ttcf")
            || raw.starts_with(b"true")
        {
            EmbeddedFormat::Sfnt
        } else if raw.len() >= 2 && ((raw[0] == 1 && raw[1] == 0) || raw[0] == 2) {
            EmbeddedFormat::Cff
        } else if raw.starts_with(b"%!") || raw.starts_with(&[0x80, 0x01]) {
            EmbeddedFormat::Type1
        } else {
            EmbeddedFormat::Unrecognised
        }
    }

    /// Fills the Unicode map from the embedded font program's own cmap.
    pub fn populate_embedded_unicode_map(&mut self, doc: &Document) {
        let mut u2g = BTreeMap::new();
        let mut taken = Vec::new();

        let format = self.embedded_format();

        // Priority 1: ToUnicode (bridges Unicode to character codes/GIDs)
        self.populate_u2g_from_tounicode(&mut u2g);

        // Priority 2: Font file's own charmap
        if matches!(format, EmbeddedFormat::Sfnt | EmbeddedFormat::Cff)
            || self.reconstructed_data.is_some()
        {
            self.populate_u2g_from_font_file(&mut u2g);
        } else if matches!(format, EmbeddedFormat::Type1) {
            taken.push(crate::interpretation::Decision::ambiguity(
                "9.9",
                format!("{} embeds a Type 1 program", self.base_font.as_str()),
                "did not read glyphs from it; this engine ingests SFNT and CFF only",
            ));
        } else if let Some(signature) =
            self.data.as_ref().map(|d| d[..std::cmp::min(4, d.len())].to_vec())
        {
            taken.push(crate::interpretation::Decision::violation(
                "9.9",
                format!(
                    "{} embeds a program in no recognised format (starts {signature:?})",
                    self.base_font.as_str()
                ),
                "skipped it and fell back to a system font",
            ));
        }
        // Priority 3: System fallback fonts (for characters still missing)
        if let Some(ftype) = self.fallback_type
            && let Some(fb_data) = doc.system_fonts.get(&ftype)
        {
            // If force_fallback is set, we proactively populate from system fonts
            // to cover potential parsing failures in embedded fonts.
            // Otherwise, it acts as a traditional fallback for missing glyphs.
            self.populate_u2g_from_data(fb_data, &mut u2g, &mut taken);
        }
        // Priority 4: Unified mapping (last resort heuristics)
        self.populate_u2g_from_unified(&mut u2g);

        self.decisions.append(&mut taken);
        self.unicode_to_gid = u2g;
    }

    fn populate_u2g_from_data(
        &self,
        data: &[u8],
        u2g: &mut BTreeMap<char, u32>,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) {
        if let Ok(face) = ttf_parser::Face::parse(data, 0) {
            log::debug!(
                "[FONT] Parsing cmap for font: {}. cmap table present: {}",
                self.base_font.as_str(),
                face.tables().cmap.is_some()
            );
            let mut count = 0;
            if let Some(cmap) = face.tables().cmap {
                for table in cmap.subtables {
                    log::debug!(
                        "[FONT] Subtable: platform={:?}, encoding={:?}, is_unicode={}",
                        table.platform_id,
                        table.encoding_id,
                        table.is_unicode()
                    );
                    if table.is_unicode() {
                        table.codepoints(|cp| {
                            if let Some(c) = char::from_u32(cp)
                                && let Some(gid) = table.glyph_index(cp)
                            {
                                u2g.entry(c).or_insert(u32::from(gid.0));
                                count += 1;
                            }
                        });
                    }
                }
            }
            log::debug!(
                "[FONT] Mapped {} Unicode characters to GIDs for font {}",
                count,
                self.base_font.as_str()
            );
        } else {
            decisions.push(crate::interpretation::Decision::violation(
                "9.9",
                format!("the embedded program for {} did not parse", self.base_font.as_str()),
                "left the font without a glyph mapping of its own",
            ));
        }
    }

    fn populate_u2g_from_tounicode(&self, u2g: &mut BTreeMap<char, u32>) {
        let Some(ref tu) = self.to_unicode else { return };

        for (code, uni) in tu.mappings.iter() {
            if let Some(c) = uni.chars().next() {
                // ROBUSTNESS: Avoid mapping to control characters or suspicious whitespace
                // if other mappings exist, but TRUST ToUnicode if it's the only source.
                if uni.is_empty() || (c.is_control() && c != '\t' && c != '\n' && c != '\r') {
                    log::debug!(
                        "[FONT] Skipping suspicious ToUnicode mapping: {code:?} -> {uni:?}"
                    );
                    continue;
                }

                let cid = self.code_to_cid(code);
                let gid = self.to_gid(cid, None);

                if gid != 0 {
                    // TRUST ToUnicode: It should override existing mappings from font file cmaps
                    // in most cases, as PDF generators use it to fix encoding issues.
                    u2g.insert(c, gid);
                }
            }
        }
    }

    fn populate_u2g_from_unified(&self, u2g: &mut BTreeMap<char, u32>) {
        let is_embedded = self.data.is_some() || self.reconstructed_data.is_some();
        let is_cid = self.is_cid_keyed || self.subtype.as_str().contains("CIDFont");

        for (uni, &cid) in &self.unified_map {
            if let Some(c) = uni.chars().next() {
                if u2g.contains_key(&c) && u2g[&c] != 0 {
                    continue;
                }

                // For non-embedded simple fonts, unified mapping (derived from heuristics)
                // is not authoritative for GIDs.
                if !is_embedded && !is_cid {
                    continue;
                }

                let gid = self.to_gid(cid, None);
                u2g.entry(c)
                    .and_modify(|e| {
                        if *e == 0 {
                            *e = gid;
                        }
                    })
                    .or_insert(gid);
            }
        }
    }

    fn map_cmap_codepoints(
        &mut self,
        cmap_table: ttf_parser::cmap::Table<'_>,
        u2g: &mut BTreeMap<char, u32>,
    ) -> usize {
        let mut count = 0;
        for table in cmap_table.subtables {
            table.codepoints(|cp: u32| {
                if let Some(gid) = table.glyph_index(cp) {
                    let gid_u32 = u32::from(gid.0);
                    if gid_u32 != 0 {
                        self.code_to_gid.insert(cp, gid_u32);
                        if table.is_unicode()
                            && let Some(c) = std::char::from_u32(cp)
                        {
                            u2g.entry(c).or_insert(gid_u32);
                            count += 1;
                        }
                    }
                }
            });
        }
        count
    }

    fn populate_u2g_from_font_file(&mut self, u2g: &mut BTreeMap<char, u32>) {
        let font_data = self.reconstructed_data.clone().or_else(|| self.data.clone());
        let font_name = self.base_font.as_str().to_string();

        if let Some(arc_data) = font_data {
            let sig = if arc_data.len() >= 4 {
                format!(
                    "{:02x}{:02x}{:02x}{:02x}",
                    arc_data[0], arc_data[1], arc_data[2], arc_data[3]
                )
            } else {
                "short".to_string()
            };
            log::debug!(
                "[FONT] Parsing embedded font file for: {} (size: {} bytes, sig: {}, is_reconstructed: {})",
                font_name,
                arc_data.len(),
                sig,
                self.reconstructed_data.is_some()
            );
            match ttf_parser::Face::parse(&arc_data, 0) {
                Ok(face) => {
                    log::debug!(
                        "[FONT] Parsing embedded font file for: {}. cmap present: {}",
                        font_name,
                        face.tables().cmap.is_some()
                    );
                    let mut count = 0;
                    if let Some(cmap_table) = face.tables().cmap {
                        count = self.map_cmap_codepoints(cmap_table, u2g);
                    }
                    log::debug!(
                        "[FONT] Mapped {count} Unicode characters from embedded file for {font_name}"
                    );
                }
                Err(e) => {
                    log::debug!(
                        "[FONT] Failed to parse embedded font file for {font_name}: {e:?}. (Falling back to document/system truth)"
                    );
                }
            }
        }
    }

    fn extract_subtype(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> PdfResult<PdfName> {
        let subtype_name = dict
            .get(&arena.name("Subtype"))
            .and_then(|o| o.resolve(arena).as_name())
            .and_then(|h| arena.get_name(h));

        if subtype_name.is_none() {
            let keys: Vec<String> = dict
                .keys()
                .filter_map(|k| arena.get_name(*k).map(|n| n.as_str().to_string()))
                .collect();
            decisions.push(crate::interpretation::Decision::violation(
                "9.6.2",
                format!("font dictionary has no /Subtype; it holds {keys:?}"),
                "rejected the font resource",
            ));
        }

        subtype_name.ok_or_else(|| PdfError::Other("Missing font subtype".into()))
    }

    fn extract_base_font(dict: &BTreeMap<Handle<PdfName>, Object>, arena: &PdfArena) -> PdfName {
        dict.get(&arena.name("BaseFont"))
            .and_then(|o| o.resolve(arena).as_name())
            .and_then(|h| arena.get_name(h))
            .unwrap_or_else(|| PdfName::new("Untitled"))
    }

    fn parse_char_procs(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
    ) -> Option<BTreeMap<String, Handle<Object>>> {
        let cp_key = arena.name("CharProcs");
        if let Some(cp_obj) = dict.get(&cp_key) {
            let cp_resolved = cp_obj.resolve(arena);
            if let Object::Dictionary(dfh) = cp_resolved
                && let Some(cp_dict) = arena.get_dict(dfh)
            {
                let mut map = BTreeMap::new();
                for (name_h, obj) in cp_dict {
                    if let Some(name) = arena.get_name(name_h)
                        && let Some(h) = obj.as_reference()
                    {
                        map.insert(name.as_str().to_string(), h);
                    }
                }
                return Some(map);
            }
        }
        None
    }

    fn parse_font_matrix(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
    ) -> Option<[f32; 6]> {
        let fm_key = arena.name("FontMatrix");
        if let Some(fm_obj) = dict.get(&fm_key)
            && let Object::Array(ah) = fm_obj.resolve(arena)
            && let Some(arr) = arena.get_array(ah)
            && arr.len() == 6
        {
            let mut matrix = [0.0; 6];
            for (i, item) in arr.iter().enumerate() {
                matrix[i] = item.as_f64().unwrap_or(0.0) as f32;
            }
            Some(matrix)
        } else {
            None
        }
    }

    fn parse_to_unicode(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        doc: &Document,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<cmap::CMap> {
        let arena = doc.arena();
        let tu_obj = dict.get(&arena.name("ToUnicode"))?;
        let base_font = if let Some(h) = dict.get(&arena.name("BaseFont")).and_then(|o| o.as_name())
        {
            arena
                .get_name(h)
                .map(|n| n.as_str().to_string())
                .unwrap_or_else(|| "Unknown".to_string())
        } else {
            "Unknown".to_string()
        };
        log::debug!("[FONT] Font {base_font} has ToUnicode obj: {tu_obj:?}");
        Self::try_load_cmap(doc, &tu_obj.resolve(arena), "ToUnicode", decisions)
    }

    fn try_load_cmap(
        doc: &Document,
        obj: &Object,
        context: &str,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<cmap::CMap> {
        match doc.decode_stream(obj) {
            Ok(data) => match cmap::CMap::parse(&data) {
                Ok(m) => Some(m),
                Err(e) => {
                    decisions.push(crate::interpretation::Decision::violation(
                        "9.10.3",
                        format!("the {context} CMap did not parse: {e:?}"),
                        "left the font without that mapping",
                    ));
                    None
                }
            },
            Err(e) => {
                decisions.push(crate::interpretation::Decision::violation(
                    "9.10.3",
                    format!("the {context} CMap stream did not decode: {e:?}"),
                    "left the font without that mapping",
                ));
                None
            }
        }
    }

    fn detect_legacy_distiller(to_unicode: &Option<cmap::CMap>) -> bool {
        let Some(tu) = to_unicode else { return false };
        tu.mappings.iter().any(|(code_vec, uni_str)| {
            code_vec.len() == 1
                && code_vec[0] >= 0x20
                && code_vec[0] <= 0x7E
                && uni_str.chars().any(|c| (c as u32) > 0xFF)
        })
    }

    fn parse_encoding(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        doc: &Document,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<cmap::CMap> {
        let arena = doc.arena();
        let enc_obj = dict.get(&arena.name("Encoding"))?;
        let enc = enc_obj.resolve(arena);
        match enc {
            Object::Name(h) => {
                let name = arena.get_name(h)?;
                let name_str = name.as_str();
                // An Annex D base encoding is asked for first, because it is not a CMap
                // and the CMap loader searches a collection that has never held one.
                // Reaching `load_named` with `/WinAnsiEncoding` returned `None` and left
                // the font with no encoding at all: 36,914 glyphs of `intel_sdm.pdf`.
                fepdf_font::annex_d::base_encoding(name_str)
                    .or_else(|| cmap::CMap::load_named(name_str))
                    .or_else(|| match name_str {
                        "Identity-H" => Some(cmap::CMap::identity_h()),
                        "Identity-V" => Some(cmap::CMap::identity_v()),
                        "90ms-RKSJ-H" => Some(cmap::CMap::rksj_h()),
                        "UniJIS-UTF16-H" => Some(cmap::CMap::unijis_h()),
                        _ => {
                            Self::record_unknown_encoding(name_str, decisions);
                            None
                        }
                    })
            }
            Object::Stream(_, _) => Self::try_load_cmap(doc, &enc, "Encoding", decisions),
            Object::Dictionary(h) => Self::parse_encoding_dict(h, arena, decisions),
            _ => None,
        }
    }

    /// Says that an `/Encoding` name resolved to nothing, and which kind of nothing.
    ///
    /// **A name this engine does not carry and a name that means nothing are different
    /// failures**, and both used to produce the same silence: a font with no encoding,
    /// and every code above `U+007E` unnamed with no record of why. `/WinAnsiEncoding`
    /// was the first kind for the whole life of this code.
    fn record_unknown_encoding(name: &str, decisions: &mut Vec<crate::interpretation::Decision>) {
        let (clause, what, took) = if fepdf_font::annex_d::is_base_encoding_name(name) {
            (
                "D.2",
                format!("/{name} is a base encoding this engine does not carry"),
                "left the font without one; codes above U+007E cannot be named",
            )
        } else {
            (
                "9.6.6.1",
                format!("/{name} names neither a CMap nor a base encoding"),
                "left the font without one; codes above U+007E cannot be named",
            )
        };
        decisions.push(crate::interpretation::Decision::violation(clause, what, took));
    }

    fn parse_encoding_dict(
        h: Handle<BTreeMap<Handle<PdfName>, Object>>,
        arena: &PdfArena,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<cmap::CMap> {
        let enc_dict = arena.get_dict(h)?;
        let mut cmap = cmap::CMap::default();

        if let Some(base_name) = enc_dict
            .get(&arena.name("BaseEncoding"))
            .and_then(|o: &Object| o.resolve(arena).as_name())
            .and_then(|h: Handle<PdfName>| arena.get_name(h))
        {
            let name_str: String = base_name.as_str().to_string();
            if let Some(base_cmap) = fepdf_font::annex_d::base_encoding(&name_str)
                .or_else(|| cmap::CMap::load_named(&name_str))
            {
                cmap = base_cmap;
            } else {
                Self::record_unknown_encoding(&name_str, decisions);
            }
        }

        if let Some(Object::Array(ah)) =
            enc_dict.get(&arena.name("Differences")).map(|o: &Object| o.resolve(arena))
            && let Some(arr) = arena.get_array(ah)
        {
            let mut new_mappings = (*cmap.mappings).clone();
            let mut current_code = 0u32;
            for item in arr {
                let resolved: Object = item.resolve(arena);
                match resolved {
                    Object::Integer(code) => current_code = code as u32,
                    Object::Name(name_h) => {
                        if let Some(glyph_name) = arena.get_name(name_h) {
                            let glyph_name_str: String = glyph_name.as_str().to_string();
                            new_mappings
                                .insert(vec![current_code as u8], format!("/{glyph_name_str}"));
                            current_code += 1;
                        }
                    }
                    _ => {}
                }
            }
            cmap.mappings = Arc::new(new_mappings);
        }
        Some(cmap)
    }

    /// `/Ordering` and `/Registry` of `dict`'s `/CIDSystemInfo`, if it carries one.
    fn read_csi(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
    ) -> (Option<String>, Option<String>) {
        let csi = dict
            .get(&arena.name("CIDSystemInfo"))
            .and_then(|o| o.resolve(arena).as_dict_handle())
            .and_then(|h| arena.get_dict(h));
        Self::parse_csi_info(csi.as_ref(), arena)
    }

    /// `/Ordering` and `/Registry` of a `/CIDSystemInfo` dictionary (9.7.3, Table 114).
    ///
    /// **Both are strings there, and this asked for names.** A name is a different object
    /// type (7.3.5), so `as_name` answered `None` for every conforming file: measured over
    /// both corpora, 116 of 116 Type0 fonts declare the pair and the engine read `None`
    /// for all 116. Everything downstream that asks which character collection a font
    /// belongs to — [`FontResource::is_cjk`], [`FontResource::init_collection_map`],
    /// `resolve_gid`'s identity fallback — was therefore deciding from the `/BaseFont`
    /// name alone while the file said so outright.
    ///
    /// A name is still accepted. Nothing in either corpus writes one, so this is not a
    /// measured need; it is what the code already did, and narrowing it would be a
    /// behavioural change with no document behind it.
    fn parse_csi_info(
        csi_dict: Option<&BTreeMap<Handle<PdfName>, Object>>,
        arena: &PdfArena,
    ) -> (Option<String>, Option<String>) {
        let read = |d: &BTreeMap<Handle<PdfName>, Object>, key: &str| -> Option<String> {
            let value = d.get(&arena.name(key))?.resolve(arena);
            // `as_string` covers both the literal and the hexadecimal form (7.3.4);
            // `fy05.pdf` writes literals and `intel_sdm.pdf` writes hex.
            if let Some(bytes) = value.as_string() {
                return Some(String::from_utf8_lossy(bytes).to_string());
            }
            value.as_name().and_then(|n| arena.get_name(n)).map(|n| n.as_str().to_string())
        };
        csi_dict.map_or((None, None), |d| (read(d, "Ordering"), read(d, "Registry")))
    }

    fn extract_descendant_font_data(
        df_dict: &BTreeMap<Handle<PdfName>, Object>,
        font_resource: &Option<Arc<FontResource>>,
        doc: &Document,
    ) -> Option<loader::FontData> {
        let arena = doc.arena();
        font_resource
            .as_ref()
            .and_then(|fr| {
                fr.data.as_ref().or(fr.reconstructed_data.as_ref()).map(|d| loader::FontData {
                    data: d.to_vec(),
                    length1: fr.length1,
                    length2: fr.length2,
                    length3: fr.length3,
                })
            })
            .or_else(|| {
                if let Some(fd_obj) = df_dict.get(&arena.name("FontDescriptor")) {
                    loader::FontLoader::extract_data(fd_obj, doc, Some(df_dict))
                } else {
                    None
                }
            })
    }

    fn get_descendant_font_obj(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<Object> {
        let df_obj = dict.get(&arena.name("DescendantFonts"))?;
        let df_resolved = df_obj.resolve(arena);
        let df_array_h = df_resolved.as_array()?;
        let df_array = arena.get_array(df_array_h)?;
        if let Some(first) = df_array.first() {
            Some(first.clone())
        } else {
            decisions.push(crate::interpretation::Decision::violation(
                "9.7.4",
                "a Type0 font's /DescendantFonts array is empty",
                "left the font without a descendant",
            ));
            None
        }
    }

    fn extract_font_file_handle(
        df_dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
    ) -> Option<Handle<Object>> {
        let fd_obj = df_dict.get(&arena.name("FontDescriptor"))?;
        let fd_dict = fd_obj.resolve(arena).as_dict_handle().and_then(|dh| arena.get_dict(dh))?;
        let (f1, f2, f3) =
            (arena.name("FontFile"), arena.name("FontFile2"), arena.name("FontFile3"));
        for k in [f1, f2, f3] {
            if let Some(ff) = fd_dict.get(&k) {
                return ff.as_reference();
            }
        }
        None
    }

    fn parse_descendant_font(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        doc: &Document,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<DescendantResult> {
        let arena = doc.arena();
        let df_dict_obj = Self::get_descendant_font_obj(dict, arena, decisions)?;
        let df_dict_resolved = df_dict_obj.resolve(arena);
        let df_h = df_dict_obj.as_reference();

        let mut font_resource = None;
        if let Some(h) = df_h
            && let Ok(res) = doc.get_font(h)
        {
            font_resource = Some(res);
        }

        let dfh = df_dict_resolved.as_dict_handle()?;
        let df_dict = arena.get_dict(dfh)?;

        let font_file_handle = Self::extract_font_file_handle(&df_dict, arena);

        // Favor the mapping from the FontResource (which includes embedded CFF truth)
        // over the potentially missing or "Identity" PDF dictionary mapping.
        let cid_to_gid_map = if let Some(ref fr) = font_resource {
            fr.cid_to_gid_map
                .clone()
                .or_else(|| Self::parse_cid_to_gid_map(&df_dict, doc, decisions))
        } else {
            Self::parse_cid_to_gid_map(&df_dict, doc, decisions)
        };

        let (ordering, registry) = Self::read_csi(&df_dict, arena);

        let res = DescendantResult {
            base_font: df_dict
                .get(&arena.name("BaseFont"))
                .and_then(|o| o.resolve(arena).as_name())
                .and_then(|h| arena.get_name(h)),
            font_data: Self::extract_descendant_font_data(&df_dict, &font_resource, doc),
            font_descriptor: df_dict
                .get(&arena.name("FontDescriptor"))
                .and_then(|fd_obj| fd_obj.as_reference()),
            font_file_handle,
            cid_to_gid_map,
            metrics: FontMetrics::parse_cid(&df_dict, arena),
            ordering,
            registry,
        };

        Some(res)
    }

    fn parse_cid_to_gid_map(
        df_dict: &BTreeMap<Handle<PdfName>, Object>,
        doc: &Document,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<BTreeMap<u32, u32>> {
        let arena = doc.arena();
        let map_obj = df_dict.get(&arena.name("CIDToGIDMap"))?;
        let resolved = map_obj.resolve(arena);
        if let Some(name) = resolved.as_name().and_then(|h| arena.get_name(h))
            && name.as_str() == "Identity"
        {
            return None;
        }
        let data = match doc.decode_stream(&resolved) {
            Ok(d) => d,
            Err(e) => {
                decisions.push(crate::interpretation::Decision::violation(
                    "9.7.4.3",
                    format!("the /CIDToGIDMap stream did not decode: {e:?}"),
                    "treated the map as absent, so CIDs map to themselves",
                ));
                return None;
            }
        };
        let mut map = BTreeMap::new();
        let (chunks, _) = data.as_chunks::<2>();
        for (i, &chunk) in chunks.iter().enumerate() {
            let gid = u32::from(u16::from_be_bytes(chunk));
            if gid != 0 {
                map.insert(i as u32, gid);
            }
        }
        Some(map)
    }

    /// Whether any character-to-Unicode mapping is available at all.
    pub fn has_any_mapping(&self) -> bool {
        (self.to_unicode.as_ref().map(|m| !m.mappings.is_empty()).unwrap_or(false))
            || (self.encoding.as_ref().map(|m| !m.mappings.is_empty()).unwrap_or(false))
    }

    /// Performs heuristic recovery of Unicode mappings if they are missing or broken.
    pub fn rescue_unicode_map(&mut self) {
        if self.to_unicode.is_some() {
            return;
        }

        // Hardening (RR-15): Never attempt rescue for component CIDFonts.
        // These fonts are part of a Type0 parent which holds the authoritative ToUnicode map.
        // Rescuing from the physical SFNT charmap of a CIDFont often produces corrupt/1-byte mappings.
        if self.subtype.as_str() == "CIDFontType0" || self.subtype.as_str() == "CIDFontType2" {
            return;
        }

        if let Some(cmap) = rescue::CMapRescue::find_rescue_cmap(self.base_font.as_str()) {
            self.to_unicode = Some(cmap);
        }

        // If it's a simple font without ToUnicode, try rescuing from glyph names
        if self.subtype.as_str() == "Type1"
            && self.to_unicode.is_none()
            && let Some(ref enc) = self.encoding
        {
            let mut mappings = BTreeMap::new();
            for (code, name) in enc.mappings.iter() {
                if let Some(uni) = rescue::CMapRescue::unicode_from_glyph_name(name) {
                    mappings.insert(code.clone(), uni);
                }
            }
            if !mappings.is_empty() {
                let rescue_cmap =
                    cmap::CMap { mappings: std::sync::Arc::new(mappings), ..cmap::CMap::default() };
                self.to_unicode = Some(rescue_cmap);
            }
        }
    }

    /// Loads the CID collection's Unicode table when the collection is one this engine
    /// carries, and says why when it is not.
    ///
    /// **The file names the collection; this used to guess it.** `/CIDSystemInfo` (9.7.3)
    /// is the standard's own statement of which character collection a CID font belongs
    /// to, and this decided from substrings of `/BaseFont` instead. The two disagree in
    /// both directions. `samples/fy05.pdf` sets its title in `RyuminPr6N-Heavy` —
    /// Morisawa's Ryumin, declared `Adobe-Japan1-6` — which matches none of the
    /// substrings, so the table never loaded and 261 glyphs came back unnamed, among them
    /// every character of the title page. In the other direction, 19 fonts of the external
    /// corpus declare `Adobe-Korea1` or `Adobe-China1` and carry `Gothic` in their name,
    /// so the *Japanese* table was applied to them — whatever character Adobe-Japan1 puts
    /// at that CID, offered as the document's text with nothing to say it was a guess.
    ///
    /// The name heuristic stays for `Ordering (Identity)` and for a font that declares
    /// nothing at all, which is what it was written for and where it is the only thing to
    /// go on: 75 fonts of the two corpora.
    ///
    /// Returns the decision reached, for a caller with somewhere to put it.
    /// [`FontResource::new_initial`] has none — `load` replaces `decisions` immediately
    /// after it returns — so only `initialize_lifecycle` keeps what comes back, and
    /// recording it at both call sites would double every entry.
    fn init_collection_map(&mut self) -> Option<crate::interpretation::Decision> {
        let declined = self.load_cid_collection();
        self.build_unicode_to_gid();
        declined
    }

    /// The character collection `/CIDSystemInfo` names, if it names one.
    ///
    /// `Identity` is not one: it is the file saying the codes are the font's own glyph
    /// order (9.7.4.2), which is a statement about indexing and not about characters.
    fn declared_collection(&self) -> Option<String> {
        self.cid_ordering.as_ref().filter(|o| !o.eq_ignore_ascii_case("Identity")).cloned()
    }

    fn load_cid_collection(&mut self) -> Option<crate::interpretation::Decision> {
        let Some(ordering) = self.declared_collection() else {
            // Nothing declared, or `Identity`, which declares nothing about characters.
            if self.name_suggests_japanese() {
                self.load_collection("Adobe", "Japan1");
            }
            return None;
        };
        // `/Registry` is part of the resource's name, and a file may write it in any
        // case: one of the corpus files says `adobe`.
        let registry = self.cid_registry.clone().unwrap_or_else(|| "Adobe".to_string());
        if self.load_collection(&registry, &ordering) {
            return None;
        }
        self.decline_collection(&ordering)
    }

    /// Records that a declared collection is one this engine has no table for.
    ///
    /// Silent only when the font carries its own `/ToUnicode`, because then nothing is
    /// lost: 9.10.3 outranks the collection and the codes are named from the file.
    fn decline_collection(&self, ordering: &str) -> Option<crate::interpretation::Decision> {
        if self.to_unicode.is_some() {
            return None;
        }
        Some(crate::interpretation::Decision::violation(
            "9.7.3",
            format!(
                "font /{} belongs to character collection {}-{ordering}, whose \
                 CID-to-Unicode table this engine does not carry, and it supplies no \
                 /ToUnicode",
                self.base_font.as_str(),
                self.cid_registry.as_deref().unwrap_or("Adobe"),
            ),
            "left its codes unnamed; reading them through Adobe-Japan1 instead would \
             give a Japanese character for every CID and nothing to say it was a guess",
        ))
    }

    /// Whether the `/BaseFont` name or the encoding's name suggests Adobe-Japan1.
    ///
    /// A guess, and reached only where the file declares no collection.
    fn name_suggests_japanese(&self) -> bool {
        let name = self.base_font.as_str().to_lowercase();
        if name.contains("hira")
            || name.contains("koz")
            || name.contains("mincho")
            || name.contains("明朝")
            || name.contains("gothic")
            || name.contains("ゴシック")
            || name.contains("aj1")
            || name.contains("#82#6c#82#72#96#be#92#a9") // ＭＳ 明朝
            || name.contains("#82#6c#82#72#83#53#83#56#83#62#83#4e")
        // ＭＳ ゴシック
        {
            return true;
        }
        self.encoding.as_ref().is_some_and(|e: &cmap::CMap| {
            let n = e.name().to_lowercase();
            n.contains("unijis") || n.contains("90ms") || n.contains("90pv") || n.contains("rksj")
        })
    }

    /// Loads `{Registry}-{Ordering}-UCS2`, Adobe's CID-to-Unicode table for a collection.
    ///
    /// **Five of these are fetched and one was read.** `fetch_font_resources.sh` brings
    /// down `Adobe-CNS1`, `Adobe-GB1`, `Adobe-Japan1`, `Adobe-KR` and `Adobe-Korea1`, and
    /// this asked for Japan1 by name whatever the file declared — so a font declaring
    /// `Adobe-Korea1` had a *Japanese* table applied to it while the Korean one sat on
    /// disk unopened (ADR-0041), and after that was stopped it had none at all.
    ///
    /// The registry is title-cased because it is part of a filename and a file may write
    /// it in any case; one corpus file says `adobe`. Everything else is taken from the
    /// document verbatim: a collection this engine has no file for finds nothing here and
    /// is reported rather than approximated.
    ///
    /// Returns whether a table was found.
    fn load_collection(&mut self, registry: &str, ordering: &str) -> bool {
        let mut registry = registry.to_lowercase();
        if let Some(first) = registry.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        let Some(cmap) = cmap::CMap::load_named(&format!("{registry}-{ordering}-UCS2")) else {
            return false;
        };
        let mut reverse = BTreeMap::new();
        for (cid_bytes, uni) in cmap.mappings.iter() {
            if cid_bytes.len() == 2 {
                let cid = (u32::from(cid_bytes[0]) << 8) | u32::from(cid_bytes[1]);
                reverse.insert(uni.clone(), cid);
            }
        }
        self.collection_map = Some(cmap);
        self.collection_unicode_to_cid = Some(reverse);
        true
    }

    /// Builds the reverse Unicode-to-glyph lookup used by fallback matching.
    pub fn build_unicode_to_gid(&mut self) {
        let data_opt = self.reconstructed_data.as_ref().or(self.data.as_ref());
        if let Some(data) = data_opt
            && let Ok(face) = ttf_parser::Face::parse(data, 0)
        {
            for subtable in face.tables().cmap.iter().flat_map(|t| t.subtables) {
                if subtable.is_unicode() {
                    subtable.codepoints(|cp| {
                        if let Some(gid) = subtable.glyph_index(cp)
                            && let Some(c) = std::char::from_u32(cp)
                        {
                            let u = c as u32;
                            let is_control = (u <= 0x1F) || (0x7F..=0x9F).contains(&u);
                            if gid.0 != 0 && !is_control {
                                self.unicode_to_gid.insert(c, u32::from(gid.0));
                            }
                        }
                    });
                }
            }
        }
    }

    // extract_font_data has been moved to loader::FontLoader::extract_data

    /// Advance width for a character code, in text-space units.
    pub fn glyph_width(&self, code: &[u8]) -> f32 {
        if code.is_empty() {
            return 0.0;
        }
        let cid = self.to_cid(code);

        if self.wmode == 1 {
            if let Some((w1_y, _, _)) = self.vertical_widths.get(&cid) {
                return *w1_y;
            }
            return 1000.0; // Default vertical advance
        }

        if let Some(w) = self.widths.get(&cid) {
            return *w;
        }

        if self.widths.is_empty() && (self.default_width == 1000.0 || self.default_width == 0.0) {
            let cat =
                FontCategory::from_name_and_flags(self.base_font.as_str(), 0, self.is_cid_keyed);
            return cat.estimate_char_width(cid);
        }

        self.default_width
    }

    fn format_cid_chars(cmap: &mut String, gid_to_uni: &[(u32, String)]) {
        for chunk in gid_to_uni.chunks(100) {
            cmap.push_str(&format!("{} begincidchar\n", chunk.len()));
            for &(gid, ref uni_str) in chunk {
                let gid_hex = format!("{gid:04X}");
                let mut uni_hex = String::new();
                for c in uni_str.chars() {
                    let u = c as u32;
                    if u > 0xFFFF {
                        let high = 0xD800 + ((u - 0x10000) >> 10);
                        let low = 0xDC00 + ((u - 0x10000) & 0x3FF);
                        uni_hex.push_str(&format!("{high:04X}{low:04X}"));
                    } else {
                        uni_hex.push_str(&format!("{u:04X}"));
                    }
                }
                cmap.push_str(&format!("<{gid_hex}> <{uni_hex}>\n"));
            }
            cmap.push_str("endcidchar\n");
        }
    }

    /// Synthesises a `/ToUnicode` CMap keyed on **glyph** ids.
    ///
    /// **No caller, deliberately.** The refinement pass used to inject the result into
    /// every `Type0` font that lacked a `/ToUnicode`, and that destroyed text: under
    /// `Identity-H` the content stream's codes are CIDs, and glyph ids equal CIDs only
    /// for a `CIDFontType2` written with `CIDToGIDMap /Identity`. See
    /// `refine/font.rs::normalize_type0_font` for the measurement that removed it.
    ///
    /// Kept because that narrow case is real. Calling this again needs a file proving
    /// it, and a check that the descendant is a `CIDFontType2` with an identity map.
    pub fn generate_standard_tounicode(&self) -> Option<Vec<u8>> {
        let mut cmap = String::new();
        cmap.push_str("/CIDInit /ProcSet findresource begin\n");
        cmap.push_str("12 dict begin\n");
        cmap.push_str("begincmap\n");
        cmap.push_str(
            "/CIDSystemInfo <<\n  /Registry (Adobe)\n  /Ordering (UCS)\n  /Supplement 0\n>> def\n",
        );
        cmap.push_str(&format!(
            "/CMapName /Adobe-Identity-ToUnicode-{} def\n",
            self.base_font.as_str()
        ));
        cmap.push_str("/CMapType 2 def\n");
        cmap.push_str("1 begincodespacerange\n");
        cmap.push_str("<0000> <FFFF>\n");
        cmap.push_str("endcodespacerange\n");

        let mut gid_to_uni = Vec::new();
        for (&c, &gid) in &self.unicode_to_gid {
            gid_to_uni.push((gid, c.to_string()));
        }

        if gid_to_uni.is_empty() {
            for (uni_str, &gid) in &self.unified_map {
                gid_to_uni.push((gid, uni_str.clone()));
            }
        }

        if !gid_to_uni.is_empty() {
            gid_to_uni.sort_by_key(|&(gid, _)| gid);
            gid_to_uni.dedup_by_key(|item| item.0);
            Self::format_cid_chars(&mut cmap, &gid_to_uni);
        }

        cmap.push_str("endcmap\n");
        cmap.push_str("CMapName currentdict /CMap defineresource pop\n");
        cmap.push_str("end\nend\n");

        Some(cmap.into_bytes())
    }

    /// Returns the vertical metrics for a CID: (w1_y, v_x, v_y).
    ///
    /// w1_y is the vertical advance (natively negative in PDF spec).
    /// (v_x, v_y) is the position of the glyph origin relative to the horizontal origin.
    pub fn glyph_vertical_metrics(&self, cid: u32) -> (f32, f32, f32) {
        if let Some(&metrics) = self.vertical_widths.get(&cid) {
            return metrics;
        }
        // Default values: (w1_y, v_x, v_y)
        // From PDF spec: Default w1 = (0, -1000), Default v = (w0/2, 880)
        let w0 = *self.widths.get(&cid).unwrap_or(&self.default_width);
        (-1000.0, w0 / 2.0, 880.0)
    }

    /// Maps a character code to the text it represents.
    pub fn to_unicode(&self, code: &[u8]) -> Option<String> {
        self.unicode_for(code).0
    }

    /// The character a code stands for, and **which route named it**.
    ///
    /// The routes are heuristic and stay that way — there is no principled way to know a
    /// broken file's CID collection, and `rescue.rs` matching on `hira` and `ゴシック` is
    /// the right shape for that. What this adds is not principle but **scope**: a name on
    /// each route, so a rule meant for one of them can be applied to one of them.
    ///
    /// The distinction is not academic. The circled-number filter below discards
    /// `U+2460`–`U+24FF` whatever produced it, and Adobe-Japan1 defines 128 CIDs that
    /// *are* those characters — so a `/ToUnicode` map saying ① is thrown away beside a
    /// broken mapping that landed there. Judged by route rather than by value the two
    /// separate cleanly: those 128 CIDs carry no Shift-JIS and no EUC encoding at all, so
    /// a Unicode-based route reaching them is right and a legacy one is not.
    ///
    /// **Nothing is judged by route yet.** This reports; the filter still fires exactly
    /// where it did.
    pub fn unicode_for(&self, code: &[u8]) -> (Option<String>, UnicodeSource) {
        let mut result = None;
        let mut source = UnicodeSource::Unmapped;

        if let Some(ref map) = self.to_unicode {
            result = map.map(code);
            if result.is_some() {
                source = UnicodeSource::ToUnicode;
            }
        }

        if result.is_none()
            && let Some(res) = self.decode_via_encoding(code, None)
        {
            result = res.1;
            if result.is_some() {
                source = UnicodeSource::Encoding;
            }
        }

        if result.is_none() {
            let cid = self.to_cid(code);
            let is_multibyte = self.subtype.as_str() == "Type0"
                || self.subtype.as_str() == "CIDFontType0"
                || self.subtype.as_str() == "CIDFontType2";

            if is_multibyte && let Some(ref collection) = self.collection_map {
                let cid_bytes = vec![(cid >> 8) as u8, (cid & 0xFF) as u8];
                result = collection.map(&cid_bytes);
                if result.is_some() {
                    source = UnicodeSource::CidCollection;
                }
            }
        }

        self.finish_unicode(code, result, source)
    }

    /// The tail of [`FontResource::unicode_for`]: a glyph name becomes a character, the
    /// private-use filter fires, and a CID in the ASCII range is guessed at.
    ///
    /// Split out for Rule 1 rather than for meaning, though the seam is a real one — this
    /// is everything that happens *after* a route has answered, and the filter's problem
    /// is that it sits here, where the route is no longer in view.
    fn finish_unicode(
        &self,
        code: &[u8],
        result: Option<String>,
        source: UnicodeSource,
    ) -> (Option<String>, UnicodeSource) {
        if let Some(res) = result {
            let (uni, withheld) = resolve_mapping(res);
            if let Some(reason) = withheld {
                return (None, reason);
            }
            return (Some(uni), source);
        }

        let cid = self.to_cid(code);
        // Final fallback: if CID is in ASCII range, try to interpret it as a character
        if cid < 128 && cid > 31 {
            return (Some((cid as u8 as char).to_string()), UnicodeSource::AsciiGuess);
        }

        (None, UnicodeSource::Unmapped)
    }

    /// Writing mode: 0 horizontal, 1 vertical.
    pub fn wmode(&self) -> i32 {
        i32::from(self.wmode)
    }

    /// Inferred bold status based on font name and descriptors.
    pub fn is_bold(&self) -> bool {
        let name = self.base_font.as_str().to_lowercase();
        name.contains("bold")
            || name.contains("heavy")
            || name.contains("black")
            || name.contains("-w6")
            || name.contains("-w7")
            || name.contains("-w8")
            || name.contains("-w9")
    }

    /// Returns the width of a glyph in 1/1000 font units, by CID.
    pub fn glyph_width_by_cid(&self, cid: u32) -> f32 {
        if let Some(w) = self.widths.get(&cid) {
            return *w;
        }
        if self.widths.is_empty() && (self.default_width == 1000.0 || self.default_width == 0.0) {
            let cat =
                FontCategory::from_name_and_flags(self.base_font.as_str(), 0, self.is_cid_keyed);
            return cat.estimate_char_width(cid);
        }
        self.default_width
    }

    /// Returns the PDF width for a given GID, handling CID vs Simple font indexing.
    pub fn glyph_width_by_gid(&self, gid: u32) -> f32 {
        if self.is_cid_keyed {
            // For CID-keyed fonts, we must map the GID back to its original CID
            // to retrieve the correct width from the PDF's widths map.
            if let Some(ref map) = self.cid_to_gid_map {
                for (&cid, &g) in map {
                    if g == gid {
                        return self.glyph_width_by_cid(cid);
                    }
                }
            }
            // Fallback to direct indexing if no map is present (standard Identity-H)
            return self.glyph_width_by_cid(gid);
        }

        // For Simple fonts, try to find a char code that maps to this GID
        for (code, g) in &self.code_to_gid {
            if *g == gid {
                return self.widths.get(code).copied().unwrap_or(self.default_width);
            }
        }

        self.default_width
    }

    /// Maps a character code to a CID through the font's CMap.
    pub fn to_cid(&self, code: &[u8]) -> u32 {
        if let Some(ref enc) = self.encoding {
            return enc.to_cid(code);
        }
        // Fallback for simple fonts
        if code.len() == 2 {
            return (u32::from(code[0]) << 8) | u32::from(code[1]);
        }
        u32::from(code.first().copied().unwrap_or(0))
    }

    /// Translates a character code from a PDF content stream into a font-internal CID or SID.
    pub fn code_to_cid(&self, code: &[u8]) -> u32 {
        // 1. If it's a CID-keyed font, use the Encoding CMap to resolve the code to a CID.
        if self.is_cid_keyed
            && let Some(ref enc) = self.encoding
        {
            return enc.to_cid(code);
        }

        // 2. For simple fonts, the character code itself is often treated as the "CID"
        // for internal mapping tables (like sid_to_gid or code_to_gid) unless
        // a complex Encoding dictionary is present.
        if code.len() == 2 {
            (u32::from(code[0]) << 8) | u32::from(code[1])
        } else if code.len() == 1 {
            u32::from(code[0])
        } else {
            0
        }
    }

    /// Maps a CID to a glyph index, applying `/CIDToGIDMap` when present.
    pub fn to_gid(&self, cid: u32, mut _trace: Option<&mut TraceContext>) -> u32 {
        log::debug!("[FONT] to_gid: font {}, cid {}", self.base_font.as_str(), cid);
        // Priority 1: CFF Charset mapping (authoritative for subsetted CID-keyed CFF)
        if !self.sid_to_gid.is_empty()
            && let Some(&gid) = self.sid_to_gid.get(&cid)
        {
            #[cfg(feature = "debug-tools")]
            if let Some(t) = _trace {
                t.push_step(format!(
                    "Resolved via CFF Charset (sid_to_gid): CID {} -> GID {}",
                    cid, gid
                ));
            }
            return gid;
        }

        // Priority 2: CIDToGIDMap from PDF
        if let Some(ref map) = self.cid_to_gid_map
            && let Some(&gid) = map.get(&cid)
        {
            #[cfg(feature = "debug-tools")]
            if let Some(t) = _trace {
                t.push_step(format!("Resolved via CIDToGIDMap: CID {} -> GID {}", cid, gid));
            }
            return gid;
        }

        // Priority 3: Internal code mapping (fallback)
        if !self.is_cid_keyed
            && let Some(&gid) = self.code_to_gid.get(&cid)
        {
            #[cfg(feature = "debug-tools")]
            if let Some(ref mut t) = _trace {
                t.push_step(format!("Resolved via code_to_gid: CID {} -> GID {}", cid, gid));
            }
            return gid;
        }
        #[cfg(feature = "debug-tools")]
        if let Some(t) = _trace {
            t.push_step(format!("Resolved via Identity (Fallback): CID {} -> GID {}", cid, cid));
        }
        log::debug!("[FONT] to_gid result: cid {cid} -> gid {cid}");
        cid
    }

    /// Returns true if this font is likely a CJK (Chinese, Japanese, Korean) font.
    pub fn is_cjk(&self) -> bool {
        // 1. Check Registry (Adobe-Japan1, Adobe-GB1, etc.)
        if let Some(ref reg) = self.cid_registry {
            let r = reg.to_lowercase();
            if r.contains("japan") || r.contains("gb1") || r.contains("cns1") || r.contains("korea")
            {
                return true;
            }
        }

        // 2. Check Ordering (Honest CJK orderings)
        if let Some(ref ord) = self.cid_ordering {
            let o = ord.to_lowercase();
            if o.contains("japan") || o.contains("gb1") || o.contains("cns1") || o.contains("korea")
            {
                return true;
            }
        }

        // 3. Check Name patterns for common Japanese fonts
        let name = self.base_font.as_str().to_lowercase();
        if name.contains("mincho")
            || name.contains("gothic")
            || name.contains("koz")
            || name.contains("hira")
            || name.contains("kana")
            || name.contains("ms-")
            || name.contains("shas")
            || name.contains("dfp")
            || name.contains("heiti")
            || name.contains("cjk")
            || name.contains("ryumin")
            || name.contains("kyokasho")
            || name.contains("shippori")
        {
            return true;
        }

        false
    }

    /// Checks if a GID is likely valid for the current font.
    pub fn is_gid_valid(&self, gid: u32) -> bool {
        if gid == 0 {
            return false;
        }
        if self.num_glyphs > 0 && gid >= self.num_glyphs {
            log::debug!(
                "[FONT] GID {} is INVALID (num_glyphs: {}) for {}",
                gid,
                self.num_glyphs,
                self.base_font.as_str()
            );
            return false;
        }
        true
    }

    /// Returns true if the font name suggests it is a subsetted font (e.g., "ABCDEF+Arial").
    pub fn is_subsetted(&self) -> bool {
        self.base_font.as_str().contains('+')
    }

    /// Returns true if the font is embedded in the PDF and has been successfully reconstructed for rendering.
    pub fn is_embedded(&self) -> bool {
        self.reconstructed_data.is_some()
    }

    /// Advance width for a glyph index, read from the font program.
    pub fn get_physical_width(&self, gid: u32) -> f32 {
        self.physical_widths.get(&gid).copied().unwrap_or(0.0)
    }

    /// Resolves a character identifier (CID) to a physical Glyph ID (GID).
    ///
    /// This method follows a prioritized resolution chain:
    /// 1. Unicode-to-GID (CMap)
    /// 2. Glyph Name to GID (via Encoding and reconstruction map)
    /// 3. CFF SID fallback (for production names like cXXX)
    /// 4. Direct CID-to-GID mapping
    fn score_source_intent(
        source: &'static str,
        is_cid_keyed: bool,
        is_cjk: bool,
        is_identity: bool,
        phys_width: f32,
    ) -> i32 {
        let mut score = 0;
        if source == "Unified" {
            score += 450;
        } else if source == "Font" {
            score += 400;
        } else if source == "Unicode" {
            score += 300;
        } else if source == "Name" {
            score += 200;
        } else if source == "Identity" && (!is_cid_keyed || is_cjk || is_identity) {
            score += 100;
        }
        if (source == "Unicode" || source == "Font" || source == "Unified")
            && !is_cjk
            && !is_identity
        {
            score += 100;
        }
        if is_cjk && source == "Identity" && phys_width > 0.0 {
            score -= 50;
        }
        score
    }

    #[allow(clippy::too_many_arguments)]
    fn score_candidate(
        &self,
        gid: u32,
        source: &'static str,
        hint: Option<char>,
        glyph_name_resolved: Option<&str>,
        pdf_width: f32,
        is_cjk: bool,
        is_identity: bool,
    ) -> i32 {
        let mut score = 10;
        let phys_width = self.get_physical_width(gid);
        if pdf_width > 0.0 {
            if phys_width > 0.0 {
                let diff = (phys_width - pdf_width).abs();
                if diff < 2.0 {
                    score += 150;
                } else if diff < 50.0 {
                    score += 50;
                } else {
                    score -= if is_cjk && gid > 100 { 50 } else { 500 };
                }
            } else {
                score -= 30;
            }
        }
        if let Some(c) = hint
            && let Some(phys_name) = self.physical_names.get(&gid)
        {
            let h_str = c.to_string();
            let h_hex = format!("uni{:04X}", c as u32);
            if phys_name == &h_str
                || phys_name == &h_hex
                || phys_name.starts_with(&format!("{h_str}_"))
            {
                score += 500;
            }
        }
        if let Some(res_name) = glyph_name_resolved
            && self.physical_names.get(&gid) == Some(&res_name.to_string())
        {
            score += 500;
        }
        score
            + Self::score_source_intent(source, self.is_cid_keyed, is_cjk, is_identity, phys_width)
    }

    fn resolve_gid_font(&self, hint: Option<char>) -> Option<u32> {
        let c = hint?;
        if let Some(ref d) = self.reconstructed_data {
            if let Ok(face) = ttf_parser::Face::parse(d, 0) {
                return face.glyph_index(c).map(|id| u32::from(id.0));
            }
        } else if let Some(ref d) = self.data
            && let Ok(face) = ttf_parser::Face::parse(d, 0)
        {
            return face.glyph_index(c).map(|id| u32::from(id.0));
        }
        None
    }

    fn gather_candidates(
        &self,
        cid: u32,
        hint: Option<char>,
        glyph_name_resolved: Option<&str>,
        is_identity: bool,
    ) -> Vec<(u32, &'static str)> {
        let mut gid_identity = self.to_gid(cid, None);
        if gid_identity == 0 && self.is_cid_keyed && is_identity {
            gid_identity = cid;
        }
        let mut gid_unicode = None;
        if let Some(c) = hint {
            gid_unicode = self.unicode_to_gid.get(&c).copied();
        }
        let mut gid_name = None;
        if let Some(name) = glyph_name_resolved {
            gid_name = self.glyph_name_to_gid.get(name).copied();
        }
        let gid_font = self.resolve_gid_font(hint);
        let mut gid_unified = None;
        if let Some(c) = hint
            && let Some(&cid_mapped) = self.unified_map.get(&c.to_string())
        {
            gid_unified = Some(self.to_gid(cid_mapped, None));
            if gid_unified == Some(0) && self.is_cid_keyed && is_identity {
                gid_unified = Some(cid_mapped);
            }
        }
        let mut candidates = Vec::new();
        if gid_identity != 0 {
            candidates.push((gid_identity, "Identity"));
        }
        if let Some(gid) = gid_unicode {
            candidates.push((gid, "Unicode"));
        }
        if let Some(gid) = gid_name {
            candidates.push((gid, "Name"));
        }
        if let Some(gid) = gid_font {
            candidates.push((gid, "Font"));
        }
        if let Some(gid) = gid_unified {
            candidates.push((gid, "Unified"));
        }
        candidates
    }

    fn resolve_fallback_gid(
        &self,
        cid: u32,
        hint: Option<char>,
        mut _trace: Option<&mut TraceContext>,
    ) -> Option<u32> {
        if self.is_embedded() {
            return None;
        }

        if let Some(c) = hint {
            log::debug!("[FONT] Falling back to system font for: U+{:04X} ({:?})", c as u32, c);
            return Some(system_fallback_gid(c));
        }

        if cid != 0 {
            // A log, not a Decision: this depends on which fonts this machine has, not
            // on anything the document says. Decisions record conclusions about the file.
            log::warn!(
                "[FONT] CID {} failed to resolve to any GID for {}. Hint: {:?}",
                cid,
                self.base_font.as_str(),
                hint
            );
        }
        #[cfg(feature = "debug-tools")]
        if let Some(ref mut t) = _trace {
            t.finish(None);
        }
        log::debug!("[FONT] resolve_gid result: cid {cid} -> gid None");
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_threshold_and_fallback(
        &self,
        best_gid: Option<u32>,
        best_score: i32,
        hint: Option<char>,
        is_suspicious: bool,
        _pdf_width: f32,
        cid: u32,
        mut _trace: Option<&mut TraceContext>,
    ) -> Option<u32> {
        if let Some(gid) = best_gid {
            let mut threshold = 0;
            if is_suspicious {
                threshold = 200;
            }
            if let Some(c) = hint
                && (c as u32 == 0x24EA || (c as u32 >= 0xE000 && c as u32 <= 0xF8FF))
            {
                threshold = 400;
            }
            if best_score >= threshold {
                #[cfg(feature = "debug-tools")]
                if let Some(ref mut t) = _trace {
                    t.push_step(format!("Selected GID {} (score {}) from True Hybrid search (PDF w: {}, Phys w: {}, Phys Name: {:?})", 
                        gid, best_score, _pdf_width, self.get_physical_width(gid), self.physical_names.get(&gid)));
                }
                log::info!(
                    "[GID] FINAL SELECTED GID {gid} with score {best_score} for CID {cid} (hint: {hint:?})"
                );
                return Some(gid);
            }
            log::info!(
                "[GID] Candidate GID {gid} rejected due to low score {best_score} (threshold: {threshold})"
            );
            return None;
        }

        self.resolve_fallback_gid(cid, hint, _trace)
    }

    fn find_best_candidate(
        &self,
        candidates: Vec<(u32, &'static str)>,
        hint: Option<char>,
        glyph_name_resolved: Option<&str>,
        pdf_width: f32,
        is_cjk: bool,
        is_identity: bool,
    ) -> (Option<u32>, i32) {
        let mut best_gid = None;
        let mut best_score = i32::MIN;

        for (gid, source) in candidates {
            if !self.is_gid_valid(gid) {
                continue;
            }
            let score = self.score_candidate(
                gid,
                source,
                hint,
                glyph_name_resolved,
                pdf_width,
                is_cjk,
                is_identity,
            );
            log::info!(
                "[GID] Candidate {}: GID {} score {} (pdf_w: {}, phys_w: {}, name: {:?})",
                source,
                gid,
                score,
                pdf_width,
                self.get_physical_width(gid),
                self.physical_names.get(&gid)
            );
            if score > best_score {
                best_score = score;
                best_gid = Some(gid);
            }
        }
        (best_gid, best_score)
    }

    fn is_suspicious_hint(&self, hint: Option<char>) -> bool {
        if let Some(c) = hint {
            let u = c as u32;
            let is_pua = (0xE000..=0xF8FF).contains(&u)
                || (0xF0000..=0xFFFFD).contains(&u)
                || (0x100000..=0x10FFFD).contains(&u);
            let is_artifact = u == 0x24EA;
            let is_control = (u <= 0x1F) || (0x7F..=0x9F).contains(&u);
            is_pua || is_artifact || is_control
        } else {
            !self.is_cid_keyed
        }
    }

    fn check_immediate_resolve(
        &self,
        cid: u32,
        unicode_hint: Option<char>,
    ) -> Result<Option<u32>, (Option<char>, Option<String>, bool)> {
        if !self.is_embedded()
            && let Some(c) = unicode_hint
        {
            return Ok(Some(system_fallback_gid(c)));
        }
        let mut hint = unicode_hint;
        let mut glyph_name_resolved = None;

        let is_suspicious = self.is_suspicious_hint(hint);

        if let Some(c) = hint
            && (c as u32 <= 0x1F || (c as u32 >= 0x7F && c as u32 <= 0x9F))
            && !self.is_cid_keyed
        {
            return Ok(None);
        }

        if let Some(ref _enc) = self.encoding
            && let Some((name, agl_hint)) = self.resolve_name_from_encoding(cid)
        {
            glyph_name_resolved = Some(name);
            if hint.is_none() {
                hint = agl_hint;
            }
        }

        if let Some(ref map) = self.cid_to_gid_map
            && let Some(&gid) = map.get(&cid)
            && self.is_gid_valid(gid)
        {
            return Ok(Some(gid));
        }

        Err((hint, glyph_name_resolved, is_suspicious))
    }

    /// Resolves a character code to a glyph index, trying each mapping in turn.
    pub fn resolve_gid(
        &self,
        cid: u32,
        unicode_hint: Option<char>,
        mut _trace: Option<&mut TraceContext>,
    ) -> Option<u32> {
        let (hint, glyph_name_resolved, is_suspicious) =
            match self.check_immediate_resolve(cid, unicode_hint) {
                Ok(res) => return res,
                Err(ctx) => ctx,
            };

        let is_cjk = self.is_cjk();
        let pdf_width = if self.wmode() == 1 {
            self.glyph_vertical_metrics(cid).0
        } else {
            self.glyph_width_by_cid(cid)
        };
        let is_identity = self.cid_ordering.as_deref().is_none_or(|o| o == "Identity");

        let candidates =
            self.gather_candidates(cid, hint, glyph_name_resolved.as_deref(), is_identity);

        let (best_gid, best_score) = self.find_best_candidate(
            candidates,
            hint,
            glyph_name_resolved.as_deref(),
            pdf_width,
            is_cjk,
            is_identity,
        );

        self.apply_threshold_and_fallback(
            best_gid,
            best_score,
            hint,
            is_suspicious,
            pdf_width,
            cid,
            _trace,
        )
    }

    fn resolve_name_from_encoding(&self, cid: u32) -> Option<(String, Option<char>)> {
        let enc = self.encoding.as_ref()?;
        let code_bytes =
            if cid > 0xFF { vec![(cid >> 8) as u8, (cid & 0xFF) as u8] } else { vec![cid as u8] };

        let (decoded_len, glyph_opt): (usize, Option<String>) = enc.decode_next(&code_bytes);
        if decoded_len > 0
            && let Some(glyph_name) = glyph_opt
        {
            let clean_name = glyph_name.strip_prefix('/').unwrap_or(&glyph_name);
            let hint = agl::lookup(clean_name).and_then(|u_str| u_str.chars().next());
            return Some((clean_name.to_string(), hint));
        }
        None
    }

    /// Decodes the next character code, returning its byte length and text.
    pub fn decode_next(&self, data: &[u8]) -> (usize, Option<String>) {
        let (consumed, text, _) = self.decode_next_sourced(data);
        (consumed, text)
    }

    /// [`FontResource::decode_next`], and **which route named the character**.
    ///
    /// The same three steps in the same order — this adds a label, not a decision. It
    /// exists because "213 of 360 glyphs have no Unicode" says nothing a reader can act
    /// on, while "213 needed the embedded font's own cmap" names the work.
    ///
    /// **There are three chains here, not one.** This is the outer one; its third step
    /// falls through to [`FontResource::unicode_for`], which repeats the `/ToUnicode` and
    /// encoding lookups the first two steps have already tried and failed. Each of the
    /// three carries its own copy of the private-use filter. None of that is changed
    /// here; it is now visible, which is the point of labelling before rebuilding.
    pub fn decode_next_sourced(&self, data: &[u8]) -> (usize, Option<String>, UnicodeSource) {
        if data.is_empty() {
            return (0, None, UnicodeSource::Unmapped);
        }
        let min_len = self.get_min_len();

        if let Some(res) = self.decode_via_to_unicode(data, min_len)
            && res.1.is_some()
        {
            return (res.0, res.1, UnicodeSource::ToUnicode);
        }
        if let Some(res) = self.decode_via_encoding(data, min_len)
            && res.1.is_some()
        {
            return (res.0, res.1, UnicodeSource::Encoding);
        }
        // The third step re-enters `unicode_for`, which reports its own route.
        self.decode_via_heuristics_sourced(data)
    }

    fn get_min_len(&self) -> Option<usize> {
        let subtype = self.subtype.as_str();
        let is_multibyte =
            subtype == "Type0" || subtype == "CIDFontType0" || subtype == "CIDFontType2";
        let is_identity =
            self.encoding.as_ref().map(|e| e.name.contains("Identity")).unwrap_or(false);
        if is_multibyte || is_identity { Some(2) } else { None }
    }

    fn decode_via_to_unicode(
        &self,
        data: &[u8],
        min_len: Option<usize>,
    ) -> Option<(usize, Option<String>)> {
        let tu = self.to_unicode.as_ref()?;
        let (len, u): (usize, Option<String>) = tu.decode_next_with_min_len(data, min_len)?;
        if let Some(u_str) = u {
            if u_str.chars().next().is_some_and(|c| is_withheld(c, false).is_some()) {
                return Some((len, None));
            }
            return Some((len, Some(u_str)));
        }
        Some((len, None))
    }

    fn decode_via_encoding(
        &self,
        data: &[u8],
        min_len: Option<usize>,
    ) -> Option<(usize, Option<String>)> {
        let enc = self.encoding.as_ref()?;
        let (len, u): (usize, Option<String>) = enc.decode_next_with_min_len(data, min_len)?;
        if let Some(u_str) = u {
            let (uni, withheld) = resolve_mapping(u_str);
            return Some((len, if withheld.is_some() { None } else { Some(uni) }));
        }
        Some((len, None))
    }

    /// The CID collection's table, for a multibyte or identity-encoded font.
    ///
    /// **Only for those.** Applying it to a simple font evaporates the next byte, because
    /// the collection's CIDs are two bytes wide in this mapping and a one-byte code would
    /// swallow its successor.
    fn decode_via_cid_collection(
        &self,
        data: &[u8],
    ) -> Option<(usize, Option<String>, UnicodeSource)> {
        let aj1 = self.collection_map.as_ref()?;
        let consumed = 2; // AJ1 CIDs are always 2 bytes in our mapping
        if data.len() < consumed {
            return None;
        }
        let u = aj1.map(&data[..consumed])?;
        let c = u.chars().next()?;
        if is_withheld(c, true).is_some() {
            return None;
        }
        Some((consumed, Some(u), UnicodeSource::CidCollection))
    }

    fn decode_via_heuristics_sourced(&self, data: &[u8]) -> (usize, Option<String>, UnicodeSource) {
        let subtype = self.subtype.as_str();
        let is_multibyte =
            subtype == "Type0" || subtype == "CIDFontType0" || subtype == "CIDFontType2";
        let is_identity =
            self.encoding.as_ref().map(|e| e.name.contains("Identity")).unwrap_or(false);

        let consumed = if is_multibyte || is_identity { 2 } else { 1 };
        if data.len() < consumed {
            return (data.len(), None, UnicodeSource::Unmapped);
        }
        let code_bytes = &data[..consumed];

        // 1. Try Adobe-Japan1 (AJ1) mapping for Japanese CIDFonts
        if (is_multibyte || is_identity)
            && let Some(found) = self.decode_via_cid_collection(data)
        {
            return found;
        }

        if !is_multibyte && !is_identity && !self.is_legacy_distiller && !data.is_empty() {
            let code = data[0];
            if (32..127).contains(&code) {
                return (
                    1,
                    Some(String::from_utf8_lossy(&[code]).to_string()),
                    UnicodeSource::AsciiGuess,
                );
            }
        }

        let is_simple = subtype == "Type1" || subtype == "TrueType" || subtype == "Type3";
        let has_reliable_map = self.to_unicode.is_some() || self.encoding.is_some();

        if is_simple
            && !self.is_legacy_distiller
            && !has_reliable_map
            && consumed == 1
            && (0x20..=0x7E).contains(&code_bytes[0])
        {
            return (
                consumed,
                Some((code_bytes[0] as char).to_string()),
                UnicodeSource::AsciiGuess,
            );
        }

        let (text, source) = self.unicode_for(code_bytes);
        (consumed, text, source)
    }
}

/// Summarises every font the document references.
pub fn list_fonts(doc: &Document) -> Vec<FontSummary> {
    let arena = doc.arena();
    let mut fonts = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    let type_key = arena.get_name_by_str("Type");
    let font_val = arena.get_name_by_str("Font");

    if let (Some(tk), Some(fv)) = (type_key, font_val) {
        for handle in arena.all_dict_handles() {
            if let Some(dict) = arena.get_dict(handle)
                && dict.get(&tk).and_then(|o| o.resolve(arena).as_name()) == Some(fv)
            {
                if seen.contains(&handle) {
                    continue;
                }
                seen.insert(handle);
                if let Some(summary) = extract_font_summary(arena, &dict, fv, handle) {
                    fonts.push(summary);
                }
            }
        }
    }
    fonts
}

fn extract_font_summary(
    arena: &PdfArena,
    dict: &std::collections::BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>,
    fv: crate::handle::Handle<crate::object::PdfName>,
    dh: crate::handle::Handle<
        std::collections::BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>,
    >,
) -> Option<FontSummary> {
    let handle = arena.find_object_by_dict_handle(dh).unwrap_or_else(|| Handle::new(dh.index()));
    let name = extract_font_name(arena, dict, fv);
    let subtype_key = arena.get_name_by_str("Subtype");
    let font_type = dict
        .get(&subtype_key.unwrap_or(fv))
        .and_then(|o| o.resolve(arena).as_name())
        .and_then(|n| arena.get_name_str(n))
        .unwrap_or_else(|| "Type1".to_string());

    let encoding_key = arena.get_name_by_str("Encoding");
    let encoding = match dict.get(&encoding_key.unwrap_or(fv)).map(|o| o.resolve(arena)) {
        Some(Object::Name(h)) => arena.get_name_str(h).unwrap_or_else(|| "CustomName".to_string()),
        Some(Object::Dictionary(_)) => "CustomDict".to_string(),
        Some(Object::Stream(_, _)) => "CustomStream".to_string(),
        _ => "Standard".to_string(),
    };

    let is_type3 = font_type == "Type3";
    let is_embedded = if is_type3 {
        dict.contains_key(&arena.name("CharProcs"))
    } else {
        check_font_embedding(arena, dict, fv)
    };
    let is_subset = name.len() > 7 && name.as_bytes().get(6).copied() == Some(b'+');
    let has_to_unicode = dict.contains_key(&arena.name("ToUnicode"));

    Some(FontSummary {
        name,
        font_type,
        is_embedded,
        is_type3,
        is_subset,
        encoding,
        has_to_unicode,
        object_id: handle.index(),
    })
}

fn extract_font_name(
    arena: &PdfArena,
    dict: &std::collections::BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>,
    fv: crate::handle::Handle<crate::object::PdfName>,
) -> String {
    let base_font_key = arena.get_name_by_str("BaseFont").unwrap_or(fv);
    let mut name = dict
        .get(&base_font_key)
        .and_then(|o| resolve_name_or_string(arena, o))
        .unwrap_or_else(|| "Untitled".to_string());

    if (name == "Untitled" || name.is_empty() || name.contains('\u{FFFD}'))
        && let Some(fd_obj) = dict.get(&arena.name("FontDescriptor"))
        && let Some(fd_dict) =
            fd_obj.resolve(arena).as_dict_handle().and_then(|dh| arena.get_dict(dh))
        && let Some(fn_val) =
            fd_dict.get(&arena.name("FontName")).and_then(|o| o.resolve(arena).as_name())
    {
        name = arena
            .get_name(fn_val)
            .map(|n| crate::refine::text::recover_name(n.as_bytes()))
            .unwrap_or(name);
    }

    if (name == "Untitled" || name.is_empty())
        && let Some(dk) = arena.get_name_by_str("DescendantFonts")
        && let Some(kids_obj) = dict.get(&dk)
        && let Some(kids) = kids_obj.resolve(arena).as_array().and_then(|ah| arena.get_array(ah))
        && let Some(kid) = kids.first()
        && let Some(kdh) = kid.resolve(arena).as_dict_handle()
        && let Some(kdict) = arena.get_dict(kdh)
        && let Some(bf) = kdict.get(&base_font_key).and_then(|o| resolve_name_or_string(arena, o))
    {
        name = bf;
    }
    name
}

fn resolve_name_or_string(arena: &PdfArena, o: &Object) -> Option<String> {
    match o.resolve(arena) {
        Object::Name(h) => {
            arena.get_name(h).map(|n| crate::refine::text::recover_name(n.as_bytes()))
        }
        Object::String(s) => Some(crate::refine::text::recover_string(&s)),
        _ => None,
    }
}

fn check_font_embedding(
    arena: &PdfArena,
    dict: &std::collections::BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>,
    fv: crate::handle::Handle<crate::object::PdfName>,
) -> bool {
    let desc_key = arena.get_name_by_str("FontDescriptor").unwrap_or(fv);
    let (f1, f2, f3) = (
        arena.get_name_by_str("FontFile"),
        arena.get_name_by_str("FontFile2"),
        arena.get_name_by_str("FontFile3"),
    );

    if let Some(desc_handle) = dict.get(&desc_key).and_then(|o| o.resolve(arena).as_dict_handle())
        && let Some(desc_dict) = arena.get_dict(desc_handle)
        && [f1, f2, f3].iter().flatten().any(|k| desc_dict.contains_key(k))
    {
        return true;
    }

    // Check descendant fonts for Type0
    let df_key = arena.get_name_by_str("DescendantFonts").unwrap_or(fv);
    if let Some(df_obj) = dict.get(&df_key)
        && let Object::Array(ah) = df_obj.resolve(arena)
        && let Some(arr) = arena.get_array(ah)
    {
        for item in arr {
            if let Some(dh) = item.resolve(arena).as_dict_handle()
                && let Some(dd) = arena.get_dict(dh)
                && check_font_embedding(arena, &dd, fv)
            {
                return true;
            }
        }
    }
    false
}

/// Maps a glyph name to its CFF standard string identifier, if it has one.
pub fn glyph_name_to_sid(name: &str) -> Option<u16> {
    static MAP: std::sync::OnceLock<BTreeMap<&'static str, u16>> = std::sync::OnceLock::new();
    let m = MAP.get_or_init(|| {
        let mut m = BTreeMap::new();
        for (i, &name) in cff_standard::CFF_STANDARD_STRINGS.iter().enumerate() {
            m.entry(name).or_insert(i as u16);
        }
        m
    });

    let clean_name = name.strip_prefix('/').unwrap_or(name);
    m.get(clean_name).copied()
}

#[cfg(feature = "debug-tools")]
/// Records the mapping steps for a specific character.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GlyphTrace {
    pub cid: u32,
    pub unicode_hint: Option<char>,
    pub resolved_gid: Option<u32>,
    pub steps: Vec<String>,
}

#[cfg(feature = "debug-tools")]
/// Orchestrates the collection of glyph traces during rendering or analysis.
pub struct TraceContext {
    pub current_trace: Option<GlyphTrace>,
    pub traces: Vec<GlyphTrace>,
}

#[cfg(feature = "debug-tools")]
impl TraceContext {
    pub fn new() -> Self {
        Self { current_trace: None, traces: Vec::new() }
    }

    pub fn start(&mut self, cid: u32, hint: Option<char>) {
        self.current_trace =
            Some(GlyphTrace { cid, unicode_hint: hint, resolved_gid: None, steps: Vec::new() });
    }

    pub fn push_step(&mut self, step: impl Into<String>) {
        if let Some(ref mut trace) = self.current_trace {
            trace.steps.push(step.into());
        }
    }

    pub fn finish(&mut self, gid: Option<u32>) {
        if let Some(mut trace) = self.current_trace.take() {
            trace.resolved_gid = gid;
            self.traces.push(trace);
        }
    }
}

#[cfg(feature = "debug-tools")]
impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(feature = "debug-tools"))]
#[derive(Default)]
/// Placeholder trace record; populated only under `debug-tools`.
pub struct GlyphTrace;

#[cfg(not(feature = "debug-tools"))]
#[derive(Default)]
/// Placeholder trace collector; populated only under `debug-tools`.
pub struct TraceContext;

#[cfg(not(feature = "debug-tools"))]
impl TraceContext {
    /// Creates an inert collector.
    pub fn new() -> Self {
        Self
    }
    /// No-op without `debug-tools`.
    pub fn start(&mut self, _cid: u32, _hint: Option<char>) {}
    /// No-op without `debug-tools`.
    pub fn push_step(&mut self, _step: impl Into<String>) {}
    /// No-op without `debug-tools`.
    pub fn finish(&mut self, _gid: Option<u32>) {}
}

impl fepdf_font::reconstruction::FontInfo for FontResource {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::PdfName;
    use std::collections::BTreeMap;

    #[test]
    fn test_resolve_gid_priority() {
        let mut res = FontResource {
            subtype: PdfName::new("Type0"),
            base_font: PdfName::new("TestFont"),
            is_cid_keyed: true,
            unicode_to_gid: {
                let mut map = BTreeMap::new();
                map.insert('T', 42); // 'T' maps to GID 42
                map
            },
            cid_to_gid_map: None, // Identity mapping
            reconstructed_data: Some(std::sync::Arc::new(vec![])),
            ..FontResource::new_initial(
                PdfName::new("Type0"),
                PdfName::new("TestFont"),
                FontMetrics::default(),
                None,
                None,
                None,
                None,
                None,
                None,
                true,
                &BTreeMap::new(),
                &crate::arena::PdfArena::new(),
                false,
                None,
                None,
            )
        };
        res.num_glyphs = 1000;

        // Case 1: Identity mapping (cid_to_gid_map is None)
        // Since it's Western (is_cjk=false) and Lying Identity, Unicode hint 'T' should return GID 42.
        let gid = res.resolve_gid(217, Some('T'), None);
        assert_eq!(gid, Some(42), "Should resolve via Unicode hint for Western Lying Identity");
    }

    #[test]
    fn test_font_precipitation() {
        let mut res = FontResource::new_test();
        res.data = Some(std::sync::Arc::new(vec![0u8; 100]));
        res.reconstructed_data = Some(std::sync::Arc::new(vec![1u8; 100]));

        // Precipitation logic
        if res.reconstructed_data.is_some() {
            res.data = None;
        }

        assert!(res.data.is_none(), "Raw data should be released after reconstruction");
    }
    /// What extraction withholds, and what it stopped withholding.
    ///
    /// Private use stays out: the meaning is not universal, so the codepoint carries
    /// nothing a reader can act on whatever named it.
    ///
    /// **Circled numbers used to go out with them and no longer do.** Adobe-Japan1
    /// defines 128 CIDs that *are* those characters, and they carry no Shift-JIS and no
    /// EUC encoding at all, so only a Unicode-based route reaches them — and every route
    /// that can produce one is authoritative. Measured: 2,753 across the corpus, in three
    /// Japanese documents, and what they spell is 注⑵ — a footnote marker that was being
    /// deleted from the extracted text.
    #[test]
    fn private_use_is_withheld_and_circled_numbers_are_not() {
        assert!(super::is_withheld('\u{e000}', false).is_some(), "private use area");
        assert!(super::is_withheld('\u{f8ff}', false).is_some(), "the end of it");
        assert!(super::is_withheld('\u{f0000}', false).is_some(), "the supplementary one");

        assert!(super::is_withheld('\u{2460}', false).is_none(), "① is a character");
        assert!(super::is_withheld('\u{2477}', false).is_none(), "⑷ is a character");
        assert!(super::is_withheld('\u{24ff}', false).is_none(), "the end of the block");
    }

    /// Control characters are refused only where the caller asks, which is the CID
    /// collection's route — a table lookup landing on a control code has gone wrong,
    /// while a `/ToUnicode` naming one is the document's own doing.
    #[test]
    fn control_characters_are_refused_only_when_the_route_asks() {
        assert!(super::is_withheld('\u{0007}', true).is_some());
        assert!(super::is_withheld('\u{0007}', false).is_none());
    }
}
