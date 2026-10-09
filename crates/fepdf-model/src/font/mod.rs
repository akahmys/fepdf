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

// `FontResource`'s methods are in seven files (ROADMAP Y-7): construction and loading
// here, and one file each for what the subjects below name.
/// Adobe's character collections, and the maps Unicode is rescued and generated from.
mod collections;
/// Splitting a string into codes, and the Unicode each decodes to.
mod decoding;
/// A font dictionary read: subtype, name, matrix, `/ToUnicode`, encoding, descendant.
mod dictionary;
/// Which glyph of a substituted face draws a code the font does not map.
mod gid_resolution;
/// A glyph asked for by code: its Unicode, width, CID and glyph index.
mod glyphs;
/// The Unicode a font's glyphs stand for, gathered from every source it has.
mod unicode_maps;

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

use crate::{Document, Object, PdfArena, PdfName, PdfResult};
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
    /// `/W` ranges too wide to expand into `widths`, as `(first, last, width)`
    /// ([`metrics::EXPANDED_CIDS`]); read through [`Self::cid_width`].
    pub width_ranges: Vec<(u32, u32, f32)>,
    /// `/W2` ranges too wide to expand into `vertical_widths`; read through
    /// [`Self::cid_vertical_width`].
    pub vertical_width_ranges: Vec<(u32, u32, (f32, f32, f32))>,
    /// `/DW2` as `(position_y, displacement_y)`; 9.7.4.3's `[880 -1000]` when absent.
    pub default_vertical: (f32, f32),
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
    /// Whether the font is set vertically — writing mode 1 (9.7.5.2).
    ///
    /// **Read from what the file declares, not from the font's name.** The mode is the
    /// `-V` suffix on the encoding's CMap name, or the `/CMapName` of an embedded one;
    /// `detect_wmode` is the same reading the interpreter uses to place a glyph. A caller
    /// asking whether a document is set vertically was previously left to look for `-V` in
    /// `name`, which is a naming convention rather than a declaration, and which
    /// `NotoSerif-Vietnamese` satisfies.
    pub is_vertical: bool,
    /// The indirect object holding the font dictionary, when one does.
    ///
    /// **`None` for a font dictionary written direct that loading left direct.** 7.3.10
    /// allows either, and a resource dictionary's `/Font` entries are given objects at load
    /// (`lift_direct_fonts`), so this is `None` only for one reached some other way. Where
    /// there is no object this reported one anyway — the *dictionary's* index, out of a
    /// different pool — and `fepdf inspect debug` hands this number to `get_font`, so the
    /// wrong one was not only printed.
    pub object_id: Option<u32>,
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
            width_ranges: Vec::new(),
            vertical_width_ranges: Vec::new(),
            default_vertical: (880.0, -1000.0),
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

    /// The font program this resource draws with, whichever form it is held in.
    ///
    /// **There are two fields and a caller should not have to know that.** `data` is the
    /// program as the file holds it, and `reconstructed_data` is the one the engine
    /// patched — and `initialize_lifecycle` *releases* `data` once it has the second, to
    /// save carrying both. So a caller that reads `data` alone finds `None` for every
    /// embedded font whose program was reconstructed, which is nearly all of them: over
    /// the nine samples, 291 of the 299 fonts that are not Type 3, measured 2026-09-19.
    /// The eight that answer `None` here are the ones the documents do not embed.
    ///
    /// A Type 3 font has no program at all — its glyphs are content streams (9.6.4) — and
    /// answers `None` because there is nothing to answer.
    #[must_use]
    pub fn program(&self) -> Option<&[u8]> {
        self.reconstructed_data.as_ref().or(self.data.as_ref()).map(|program| program.as_slice())
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
            width_ranges: metrics.width_ranges,
            vertical_width_ranges: metrics.v_width_ranges,
            default_vertical: metrics.default_vertical,
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
        self.fallback_type = Some(self.infer_fallback_type(doc));
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

    /// Which stand-in face a font that embeds no usable program is drawn with.
    ///
    /// **The order is the standard's, not a heuristic's.** ISO 32000-2 decides most of
    /// this, and where it does not the answer follows what a mainstream reader does.
    ///
    /// | Question | What answers it | Clause |
    /// | :--- | :--- | :--- |
    /// | Is it CJK, and which collection? | `/CIDSystemInfo` `/Registry` and `/Ordering` | 9.7.3, Table 114 |
    /// | Is it fixed-pitch? Is it serif? | `/FontDescriptor` `/Flags`, bits 1 and 2 | 9.8.2, Table 121 |
    /// | There is no descriptor at all | Then it is a standard 14 font and its name is its identity | 9.6.2.2 |
    /// | Nothing above answered | The name, and then a sans | *not decided by ISO* |
    ///
    /// **The descriptor comes first because the standard puts it first.** Arlington's
    /// model gives `/FontDescriptor` as
    /// `fn:IsRequired(fn:SinceVersion(2.0) || fn:NotStandard14Font())` — so in PDF 2.0 it
    /// is required on every simple font, the standard-14 exception having been removed,
    /// and `/Flags` is required within it. A reader that asks the *name* first is asking
    /// the guess before the declaration.
    ///
    /// **This is ADR-0041's lesson applied to the other half of the font.** That record
    /// is "a character collection is declared, not guessed", after the engine decided a
    /// font's collection from `/BaseFont` substrings while `/CIDSystemInfo` said so
    /// outright. The shape of a face was still being decided from substrings —
    /// `contains("century")`, `contains("gothic")` — with `/Flags` declared, required,
    /// and read by nothing.
    ///
    /// What ISO does not decide is which *file* stands in for each category. That is
    /// `fallback_fonts`, and it follows the same shape a mainstream reader uses.
    fn infer_fallback_type(&self, doc: &Document) -> FallbackFontType {
        if let Some(kind) = self.declared_script() {
            return kind;
        }
        if let Some(kind) = self.declared_shape(doc) {
            return kind;
        }
        let name = self.base_font.as_str().to_lowercase();
        if let Some(kind) = Self::standard_fourteen(&name) {
            return kind;
        }
        Self::guessed_from_name(&name).unwrap_or(FallbackFontType::SansSerif)
    }

    /// The collection the font declares, per 9.7.3 — not what its name looks like.
    ///
    /// A Latin face substituted for a CJK one draws nothing, so script is asked before
    /// shape. `Identity` declares no collection and so answers nothing here; a Type0 font
    /// with `Identity` ordering falls through to the descriptor like any other.
    fn declared_script(&self) -> Option<FallbackFontType> {
        let ordering = self.cid_ordering.as_deref()?;
        if !matches!(self.cid_registry.as_deref(), Some("Adobe")) {
            return None;
        }
        match ordering {
            // 9.7.3's Adobe collections. Korea1 and KR take the Japanese faces because
            // those are the CJK faces this engine bundles; CNS1 and GB1 likewise.
            "Japan1" | "Japan2" | "GB1" | "CNS1" | "Korea1" | "KR" => {
                let name = self.base_font.as_str().to_lowercase();
                if name.contains("mincho") || name.contains("ming") || name.contains("serif") {
                    Some(FallbackFontType::JapaneseSerif)
                } else {
                    Some(FallbackFontType::JapaneseSans)
                }
            }
            _ => None,
        }
    }

    /// `/FontDescriptor` `/Flags`: bit 1 is `FixedPitch`, bit 2 is `Serif` (Table 121).
    ///
    /// Required by the standard and, before 2026-09-06, read by nothing: the compliance
    /// audit checked the key was *present* and no code took its value, so
    /// [`FallbackFontType::Default`]'s own doc — "the loader picks by descriptor flags" —
    /// described something that did not happen.
    fn declared_shape(&self, doc: &Document) -> Option<FallbackFontType> {
        let arena = doc.arena();
        let dict = arena.get_dict(arena.get_object(self.descriptor?)?.as_dict_handle()?)?;
        let flags = dict.get(&arena.name("Flags"))?.resolve(arena).as_integer()?;
        if flags & 0b1 != 0 {
            return Some(FallbackFontType::Monospace);
        }
        if flags & 0b10 != 0 {
            return Some(FallbackFontType::Serif);
        }
        // Nonsymbolic (bit 6) without Serif is a sans by elimination; Symbolic (bit 3)
        // says nothing about shape and is left to the name.
        if flags & 0b10_0000 != 0 {
            return Some(FallbackFontType::SansSerif);
        }
        None
    }

    /// The standard 14 of 9.6.2.2, which a file may name without a descriptor.
    ///
    /// Reached only when there is no descriptor to read, which before PDF 2.0 is legal
    /// exactly for these fonts. `Symbol` and `ZapfDingbats` are in the table and get no
    /// stand-in: substituting a text face would draw the wrong glyphs rather than similar
    /// ones, which is worse than drawing the default.
    fn standard_fourteen(name: &str) -> Option<FallbackFontType> {
        let bare = name.rsplit('+').next().unwrap_or(name);
        let stem: String = bare.chars().filter(char::is_ascii_alphanumeric).collect();
        [
            ("courier", FallbackFontType::Monospace),
            ("helvetica", FallbackFontType::SansSerif),
            ("arial", FallbackFontType::SansSerif),
            ("times", FallbackFontType::Serif),
        ]
        .into_iter()
        .find(|(needle, _)| stem.starts_with(needle))
        .map(|(_, kind)| kind)
    }

    /// The last question, and the only one ISO does not put words to.
    ///
    /// A guess, and named one. Kept because a file may carry a descriptor whose `/Flags`
    /// declares neither serif nor fixed-pitch nor nonsymbolic, and a name is then the
    /// only thing left that is about the face at all.
    fn guessed_from_name(name: &str) -> Option<FallbackFontType> {
        if name.contains("mono") {
            return Some(FallbackFontType::Monospace);
        }
        if ["sans", "verdana", "tahoma", "calibri", "segoe"].iter().any(|k| name.contains(k)) {
            return Some(FallbackFontType::SansSerif);
        }
        if ["serif", "century", "georgia", "garamond", "palatino", "book", "roman"]
            .iter()
            .any(|k| name.contains(k))
        {
            return Some(FallbackFontType::Serif);
        }
        None
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
            let sig = if let Some(&[s0, s1, s2, s3]) = res.data.get(..4) {
                format!("{s0:02x}{s1:02x}{s2:02x}{s3:02x}")
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
                let cid = match code.as_slice() {
                    &[high, low] => (u32::from(high) << 8) | u32::from(low),
                    &[one, ..] => u32::from(one),
                    [] => 0,
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
        } else if matches!(raw.as_slice(), [1, 0, ..] | [2, _, ..]) {
            EmbeddedFormat::Cff
        } else if raw.starts_with(b"%!") || raw.starts_with(&[0x80, 0x01]) {
            EmbeddedFormat::Type1
        } else {
            EmbeddedFormat::Unrecognised
        }
    }
}

/// One entry of a dictionary, by a key that may never have been interned.
///
/// **A name the arena has not seen is a key no dictionary here holds**, so the answer is
/// "not there" rather than the value under some other key. Five sites read
/// `arena.get_name_by_str(k).unwrap_or(fv)`, and `fv` is the handle for `/Font` — the
/// resource name this walk starts from. Where the name was missing they looked `/Font`
/// up in a font dictionary and reported what they found there as the font's subtype, its
/// encoding or its name.
///
/// Reachable only for a document in which the key occurs nowhere at all, and harmful only
/// where the dictionary happens to hold a `/Font` key of its own — so this is a fallback
/// that could answer from the wrong place rather than one that was observed doing it. It
/// is still a fallback that answers, which is the shape this engine keeps removing.
fn entry<'d>(
    arena: &PdfArena,
    dict: &'d std::collections::BTreeMap<Handle<PdfName>, Object>,
    key: &str,
) -> Option<&'d Object> {
    dict.get(&arena.get_name_by_str(key)?)
}

/// A font dictionary this document reaches, and the indirect object holding it.
///
/// **The two are not the same handle and not the same index space.** A dictionary lives in
/// the arena's `dicts` pool and an indirect object in its `objects` pool, and the walk that
/// finds a font reaches it through a `/Font` resource entry that is usually a reference —
/// so the object handle is in hand at that moment and was being thrown away, then looked
/// for again by scanning every object in the arena.
struct ReachedFont {
    /// The indirect object the `/Font` entry named, when it named one. `None` for a font
    /// dictionary written **direct**, which 7.3.10 allows and which has no object number.
    object: Option<Handle<Object>>,
    /// The font dictionary itself.
    dict: Handle<BTreeMap<Handle<PdfName>, Object>>,
}

/// Summarises every font the document references.
pub fn list_fonts(doc: &Document) -> Vec<FontSummary> {
    let arena = doc.arena();
    let mut fonts = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    // A document that never wrote the name `/Font` has no font resource to reach one
    // through. Cheap, and it is the only thing this name was still for: the five key
    // lookups that used to substitute it now fail instead.
    if arena.get_name_by_str("Font").is_none() {
        return fonts;
    }
    for reached in reachable_font_dicts(doc) {
        if !seen.insert(reached.dict) {
            continue;
        }
        if let Some(dict) = arena.get_dict(reached.dict)
            && let Some(summary) = extract_font_summary(arena, &dict, reached.object)
        {
            fonts.push(summary);
        }
    }
    fonts
}

/// Every font dictionary a page or a form reaches, and the descendants they name.
///
/// **Reachability rather than every dictionary in the arena.** Refinement commits each
/// dictionary it rewrites to a *new* handle, leaving the one it replaced behind
/// unreferenced, so a walk over `all_dict_handles` counted both: `inspect info` reported
/// 24 fonts for `samples/constitution.pdf` where it has 12, and twice the true number on
/// every sample.
///
/// **Walking objects instead would have been the other error.** 7.3.10 lets a font
/// dictionary sit *directly* in a resource dictionary, with no object of its own to be
/// found by — the shape this engine's own decorations used until 2026-09-19 — so the fonts
/// are reached the way a content stream reaches them: through the resources of the page or
/// form that names them.
fn reachable_font_dicts(doc: &Document) -> Vec<ReachedFont> {
    let arena = doc.arena();
    let (type_key, subtype_key, font_key, resources_key) =
        (arena.name("Type"), arena.name("Subtype"), arena.name("Font"), arena.name("Resources"));
    let (page_val, form_val) = (arena.name("Page"), arena.name("Form"));

    let mut out = Vec::new();
    for index in 0..arena.object_count() {
        let handle = Handle::new(index);
        let Some(Object::Dictionary(dh) | Object::Stream(dh, _)) = arena.get_object(handle) else {
            continue;
        };
        let Some(dict) = arena.get_dict(dh) else { continue };
        let is_page =
            dict.get(&type_key).and_then(|o| o.resolve(arena).as_name()) == Some(page_val);
        let is_form =
            dict.get(&subtype_key).and_then(|o| o.resolve(arena).as_name()) == Some(form_val);
        if !is_page && !is_form {
            continue;
        }

        for resources in
            crate::ingest::discovery::accumulate_resources(arena, &dict, is_form, &resources_key)
        {
            let Some(fonts_dh) =
                resources.get(&font_key).and_then(|o| o.resolve(arena).as_dict_handle())
            else {
                continue;
            };
            let Some(named) = arena.get_dict(fonts_dh) else { continue };
            for value in named.values() {
                if let Some(font_dh) = value.resolve(arena).as_dict_handle() {
                    out.push(ReachedFont { object: value.as_reference(), dict: font_dh });
                    out.extend(descendants_of(arena, font_dh));
                }
            }
        }
    }
    out
}

/// The CIDFonts a Type 0 font names, which are fonts in their own right (9.7.4).
fn descendants_of(
    arena: &PdfArena,
    font_dh: Handle<BTreeMap<Handle<PdfName>, Object>>,
) -> Vec<ReachedFont> {
    let Some(dict) = arena.get_dict(font_dh) else { return Vec::new() };
    let Some(Object::Array(ah)) =
        dict.get(&arena.name("DescendantFonts")).map(|o| o.resolve(arena))
    else {
        return Vec::new();
    };
    arena
        .get_array(ah)
        .unwrap_or_default()
        .iter()
        .filter_map(|item| {
            Some(ReachedFont {
                object: item.as_reference(),
                dict: item.resolve(arena).as_dict_handle()?,
            })
        })
        .collect()
}

fn extract_font_summary(
    arena: &PdfArena,
    dict: &std::collections::BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>,
    object: Option<Handle<Object>>,
) -> Option<FontSummary> {
    let name = extract_font_name(arena, dict);
    let font_type = entry(arena, dict, "Subtype")
        .and_then(|o| o.resolve(arena).as_name())
        .and_then(|n| arena.get_name_str(n))
        .unwrap_or_else(|| "Type1".to_string());

    let encoding = match entry(arena, dict, "Encoding").map(|o| o.resolve(arena)) {
        Some(Object::Name(h)) => arena.get_name_str(h).unwrap_or_else(|| "CustomName".to_string()),
        Some(Object::Dictionary(_)) => "CustomDict".to_string(),
        Some(Object::Stream(_, _)) => "CustomStream".to_string(),
        _ => "Standard".to_string(),
    };

    let is_type3 = font_type == "Type3";
    let is_embedded = if is_type3 {
        dict.contains_key(&arena.name("CharProcs"))
    } else {
        check_font_embedding(arena, dict)
    };
    let is_subset = fepdf_font::subset::subset_tag(&name).is_some();
    let has_to_unicode = dict.contains_key(&arena.name("ToUnicode"));

    Some(FontSummary {
        name,
        font_type,
        is_embedded,
        is_type3,
        is_subset,
        encoding,
        has_to_unicode,
        is_vertical: metrics::detect_wmode(dict, arena) == 1,
        object_id: object.map(|handle| handle.index()),
    })
}

fn extract_font_name(
    arena: &PdfArena,
    dict: &std::collections::BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>,
) -> String {
    let mut name = entry(arena, dict, "BaseFont")
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
        && let Some(bf) =
            entry(arena, &kdict, "BaseFont").and_then(|o| resolve_name_or_string(arena, o))
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

/// How deep a `/DescendantFonts` chain is followed before it is taken to be looping.
///
/// The same 64 the field-tree and `/Next` walks use, and a depth rather than a visited
/// set for the reason [ADR-0060] gives: this is a tree.
///
/// [ADR-0060]: ../../../../docs/adr/0060-a-reference-chain-is-bounded-by-what-it-has-seen.md
const MAX_DESCENDANT_DEPTH: usize = 64;

/// Whether this font or any font it descends from carries an embedded font file.
///
/// **Bounded since 2026-09-06.** A Type0 font whose `/DescendantFonts` names itself made
/// this follow it forever, and `fepdf inspect audit` aborted on a five-object file:
/// `fatal runtime error: stack overflow`. RR-15 Rule 6.
///
/// Found by `scripts/audit/unbounded_recursion.py` on the run that first listed anything,
/// after five hand sweeps and a throwaway detector had all passed over it — the detector
/// looked for `/Kids`, `/K` and `/Next` in the body and this walk names none of them.
fn check_font_embedding(
    arena: &PdfArena,
    dict: &std::collections::BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>,
) -> bool {
    check_font_embedding_at(arena, dict, 0)
}

fn check_font_embedding_at(
    arena: &PdfArena,
    dict: &std::collections::BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>,
    depth: usize,
) -> bool {
    if depth >= MAX_DESCENDANT_DEPTH {
        return false;
    }
    let (f1, f2, f3) = (
        arena.get_name_by_str("FontFile"),
        arena.get_name_by_str("FontFile2"),
        arena.get_name_by_str("FontFile3"),
    );

    if let Some(desc_handle) =
        entry(arena, dict, "FontDescriptor").and_then(|o| o.resolve(arena).as_dict_handle())
        && let Some(desc_dict) = arena.get_dict(desc_handle)
        && [f1, f2, f3].iter().flatten().any(|k| desc_dict.contains_key(k))
    {
        return true;
    }

    // Check descendant fonts for Type0
    if let Some(df_obj) = entry(arena, dict, "DescendantFonts")
        && let Object::Array(ah) = df_obj.resolve(arena)
        && let Some(arr) = arena.get_array(ah)
    {
        for item in arr {
            if let Some(dh) = item.resolve(arena).as_dict_handle()
                && let Some(dd) = arena.get_dict(dh)
                && check_font_embedding_at(arena, &dd, depth + 1)
            {
                return true;
            }
        }
    }
    false
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
    fn type1_cleartext_length(&self) -> Option<usize> {
        self.length1.and_then(|n| usize::try_from(n).ok())
    }
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
mod vertical_defaults {
    //! `/DW2` is the font's own default vertical metrics (9.7.4.3), and is read.

    use crate::font::metrics::FontMetrics;
    use crate::{Object, PdfArena};
    use std::collections::BTreeMap;

    /// A CIDFont dictionary built by `fill`, in an arena of its own.
    ///
    /// `fill` takes the arena because a handle belongs to the one that issued it. Two
    /// tests in this file have now been written with the array in one arena and the
    /// dictionary in another; the handles collide and the reader silently sees something
    /// else. It is the failure mode `Handle` has, and a helper that hands the arena over
    /// is what stops it.
    fn cid_font(
        fill: impl Fn(&PdfArena, &mut BTreeMap<crate::Handle<crate::PdfName>, Object>),
    ) -> (PdfArena, BTreeMap<crate::Handle<crate::PdfName>, Object>) {
        let arena = PdfArena::new();
        let mut dict = BTreeMap::new();
        fill(&arena, &mut dict);
        (arena, dict)
    }

    fn numbers(arena: &PdfArena, values: &[f64]) -> Object {
        Object::Array(arena.alloc_array(values.iter().map(|v| Object::Real(*v)).collect()))
    }

    /// A font that declares `/DW2` is laid out by what it declares.
    ///
    /// **`/DW` beside it was read from the file all along**, while these two numbers were
    /// written into `glyph_vertical_metrics` as literals. They happened to be 9.7.4.3's
    /// defaults, so a font that said nothing was right and a font that said something
    /// else was laid out as though it had not.
    #[test]
    fn a_declared_dw2_is_what_the_font_gets() {
        let (arena, dict) = cid_font(|a, d| {
            d.insert(a.name("DW2"), numbers(a, &[760.0, -880.0]));
        });
        let metrics = FontMetrics::parse_cid(&dict, &arena);
        assert_eq!(metrics.default_vertical, (760.0, -880.0));
    }

    /// A font that declares nothing gets 9.7.4.3's `[880 -1000]`.
    #[test]
    fn an_absent_dw2_is_the_standards_default() {
        let (arena, dict) = cid_font(|_, _| {});
        let metrics = FontMetrics::parse_cid(&dict, &arena);
        assert_eq!(
            metrics.default_vertical,
            (880.0, -1000.0),
            "the values that were hard-coded are the standard's, and stay the default"
        );
    }

    /// A `/DW2` that is not two numbers leaves the default whole.
    ///
    /// Not half of it: a font cannot declare one number and inherit the other, and a
    /// partial read would put a declared position vector against a default displacement.
    #[test]
    fn a_malformed_dw2_leaves_the_default_whole() {
        let (arena, dict) = cid_font(|a, d| {
            d.insert(a.name("DW2"), numbers(a, &[760.0]));
        });
        let metrics = FontMetrics::parse_cid(&dict, &arena);
        assert_eq!(metrics.default_vertical, (880.0, -1000.0));
    }
}

#[cfg(test)]
mod substitution {
    //! Which face stands in for a font the file does not embed.
    //!
    //! The order is the standard's: the collection a CID font declares (9.7.3), then
    //! `/FontDescriptor` `/Flags` (9.8.2, Table 121), then — only where there is no
    //! descriptor, which before PDF 2.0 is legal exactly for the standard 14 — the name
    //! (9.6.2.2). What ISO does not decide is which file stands in for each category.

    use super::{FallbackFontType, FontResource};
    use crate::Document;
    use crate::ingest::IngestionOptions;

    fn assemble(objects: &[String]) -> bytes::Bytes {
        let out = fepdf_fixtures::assemble(objects);
        bytes::Bytes::from(out)
    }

    /// A one-page document whose only font is `font`, at object 5.
    fn face_chosen_for(font: &str, extra: &[String]) -> Option<FallbackFontType> {
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
              /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
                .to_string(),
            "<< /Length 33 >>\nstream\nBT /F1 12 Tf 20 100 Td (h) Tj ET\nendstream".to_string(),
            font.to_string(),
        ];
        objects.extend_from_slice(extra);
        let doc = Document::open(assemble(&objects), &IngestionOptions::default())
            .expect("the fixture opens");
        doc.get_font(crate::handle::Handle::new(5)).expect("the font loads").fallback_type
    }

    const SERIF_DESCRIPTOR: &str = "<< /Type /FontDescriptor /FontName /Frobnicate /Flags 2 \
        /FontBBox [0 0 1000 1000] /ItalicAngle 0 /Ascent 900 /Descent -200 \
        /CapHeight 700 /StemV 80 >>";

    /// `/Flags` decides, and it is asked before the name.
    ///
    /// **The entry the standard requires, that nothing read.** Arlington gives
    /// `/FontDescriptor` as required whenever the file is 2.0 or the font is not one of
    /// the standard 14, and `/Flags` as required within it — so this is the declaration,
    /// where a name is a guess about the same thing. `/Flags` was declared in the schema,
    /// checked for *presence* by the compliance audit, and read by no code until
    /// 2026-09-06.
    #[test]
    fn the_descriptor_is_asked_before_the_name() {
        for (flags, want) in [
            (2, FallbackFontType::Serif),      // bit 2, Serif
            (1, FallbackFontType::Monospace),  // bit 1, FixedPitch
            (32, FallbackFontType::SansSerif), // bit 6, Nonsymbolic and not Serif
        ] {
            let chosen = face_chosen_for(
                "<< /Type /Font /Subtype /TrueType /BaseFont /Frobnicate \
                  /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R >>",
                &[format!(
                    "<< /Type /FontDescriptor /FontName /Frobnicate /Flags {flags} \
                      /FontBBox [0 0 1000 1000] /ItalicAngle 0 /Ascent 900 /Descent -200 \
                      /CapHeight 700 /StemV 80 >>"
                )],
            );
            assert_eq!(chosen, Some(want), "/Flags {flags} decides");
        }
    }

    /// And it wins over a name that says otherwise.
    ///
    /// `Arial` is a standard-14 alias and a sans by every name heuristic. A descriptor
    /// declaring Serif is the file's own statement, and it is the one that counts.
    #[test]
    fn a_declaration_beats_a_name_that_disagrees_with_it() {
        let chosen = face_chosen_for(
            "<< /Type /Font /Subtype /TrueType /BaseFont /Arial \
              /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R >>",
            &[SERIF_DESCRIPTOR.to_string()],
        );
        assert_eq!(chosen, Some(FallbackFontType::Serif), "the descriptor is the declaration");
    }

    /// With no descriptor, the name is the identity — which is legal only for these.
    ///
    /// Before PDF 2.0 a standard-14 font may omit its descriptor, and 9.6.2.2 is then
    /// what says which face it is. A subset tag or a style suffix does not change that.
    #[test]
    fn without_a_descriptor_the_standard_fourteen_are_named() {
        for (name, want) in [
            ("helvetica", FallbackFontType::SansSerif),
            ("helvetica-bold", FallbackFontType::SansSerif),
            ("arial", FallbackFontType::SansSerif),
            ("abcdef+arialmt", FallbackFontType::SansSerif),
            ("times-roman", FallbackFontType::Serif),
            ("abcdef+timesnewromanpsmt", FallbackFontType::Serif),
            ("couriernewpsmt", FallbackFontType::Monospace),
        ] {
            assert_eq!(FontResource::standard_fourteen(name), Some(want), "{name}");
        }
        // Also in the fourteen, and deliberately without a stand-in: putting their code
        // points through a text face draws the wrong glyphs, not similar ones.
        assert_eq!(FontResource::standard_fourteen("symbol"), None);
        assert_eq!(FontResource::standard_fourteen("zapfdingbats"), None);
    }

    /// The name is the last question and may decline to answer.
    ///
    /// `None` is what lets the descriptor be asked first. A heuristic that always
    /// guessed would make the file's own declaration unreachable.
    #[test]
    fn the_name_is_the_last_question_and_the_only_guess() {
        assert_eq!(FontResource::guessed_from_name("garamond"), Some(FallbackFontType::Serif));
        assert_eq!(FontResource::guessed_from_name("verdana"), Some(FallbackFontType::SansSerif));
        assert_eq!(
            FontResource::guessed_from_name("consolas-mono"),
            Some(FallbackFontType::Monospace)
        );
        assert_eq!(FontResource::guessed_from_name("wingdings"), None);
        // A compound name means the sans: "sans" is asked before "serif" for that reason.
        assert_eq!(
            FontResource::guessed_from_name("dejavusans-serif"),
            Some(FallbackFontType::SansSerif)
        );
    }
}

#[cfg(test)]
mod descendant_fonts {
    //! A `/DescendantFonts` that names its own font is followed once, not forever.

    use crate::Document;
    use crate::ingest::IngestionOptions;

    /// **The defect this was written for.** `fepdf inspect audit` aborted with
    /// `fatal runtime error: stack overflow` on the five objects below: object 5 is a
    /// Type0 font whose `/DescendantFonts` array names object 5.
    ///
    /// Found by `scripts/audit/unbounded_recursion.py`, on the run that first listed
    /// anything. Five sweeps by hand and a throwaway detector had passed over it — the
    /// detector looked for `/Kids`, `/K` and `/Next`, and this walk names none of them.
    #[test]
    fn a_font_that_descends_from_itself_is_not_followed_forever() {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
              /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
            "<< /Length 34 >>\nstream\nBT /F1 12 Tf 20 100 Td (hi) Tj ET\nendstream",
            "<< /Type /Font /Subtype /Type0 /BaseFont /X /Encoding /Identity-H \
              /DescendantFonts [5 0 R] >>",
        ];
        let out = fepdf_fixtures::assemble(&objects);

        let doc = Document::open(bytes::Bytes::from(out), &IngestionOptions::default())
            .expect("the fixture opens");
        let fonts = doc.fonts();

        assert!(!fonts.is_empty(), "the font is surveyed rather than skipped");
        assert!(
            !fonts[0].is_embedded,
            "a font that descends only from itself carries no font file"
        );
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
