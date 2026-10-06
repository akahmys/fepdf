// `FontReconstructor`'s methods are in three files (ROADMAP Y-7): the entry and SFNT
// patching here, Type 1 transcoding in `type1`, and the tables built in `sfnt_tables`.
/// The SFNT tables a reconstructed program needs, and the container around them.
mod sfnt_tables;
/// A Type 1 program made a CFF one.
mod type1;

use crate::cmap::CMap;
use crate::{FontError, FontResult};
use std::collections::BTreeMap;

/// Interface exposing font metadata needed for binary reconstruction.
pub trait FontInfo {
    /// PostScript base font name.
    fn base_font(&self) -> &str;
    /// Subtype of the font.
    fn subtype(&self) -> &str;
    /// Whether the font is CID-keyed.
    fn is_cid_keyed(&self) -> bool;
    /// Whether the font is a CJK font.
    fn is_cjk(&self) -> bool;
    /// CID ordering string.
    fn cid_ordering(&self) -> Option<&str>;
    /// Map of CID to GID.
    fn cid_to_gid_map(&self) -> Option<&BTreeMap<u32, u32>>;
    /// Unified mapping for CMap synthesis.
    fn unified_map(&self) -> &BTreeMap<String, u32>;
    /// Encoding CMap if present.
    fn encoding(&self) -> Option<&CMap>;
    /// Physical or PDF glyph width by GID.
    fn glyph_width_by_gid(&self, gid: u32) -> f32;
    /// Resolves GID for a CID or hint name.
    fn to_gid_hint(&self, cid: u32, hint_name: Option<&str>) -> u32;
}

/// A surgical patcher for SFNT binaries.
pub struct FontReconstructor;

pub(crate) struct DisassembledSfnt {
    pub(crate) magic: [u8; 4],
    pub(crate) tables: Vec<([u8; 4], Vec<u8>)>,
}

/// The result of a font reconstruction operation.
#[derive(Debug, Clone)]
pub struct ReconstructedFont {
    /// The patched SFNT binary data.
    pub data: Vec<u8>,
    /// Whether the font is CID-keyed.
    pub is_cid: bool,
    /// Discovered CID-to-GID mapping (Authoritative).
    pub cid_to_gid_map: Option<BTreeMap<u32, u32>>,
    /// Discovered Glyph Name to GID mapping.
    pub name_to_gid_map: Option<BTreeMap<String, u32>>,
    /// Discovered CFF SID to GID mapping.
    pub sid_to_gid_map: Option<BTreeMap<u32, u32>>,
    /// Discovered or synthesized glyph count.
    pub num_glyphs: Option<u32>,
}

struct Type1Data {
    charstrings: BTreeMap<String, Vec<u8>>,
    subrs: Vec<Vec<u8>>,
    len_iv: usize,
}

struct Type1Segments {
    pub ascii: Vec<u8>,
    pub binary: Vec<u8>,
    pub trailer: Vec<u8>,
}

/// Standard CFF SID for the last predefined standard string.
const CFF_LAST_STANDARD_SID: u32 = 390;

/// Font format identified from binary data signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFormat {
    /// SFNT container (OpenType/TrueType).
    Sfnt,
    /// Naked CFF version 1.0.
    Cff1,
    /// Naked CFF version 2.0.
    Cff2,
    /// Adobe Type 1 Binary (PFB).
    Type1Pfb,
    /// Adobe Type 1 ASCII (PFA).
    Type1Pfa,
    /// Unrecognized format.
    Unknown,
}

impl FontFormat {
    /// Detects the font format from raw binary data, optionally using metadata hints.
    pub fn detect_with_resource(data: &[u8], resource: &impl FontInfo) -> Self {
        let format = Self::detect(data);

        // Hardening (RR-15): If metadata says CIDFontType0 (CFF) but we detected Type1Pfb,
        // it might be a misidentified CFF font or a CID-keyed Type 1 font.
        // We check if it's actually CFF but just has a weird start (coincidental 0x80 0x01).
        if format == FontFormat::Type1Pfb && resource.subtype() == "CIDFontType0" {
            // CFF should start with version 1.x or 2.x.
            // If data[0] is 1 or 2, it's likely CFF even if the first 2 bytes match PFB.
            if data.len() >= 4 && (data[0] == 1 || data[0] == 2) {
                return if data[0] == 1 { FontFormat::Cff1 } else { FontFormat::Cff2 };
            }
        }

        format
    }

    /// Detects the font format from raw binary data.
    pub fn detect(data: &[u8]) -> Self {
        if data.len() < 2 {
            return FontFormat::Unknown;
        }

        // 1. SFNT Signatures (OTTO, 0x00 0x01 0x00 0x00, ttcf, true)
        if data.len() >= 4
            && (data.starts_with(b"OTTO")
                || data.starts_with(&[0, 1, 0, 0])
                || data.starts_with(b"ttcf")
                || data.starts_with(b"true"))
        {
            return FontFormat::Sfnt;
        }

        // 2. CFF Signatures
        // CFF1: major=1, minor=0 (Standard)
        if data.len() >= 2 && data[0] == 1 && data[1] == 0 {
            return FontFormat::Cff1;
        }
        // CFF2: major=2
        if !data.is_empty() && data[0] == 2 {
            return FontFormat::Cff2;
        }

        // 3. Type 1 Signatures
        // PFB: 0x80 0x01 (Segment start)
        if data.starts_with(&[0x80, 0x01]) {
            return FontFormat::Type1Pfb;
        }
        // PFA: %! (PostScript)
        if data.starts_with(b"%!") {
            return FontFormat::Type1Pfa;
        }

        FontFormat::Unknown
    }
}

impl FontReconstructor {
    /// Reconstructs a font by injecting PDF metrics into the provided SFNT data.
    ///
    /// This method performs surgical patching of font tables (hmtx, cmap) to align
    /// the physical font file with the metrics declared in the PDF document.
    pub fn reconstruct(resource: &impl FontInfo, raw_data: &[u8]) -> FontResult<ReconstructedFont> {
        let format = FontFormat::detect_with_resource(raw_data, resource);
        let sig = if raw_data.len() >= 4 {
            format!("{:02x}{:02x}{:02x}{:02x}", raw_data[0], raw_data[1], raw_data[2], raw_data[3])
        } else {
            "short".to_string()
        };
        log::debug!(
            "[RECONSTRUCT] Starting reconstruction for {} (format: {:?}, size: {} bytes, sig: {})",
            resource.base_font(),
            format,
            raw_data.len(),
            sig
        );

        let normalized = Self::normalize_to_sfnt(format, raw_data, resource)?;
        Ok(Self::patch_sfnt_data(resource, raw_data, normalized))
    }

    fn get_native_metrics(tables: &[([u8; 4], Vec<u8>)]) -> (u16, Option<u16>) {
        let upem =
            tables
                .iter()
                .find(|(t, _)| t == b"head")
                .and_then(|(_, d)| {
                    if d.len() >= 20 { Some(u16::from_be_bytes([d[18], d[19]])) } else { None }
                })
                .unwrap_or(1000);
        let num_glyphs = tables.iter().find(|(t, _)| t == b"maxp").and_then(|(_, d)| {
            if d.len() >= 6 { Some(u16::from_be_bytes([d[4], d[5]])) } else { None }
        });
        (upem, num_glyphs)
    }

    fn patch_sfnt_data(
        // RR-15 Limit: Dispatcher - surgical patching of SFNT binary structures
        resource: &impl FontInfo,
        raw_data: &[u8],
        normalized: ReconstructedFont,
    ) -> ReconstructedFont {
        let mut sfnt = normalized.data;
        let is_cid_font = normalized.is_cid;
        let discovered_map_bt = normalized.sid_to_gid_map.clone();
        let discovered_sid_map = normalized.sid_to_gid_map;
        let discovered_name_map = normalized.name_to_gid_map;

        if let Ok(mut sfnt_dis) = Self::disassemble_sfnt(&sfnt) {
            let (upem, native_num_glyphs) = Self::get_native_metrics(&sfnt_dis.tables);
            if let Some(n) = native_num_glyphs {
                log::debug!("[RECONSTRUCT] Discovered native num_glyphs: {n}");
            }
            Self::patch_hmtx_direct(&mut sfnt_dis.tables, resource, upem);

            let (cmap_data_opt, synthesized_cid_map) = Self::synthesize_bridged_cmap(
                resource,
                raw_data,
                discovered_map_bt.as_ref(),
                discovered_name_map.as_ref(),
                is_cid_font,
            );
            if let Some(cmap_data) = cmap_data_opt {
                if let Some(idx) = sfnt_dis.tables.iter().position(|(t, _)| t == b"cmap") {
                    sfnt_dis.tables[idx].1 = cmap_data;
                } else {
                    sfnt_dis.tables.push((*b"cmap", cmap_data));
                }
                log::debug!("[RECONSTRUCT] Synthesized SFNT with {} tables", sfnt_dis.tables.len());
                if let Ok(new_data) = Self::assemble_sfnt(&sfnt_dis.magic, &sfnt_dis.tables) {
                    sfnt = new_data;
                }
            }

            let final_cid_map = if !synthesized_cid_map.is_empty() {
                Some(synthesized_cid_map)
            } else {
                normalized.cid_to_gid_map
            };
            return ReconstructedFont {
                data: sfnt,
                is_cid: is_cid_font,
                cid_to_gid_map: final_cid_map,
                name_to_gid_map: discovered_name_map,
                sid_to_gid_map: discovered_sid_map,
                num_glyphs: native_num_glyphs.map(u32::from),
            };
        }

        ReconstructedFont {
            data: sfnt,
            is_cid: is_cid_font,
            cid_to_gid_map: discovered_map_bt,
            name_to_gid_map: discovered_name_map,
            sid_to_gid_map: discovered_sid_map,
            num_glyphs: None,
        }
    }

    /// Attempts to rescue the CID-to-GID mapping by scanning internal SFNT cmap tables
    /// and bridging them via the PDF's ToUnicode map if available.
    /// Also attempts to rescue Glyph Name to GID mappings from the 'post' table.
    fn rescue_sid_map_from_sfnt(
        data: &[u8],
        resource: &impl FontInfo,
        name_to_gid: &mut BTreeMap<String, u32>,
    ) -> Option<BTreeMap<u32, u32>> {
        let mut map = BTreeMap::new();
        if let Ok(face) = ttf_parser::Face::parse(data, 0) {
            // 1. Build an internal Unicode -> GID map from the font's own cmap tables
            let mut internal_u2g = BTreeMap::new();

            // 1.1 Extract Glyph Names if available (Essential for subsetted fonts)
            for gid in 0..face.number_of_glyphs() {
                if let Some(name) = face.glyph_name(ttf_parser::GlyphId(gid)) {
                    name_to_gid.insert(name.to_string(), u32::from(gid));
                }
            }

            for table in face.tables().cmap.iter().flat_map(|t| t.subtables) {
                if table.is_unicode() {
                    table.codepoints(|cp| {
                        if let Some(gid) = table.glyph_index(cp) {
                            internal_u2g.insert(cp, u32::from(gid.0));
                        }
                    });
                } else {
                    table.codepoints(|cp| {
                        if let Some(gid) = table.glyph_index(cp) {
                            map.insert(cp, u32::from(gid.0));
                        }
                    });
                }
            }

            // 2. Bridge via unified_map (ToUnicode): maps logical Unicode back to character codes.
            // This allows us to find the correct GID for a character code even if the font's
            // internal cmap is strictly Unicode or has lying Identity entries.
            // Priority: PDF metrics (ToUnicode) ALWAYS override font internal cmaps.
            for (uni_str, &code) in resource.unified_map() {
                if let Some(c) = uni_str.chars().next()
                    && let Some(&gid) = internal_u2g.get(&(c as u32))
                {
                    map.insert(code, gid);
                }
            }
        }
        if map.is_empty() { None } else { Some(map) }
    }

    fn normalize_sfnt_format(
        data: &[u8],
        resource: &impl FontInfo,
    ) -> FontResult<ReconstructedFont> {
        let mut info = Self::inspect_cff(data).unwrap_or(CffInfo::empty());

        if info.sid_to_gid.is_none() {
            info.sid_to_gid = Self::rescue_sid_map_from_sfnt(
                data,
                resource,
                info.name_to_gid.get_or_insert_with(BTreeMap::new),
            );
        }

        let (synthesized_cmap, synthesized_cid_map) = Self::synthesize_bridged_cmap(
            resource,
            data,
            info.sid_to_gid.as_ref(),
            info.name_to_gid.as_ref(),
            info.is_cid,
        );

        let mut final_data = data.to_vec();

        if let Some(new_cmap_data) = synthesized_cmap
            && let Ok(mut sfnt_dis) = Self::disassemble_sfnt(data)
        {
            log::debug!("[RECONSTRUCT] Patching SFNT cmap table for {}", resource.base_font());
            if let Some(idx) = sfnt_dis.tables.iter().position(|(t, _)| t == b"cmap") {
                sfnt_dis.tables[idx].1 = new_cmap_data;
            } else {
                sfnt_dis.tables.push((*b"cmap", new_cmap_data));
            }

            if let Ok(patched_data) = Self::assemble_sfnt(&sfnt_dis.magic, &sfnt_dis.tables) {
                final_data = patched_data;
            }
        }

        let cid_to_gid_map = if !synthesized_cid_map.is_empty() {
            Some(synthesized_cid_map)
        } else {
            Self::build_cid_to_gid_map(&info)
        };

        Ok(ReconstructedFont {
            data: final_data,
            is_cid: info.is_cid,
            cid_to_gid_map,
            name_to_gid_map: info.name_to_gid,
            sid_to_gid_map: info.sid_to_gid,
            num_glyphs: Some(info.num_glyphs as u32),
        })
    }

    fn normalize_to_sfnt(
        format: FontFormat,
        data: &[u8],
        resource: &impl FontInfo,
    ) -> FontResult<ReconstructedFont> {
        match format {
            FontFormat::Sfnt => Self::normalize_sfnt_format(data, resource),
            FontFormat::Cff1 | FontFormat::Cff2 => {
                let tag = if format == FontFormat::Cff1 { *b"CFF " } else { *b"CFF2" };
                Self::wrap_naked_outline(tag, data, resource)
            }
            FontFormat::Type1Pfb | FontFormat::Type1Pfa => {
                Self::transcode_type1_to_cff(data, resource)
            }
            FontFormat::Unknown => {
                // Deliberately silent, because the document-level fact is already
                // recorded and recorded better: `fepdf-model/src/font/mod.rs` raises a
                // 9.9 `Violation` — "embeds a program in no recognised format … skipped
                // it and fell back to a system font" — gated on the font actually
                // embedding something.
                //
                // Measured against the whole external corpus: this warned on exactly the
                // three `isartor-6-3-2-t01-fail-*` files and no others, which are exactly
                // the files carrying that `Violation`. It fired twice where the decision
                // fires once, so it was the same finding, duplicated and less precise.
                Ok(ReconstructedFont {
                    data: data.to_vec(),
                    is_cid: false,
                    cid_to_gid_map: None,
                    name_to_gid_map: None,
                    sid_to_gid_map: None,
                    num_glyphs: None,
                })
            }
        }
    }
}

/// What an inspection pass learned about a CFF font program.
pub struct CffInfo {
    /// Number of glyphs in the CharStrings index.
    pub num_glyphs: usize,
    /// Maps string identifiers to glyph indices, for non-CID fonts.
    pub sid_to_gid: Option<BTreeMap<u32, u32>>,
    /// Maps glyph names to glyph indices, for non-CID fonts.
    pub name_to_gid: Option<BTreeMap<String, u32>>,
    /// Whether the font is CID-keyed.
    pub is_cid: bool,
    /// The font's string index, used to resolve custom names.
    pub string_index: Vec<String>,
}

impl CffInfo {
    /// An inspection result describing no usable font.
    pub fn empty() -> Self {
        Self {
            num_glyphs: 1,
            sid_to_gid: None,
            name_to_gid: None,
            is_cid: false,
            string_index: Vec::new(),
        }
    }
}

impl FontReconstructor {
    fn parse_predefined_charset(o: usize, ng: u32) -> BTreeMap<u32, u32> {
        let mut sid_map = BTreeMap::new();
        if o == 0 {
            for gid in 1..std::cmp::min(ng, 229) {
                sid_map.insert(gid, gid);
            }
        } else if o == 1 {
            for gid in 1..std::cmp::min(ng, 166) {
                sid_map.insert(gid, gid);
            }
        } else if o == 2 {
            for gid in 1..std::cmp::min(ng, 87) {
                sid_map.insert(gid, gid);
            }
        }
        sid_map
    }

    fn parse_charset_map(cff_data: &[u8], o: usize, ng: u32) -> BTreeMap<u32, u32> {
        if o < cff_data.len() && o > 2 {
            Self::parse_cff_charset(cff_data, o, ng as u16).unwrap_or_default()
        } else {
            Self::parse_predefined_charset(o, ng)
        }
    }

    /// Reads a CFF program's indices without fully decoding its charstrings.
    pub fn inspect_cff(data: &[u8]) -> FontResult<CffInfo> {
        // RR-15 Limit: Dispatcher - parses and inspects raw CFF index tables and structures
        let cff_data = Self::extract_cff_stream(data)?;
        if cff_data.len() < 10 {
            return Ok(CffInfo::empty());
        }

        let mut pos = cff_data[2] as usize;
        pos = skip_index(cff_data, pos);
        let top_dict_pos = pos;
        let tc = if pos + 2 <= cff_data.len() {
            u16::from_be_bytes([cff_data[pos], cff_data[pos + 1]])
        } else {
            0
        };
        pos = skip_index(cff_data, pos);
        let string_idx_pos = pos;
        let string_index = Self::parse_string_index(cff_data, string_idx_pos);

        let gsubr_pos = skip_index(cff_data, string_idx_pos);
        let gsubr_count = if gsubr_pos + 2 <= cff_data.len() {
            u16::from_be_bytes([cff_data[gsubr_pos], cff_data[gsubr_pos + 1]])
        } else {
            0
        };
        log::debug!("[RECONSTRUCT] Global Subrs INDEX at {gsubr_pos}, count: {gsubr_count}");

        let (cso, cso2, is_cid) = Self::parse_cff_top_dict(cff_data, top_dict_pos, tc);
        let ng = Self::determine_glyph_count(cff_data, cso);
        if let Some(o) = cso {
            log::debug!("[RECONSTRUCT] CharStrings INDEX at {o}, count: {ng}");
        }
        log::debug!("[RECONSTRUCT] CFF glyphs: {ng}, Top DICT: {tc}, is_cid: {is_cid}");

        let mut sid_map = BTreeMap::new();
        if let Some(o) = cso2 {
            log::debug!(
                "[RECONSTRUCT] Charset: {}, format: {}",
                o,
                cff_data.get(o).unwrap_or(&255)
            );
            sid_map = Self::parse_charset_map(cff_data, o, ng.into());
        } else {
            Self::apply_default_charset(&mut sid_map, is_cid, ng);
        }

        let name_to_gid = if !sid_map.is_empty() {
            Some(Self::derive_name_map(cff_data, &sid_map, string_idx_pos))
        } else {
            None
        };
        Ok(CffInfo {
            num_glyphs: ng as usize,
            sid_to_gid: Some(sid_map),
            name_to_gid,
            is_cid,
            string_index,
        })
    }

    fn build_cid_to_gid_map(info: &CffInfo) -> Option<BTreeMap<u32, u32>> {
        if info.is_cid { info.sid_to_gid.clone() } else { None }
    }

    fn extract_cff_stream(data: &[u8]) -> FontResult<&[u8]> {
        // **A collection is an SFNT too**, and this did not say so: `detect` at the top of
        // this file lists `ttcf` beside `OTTO` and the version tag, and this second test
        // of the same thing left it out. So every CFF-based face installed on macOS —
        // which is all of the Japanese ones — fell through to being read as a bare CFF,
        // and answered with the count a failed parse leaves behind.
        let base = sfnt_base(data);
        let is_sfnt = data.len() >= base + 4
            && (data[base..].starts_with(b"OTTO") || data[base..].starts_with(&[0, 1, 0, 0]));
        if is_sfnt {
            if let Some((o, e)) = find_table_range(data, b"CFF ") {
                log::debug!("[RECONSTRUCT] Found CFF table at {}-{} (size: {})", o, e, e - o);
                Ok(&data[o..e])
            } else if let Some((o, e)) = find_table_range(data, b"CFF2") {
                log::debug!("[RECONSTRUCT] Found CFF2 table at {}-{} (size: {})", o, e, e - o);
                Ok(&data[o..e])
            } else {
                // Deliberately silent. This warned, and it fired **918 times across six
                // of the nine conforming samples** — 342 on `intel_sdm.pdf` alone —
                // because an SFNT container with no `CFF ` table is an ordinary
                // *TrueType* font with `glyf` outlines. Every caller of `inspect_cff`
                // reaches it through `.unwrap_or(CffInfo::empty())`, which is to say it
                // is asked speculatively and this `Err` is the expected answer.
                //
                // Converting it to a `Decision` would have put 918 false departures on
                // clean files and made `is_conforming` false for all six — ADR-0008
                // exactly. The error still carries the reason to whoever wants it.
                Err(FontError::Other("CFF table not found in SFNT container".into()))
            }
        } else {
            Ok(data)
        }
    }

    fn parse_string_index(data: &[u8], pos: usize) -> Vec<String> {
        let mut string_index = Vec::new();
        let str_count = if pos + 2 <= data.len() {
            u16::from_be_bytes([data[pos], data[pos + 1]]) as usize
        } else {
            0
        };
        for i in 0..str_count {
            if let Some(item) = get_index_item(data, pos, i) {
                string_index.push(String::from_utf8_lossy(&item).to_string());
            }
        }
        string_index
    }

    fn determine_glyph_count(data: &[u8], char_strings_offset: Option<usize>) -> u16 {
        if let Some(o) = char_strings_offset {
            if o + 2 <= data.len() { u16::from_be_bytes([data[o], data[o + 1]]) } else { 1024 }
        } else {
            1024
        }
    }

    fn apply_default_charset(map: &mut BTreeMap<u32, u32>, is_cid: bool, num_glyphs: u16) {
        if is_cid {
            for gid in 0..num_glyphs {
                map.insert(u32::from(gid), u32::from(gid));
            }
        } else {
            // ISOAdobe fallback for simple fonts
            for gid in 1..std::cmp::min(num_glyphs, 229) {
                map.insert(u32::from(gid), u32::from(gid));
            }
        }
    }

    #[allow(clippy::collapsible_if)]
    fn derive_name_map(
        data: &[u8],
        sid_map: &BTreeMap<u32, u32>,
        string_idx_pos: usize,
    ) -> BTreeMap<String, u32> {
        let mut nm = BTreeMap::new();
        log::debug!(
            "[RECONSTRUCT] Deriving name map for {} SIDs, String INDEX at {}",
            sid_map.len(),
            string_idx_pos
        );
        use crate::cff_standard::CFF_STANDARD_STRINGS;

        for (&sid, &gid) in sid_map {
            let name = if sid <= CFF_LAST_STANDARD_SID {
                CFF_STANDARD_STRINGS[sid as usize].to_string()
            } else {
                let custom_idx = (sid - (CFF_LAST_STANDARD_SID + 1)) as usize;
                if let Some(item) = get_index_item(data, string_idx_pos, custom_idx) {
                    String::from_utf8_lossy(&item).to_string()
                } else {
                    format!("c{sid:03}")
                }
            };
            nm.insert(name.clone(), gid);
            log::debug!("[RECONSTRUCT] Derived name: {name} -> GID {gid}");

            // Add Unicode alias for standard SIDs (e.g. "!" for "exclam")
            if sid <= CFF_LAST_STANDARD_SID {
                if let Some(c) = Self::standard_sid_to_unicode(sid) {
                    nm.insert(c.to_string(), gid);
                    log::debug!("[RECONSTRUCT] Added Unicode alias: {c} -> GID {gid}");
                }
            }
        }
        nm
    }

    fn standard_sid_to_unicode(sid: u32) -> Option<char> {
        // Subset of Adobe Glyph List for standard CFF SIDs
        match sid {
            1 => Some(' '),
            2 => Some('!'),
            3 => Some('"'),
            4 => Some('#'),
            5 => Some('$'),
            6 => Some('%'),
            7 => Some('&'),
            8 => Some('\''),
            9 => Some('('),
            10 => Some(')'),
            11 => Some('*'),
            12 => Some('+'),
            13 => Some(','),
            14 => Some('-'),
            15 => Some('.'),
            16 => Some('/'),
            17..=26 => Some(std::char::from_u32(0x30 + (sid - 17)).unwrap()), // RR-15 Safe: range is statically bounded inside this match arm // 0-9
            27 => Some(':'),
            28 => Some(';'),
            29 => Some('<'),
            30 => Some('='),
            31 => Some('>'),
            32 => Some('?'),
            33 => Some('@'),
            34..=59 => Some(std::char::from_u32(0x41 + (sid - 34)).unwrap()), // RR-15 Safe: range is statically bounded inside this match arm // A-Z
            60 => Some('['),
            61 => Some('\\'),
            62 => Some(']'),
            63 => Some('^'),
            64 => Some('_'),
            65 => Some('`'),
            66..=91 => Some(std::char::from_u32(0x61 + (sid - 66)).unwrap()), // RR-15 Safe: range is statically bounded inside this match arm // a-z
            92 => Some('{'),
            93 => Some('|'),
            94 => Some('}'),
            95 => Some('~'),
            _ => None,
        }
    }

    fn unicode_to_agl_name(c: char) -> &'static str {
        match c {
            '!' => "exclam",
            '"' => "quotedbl",
            '#' => "numbersign",
            '$' => "dollar",
            '%' => "percent",
            '&' => "ampersand",
            '\'' => "quoteright",
            '(' => "parenleft",
            ')' => "parenright",
            '*' => "asterisk",
            '+' => "plus",
            ',' => "comma",
            '-' => "hyphen",
            '.' => "period",
            '/' => "slash",
            ':' => "colon",
            ';' => "semicolon",
            '<' => "less",
            '=' => "equal",
            '>' => "greater",
            '?' => "question",
            '@' => "at",
            '[' => "bracketleft",
            '\\' => "backslash",
            ']' => "bracketright",
            '^' => "asciicircum",
            '_' => "underscore",
            '`' => "grave",
            '{' => "braceleft",
            '|' => "bar",
            '}' => "braceright",
            '~' => "asciitilde",
            _ => ".notdef",
        }
    }

    fn parse_fdarray_subrs(data: &[u8], offset: usize) {
        log::debug!("[RECONSTRUCT] FDArray: offset {offset}");
        if offset > 0 && offset < data.len() {
            let (_, count, _) = Self::parse_index_header(data, offset).unwrap_or((0, 0, 0));
            for i in 0..count {
                if let Some(fd) = get_index_item(data, offset, i.into()) {
                    let mut fdp = 0;
                    let mut fdops = Vec::new();
                    while fdp < fd.len() {
                        let b0 = fd[fdp];
                        if b0 <= 21 {
                            let mut op = u16::from(b0);
                            fdp += 1;
                            if op == 12 && fdp < fd.len() {
                                op = (op << 8) | u16::from(fd[fdp]);
                                fdp += 1;
                            }
                            if op == 18 && fdops.len() >= 2 {
                                let size = fdops[fdops.len() - 2] as usize;
                                let off = fdops[fdops.len() - 1] as usize;
                                if off + size <= data.len() {
                                    let priv_data = &data[off..off + size];
                                    let mut pp = 0;
                                    let mut pops = Vec::new();
                                    while pp < priv_data.len() {
                                        let pb0 = priv_data[pp];
                                        if pb0 <= 21 {
                                            pp += 1;
                                            pops.clear();
                                        } else {
                                            let (v, l) = parse_dict_number(&priv_data[pp..]);
                                            pops.push(v);
                                            pp += l;
                                        }
                                    }
                                }
                            }
                            fdops.clear();
                        } else {
                            let (v, l) = parse_dict_number(&fd[fdp..]);
                            fdops.push(v);
                            fdp += l;
                        }
                    }
                }
            }
        }
    }

    fn parse_cff_top_dict(
        // RR-15 Limit: Dispatcher - parses operators and offsets in CFF top dictionary
        data: &[u8],
        start: usize,
        count: u16,
    ) -> (Option<usize>, Option<usize>, bool) {
        let mut cso = None;
        let mut cso2 = None;
        let mut is_cid = false;
        if count > 0
            && let Some(dd) = get_index_item(data, start, 0)
        {
            let mut dpos = 0;
            let mut ops = Vec::new();
            while dpos < dd.len() {
                let b0 = dd[dpos];
                if b0 <= 21 {
                    let mut op = u16::from(b0);
                    dpos += 1;
                    if op == 12 && dpos < dd.len() {
                        op = (op << 8) | u16::from(dd[dpos]);
                        dpos += 1;
                    }
                    match op {
                        17 => cso = ops.last().copied().map(|v| v as usize),
                        18 => {
                            if ops.len() >= 2 {
                                let size = ops[ops.len() - 2] as usize;
                                let offset = ops[ops.len() - 1] as usize;
                                log::debug!(
                                    "[RECONSTRUCT] Private DICT: offset {offset}, size {size}"
                                );
                            }
                        }
                        0x0C24 => {
                            let offset = ops.last().copied().unwrap_or(0) as usize;
                            Self::parse_fdarray_subrs(data, offset);
                        }
                        15 => cso2 = ops.last().copied().map(|v| v as usize),
                        0x0C1E | 0x0C1F | 0x0C22 | 0x0C23 | 0x0C16 => is_cid = true,
                        _ => {}
                    }
                    ops.clear();
                } else {
                    let (v, l) = parse_dict_number(&dd[dpos..]);
                    ops.push(v);
                    dpos += l;
                }
            }
        }
        (cso, cso2, is_cid)
    }

    fn parse_cff_charset(data: &[u8], off: usize, num_glyphs: u16) -> Option<BTreeMap<u32, u32>> {
        let mut map = BTreeMap::new();
        let format = data[off];
        let mut cpos = off + 1;
        if format == 0 {
            for gid in 1..num_glyphs {
                if cpos + 2 > data.len() {
                    break;
                }
                let cid = u16::from_be_bytes([data[cpos], data[cpos + 1]]);
                map.insert(u32::from(cid), u32::from(gid));
                cpos += 2;
            }
        } else if format == 1 || format == 2 {
            let mut gid = 1;
            while gid < num_glyphs {
                let sz = if format == 1 { 3 } else { 4 };
                if cpos + sz > data.len() {
                    break;
                }
                let fc = u16::from_be_bytes([data[cpos], data[cpos + 1]]);
                let nl = if format == 1 {
                    u16::from(data[cpos + 2])
                } else {
                    u16::from_be_bytes([data[cpos + 2], data[cpos + 3]])
                };
                cpos += sz;
                for i in 0..=nl {
                    if (u32::from(fc) + u32::from(i)) < 65536 {
                        let cid = u32::from(fc) + u32::from(i);
                        map.insert(cid, u32::from(gid));
                    }
                    gid += 1;
                    if gid >= num_glyphs {
                        break;
                    }
                }
            }
        }
        Some(map)
    }

    fn parse_index_header(data: &[u8], pos: usize) -> Option<(usize, u16, usize)> {
        if pos + 2 > data.len() {
            return None;
        }
        let count = u16::from_be_bytes([data[pos], data[pos + 1]]);
        if count == 0 {
            return Some((pos + 2, 0, 0));
        }
        if pos + 3 > data.len() {
            return None;
        }
        let off_size = data[pos + 2] as usize;
        Some((pos + 3, count, off_size))
    }
}

pub(crate) fn skip_index(data: &[u8], pos: usize) -> usize {
    if pos + 2 > data.len() {
        return pos;
    }
    let count = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
    if count == 0 {
        return pos + 2;
    }
    let os = data[pos + 2] as usize;
    let is = 2 + 1 + (count + 1) * os;
    if pos + is > data.len() {
        return pos;
    }
    let lo = pos + 3 + count * os;
    let mut off = 0;
    for j in 0..os {
        off = (off << 8) | data[lo + j] as usize;
    }
    pos + is + off - 1
}

/// One item of a CFF INDEX, by position.
///
/// Every offset here comes out of the file, and every read was unchecked. A font whose
/// INDEX names an item running past the end walked straight off the buffer:
/// `isartor-6-3-2-t01-fail-b.pdf` panicked with "the len is 37458 but the index is
/// 37458" — one byte past — where an error was the correct answer. The nine files in
/// `samples/` contain no such font and never produced it; 205 files this project did not
/// choose produced it on the first run.
///
/// The function already returned `Option` and every caller already handles `None`, so
/// bounds-checking each read is the whole fix. Arithmetic is checked too, because a
/// wrapped offset panics in a debug build before `get` ever sees it — and `cli_smoke.sh`
/// runs a debug build for exactly that class of reason.
pub(crate) fn get_index_item(data: &[u8], ip: usize, i: usize) -> Option<Vec<u8>> {
    let count = u16::from_be_bytes([*data.get(ip)?, *data.get(ip + 1)?]) as usize;
    if i >= count {
        return None;
    }
    let offset_size = *data.get(ip + 2)? as usize;
    let start_pos = ip.checked_add(3)?.checked_add(i.checked_mul(offset_size)?)?;
    let end_pos = start_pos.checked_add(offset_size)?;

    let mut s = 0usize;
    let mut e = 0usize;
    for j in 0..offset_size {
        s = (s << 8) | *data.get(start_pos + j)? as usize;
        e = (e << 8) | *data.get(end_pos + j)? as usize;
    }

    let base = ip.checked_add(3)?.checked_add(count.checked_add(1)?.checked_mul(offset_size)?)?;
    let ds = base.checked_add(s)?.checked_sub(1)?;
    let de = base.checked_add(e)?.checked_sub(1)?;
    // `ds <= de` was not checked either. A backwards pair panics on the slice even when
    // both ends are inside the buffer, which the `de <= data.len()` guard alone allows.
    if ds <= de && de <= data.len() { Some(data[ds..de].to_vec()) } else { None }
}

fn parse_dict_number(d: &[u8]) -> (i32, usize) {
    let b0 = d[0];
    if b0 == 30 {
        let mut len = 1;
        while len < d.len() {
            let b = d[len];
            len += 1;
            if (b & 0x0F) == 0x0F || (b >> 4) == 0x0F {
                break;
            }
        }
        (0, len)
    } else if b0 == 28 {
        (i32::from(u16::from_be_bytes([d[1], d[2]]) as i16), 3)
    } else if b0 == 29 {
        (i32::from_be_bytes([d[1], d[2], d[3], d[4]]), 5)
    } else if (32..=246).contains(&b0) {
        (i32::from(b0) - 139, 1)
    } else if (247..=250).contains(&b0) {
        ((i32::from(b0) - 247) * 256 + i32::from(d[1]) + 108, 2)
    } else if (251..=254).contains(&b0) {
        (-(i32::from(b0) - 251) * 256 - i32::from(d[1]) - 108, 2)
    } else {
        (0, 1)
    }
}

/// Where the first font's table directory starts.
///
/// **A collection puts its fonts behind a header**, and every face installed on a macOS
/// system is one: all four of the platform faces this engine finds there are `ttcf`,
/// measured 2026-09-19. Reading a table directory at offset 0 of one of those reads the
/// collection header as if it were a font, so `OS/2` is not found, `glyf` is not found,
/// and the face reads as carrying neither — which is indistinguishable from a face that
/// states no permission and has no outlines.
///
/// Font 0 is the one taken here, which is a choice a collection does not make for us.
/// Where a face is chosen to embed, [`crate::metrics::regular_face`] chooses it and
/// [`crate::subset::standalone_face`] takes it out, so that what reaches this is a font of
/// its own.
pub(crate) fn sfnt_base(s: &[u8]) -> usize {
    sfnt_base_at(s, 0)
}

/// Where the table directory of face `index` starts.
///
/// **A collection is several faces and the first is not a choice.** Hiragino ships
/// `ProN W3`, `Pro W3`, `ProN W6` and `Pro W6` in one file, and Helvetica six weights, so
/// which one is wanted is decided by what each states ([`crate::metrics::regular_face`]),
/// not by its position.
pub(crate) fn sfnt_base_at(s: &[u8], index: u32) -> usize {
    if s.len() < 16 || &s[0..4] != b"ttcf" {
        return 0;
    }
    let at = 12 + (index as usize) * 4;
    let Some(bytes) = s.get(at..at + 4) else { return 0 };
    let base = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if base + 12 <= s.len() { base } else { 0 }
}

/// How many faces `s` holds: one, unless it is a collection.
#[must_use]
pub fn face_count(s: &[u8]) -> u32 {
    if s.len() < 12 || &s[0..4] != b"ttcf" {
        return 1;
    }
    u32::from_be_bytes([s[8], s[9], s[10], s[11]])
}

/// Where the table tagged `t` lies in the sfnt program `s` — its first face, in a
/// collection — as the start and end byte offsets its directory states.
///
/// **The range is the directory's word**, not checked against the program's length, so a
/// caller slices it with `get` rather than indexing.
pub fn find_table_range(s: &[u8], t: &[u8; 4]) -> Option<(usize, usize)> {
    find_table_range_at(s, t, sfnt_base(s))
}

/// The same, in the face whose directory starts at `base`.
pub(crate) fn find_table_range_at(s: &[u8], t: &[u8; 4], base: usize) -> Option<(usize, usize)> {
    if s.len() < 12 {
        return None;
    }
    let nt = u16::from_be_bytes([*s.get(base + 4)?, *s.get(base + 5)?]) as usize;
    for i in 0..nt {
        let e = base + 12 + i * 16;
        if e + 16 > s.len() {
            break;
        }
        if &s[e..e + 4] == t {
            let o = u32::from_be_bytes([s[e + 8], s[e + 9], s[e + 10], s[e + 11]]) as usize;
            let l = u32::from_be_bytes([s[e + 12], s[e + 13], s[e + 14], s[e + 15]]) as usize;
            return Some((o, o + l));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_format_detection() {
        // SFNT
        assert_eq!(FontFormat::detect(b"OTTO"), FontFormat::Sfnt);
        assert_eq!(FontFormat::detect(&[0, 1, 0, 0]), FontFormat::Sfnt);
        assert_eq!(FontFormat::detect(b"ttcf"), FontFormat::Sfnt);
        assert_eq!(FontFormat::detect(b"true"), FontFormat::Sfnt);

        // CFF
        assert_eq!(FontFormat::detect(&[1, 0, 4, 1]), FontFormat::Cff1);
        assert_eq!(FontFormat::detect(&[2, 0, 5]), FontFormat::Cff2);

        // Type 1
        assert_eq!(FontFormat::detect(&[0x80, 0x01, 0x01]), FontFormat::Type1Pfb);
        assert_eq!(FontFormat::detect(b"%!PS-AdobeFont"), FontFormat::Type1Pfa);
        assert_eq!(FontFormat::detect(b"%!FontType1"), FontFormat::Type1Pfa);

        // Unknown
        assert_eq!(FontFormat::detect(b"abc"), FontFormat::Unknown);
        assert_eq!(FontFormat::detect(&[0x12, 0x34]), FontFormat::Unknown);
    }

    struct TestFontInfo;
    impl FontInfo for TestFontInfo {
        fn base_font(&self) -> &str {
            "Test"
        }
        fn subtype(&self) -> &str {
            "TrueType"
        }
        fn is_cid_keyed(&self) -> bool {
            false
        }
        fn is_cjk(&self) -> bool {
            false
        }
        fn cid_ordering(&self) -> Option<&str> {
            None
        }
        fn cid_to_gid_map(&self) -> Option<&BTreeMap<u32, u32>> {
            None
        }
        fn unified_map(&self) -> &BTreeMap<String, u32> {
            static EMPTY: std::sync::OnceLock<BTreeMap<String, u32>> = std::sync::OnceLock::new();
            EMPTY.get_or_init(BTreeMap::new)
        }
        fn encoding(&self) -> Option<&CMap> {
            None
        }
        fn glyph_width_by_gid(&self, _gid: u32) -> f32 {
            1000.0
        }
        fn to_gid_hint(&self, cid: u32, _hint_name: Option<&str>) -> u32 {
            cid
        }
    }

    #[test]
    fn test_cff2_wrapping() {
        let dummy_cff2 = vec![2, 0, 5, 1, 2, 3, 4, 5];
        let resource = TestFontInfo;

        let res = FontReconstructor::wrap_naked_outline(*b"CFF2", &dummy_cff2, &resource).unwrap();
        assert_eq!(FontFormat::detect(&res.data), FontFormat::Sfnt);

        let dis = FontReconstructor::disassemble_sfnt(&res.data).unwrap();
        assert!(dis.tables.iter().any(|(t, _)| t == b"CFF2"));
    }

    #[test]
    fn test_ttcf_disassembly() {
        let mut ttcf = vec![0; 64];
        ttcf[0..4].copy_from_slice(b"ttcf");
        ttcf[8..12].copy_from_slice(&1u32.to_be_bytes()); // numFonts
        ttcf[12..16].copy_from_slice(&32u32.to_be_bytes()); // offset to first font at 32

        // First font header at offset 32
        ttcf[32..36].copy_from_slice(b"OTTO");
        ttcf[36..38].copy_from_slice(&1u16.to_be_bytes()); // numTables = 1

        // Table entry at offset 32 + 12 = 44
        ttcf[44..48].copy_from_slice(b"TEST");
        ttcf[52..56].copy_from_slice(&60u32.to_be_bytes()); // offset to data at 60
        ttcf[56..60].copy_from_slice(&4u32.to_be_bytes()); // length

        // Table data at offset 60
        ttcf[60..64].copy_from_slice(b"DATA");

        let dis = FontReconstructor::disassemble_sfnt(&ttcf).expect("Failed to disassemble TTCF");
        assert_eq!(dis.magic, *b"OTTO");
        assert_eq!(dis.tables.len(), 1);
        assert_eq!(dis.tables[0].0, *b"TEST");
        assert_eq!(dis.tables[0].1, b"DATA");
    }

    // `test_reconstructed_font_parsing` stood here, `#[ignore]`d, reading
    // `exports/font-0003.otf` — a path no fixture produces, in a directory
    // `DIRECTORY_LAYOUT.md` does not register. It had therefore never run, in either
    // sense: the attribute stopped it, and the file was not there if the attribute had
    // not. A test that has never executed is not evidence of anything, and this one read
    // as coverage of font reconstruction. `TESTING.md` records the same removal being
    // made once before for the same reason.
    //
    // What it was reaching for is worth building: parse a reconstructed font with an
    // independent implementation and pull an outline out of GID 1. That needs a fixture
    // this crate generates, not a file left behind by a debug command. Its `NoopBuilder`
    // went with it — clippy under `-D warnings` named it the moment its only constructor
    // was gone, which is more than the ignored test had managed in its whole existence.
}

#[cfg(test)]
mod index_bounds_tests {
    use super::get_index_item;

    /// A CFF INDEX whose offsets run past the end of the buffer must return `None`.
    ///
    /// `isartor-6-3-2-t01-fail-b.pdf` panicked here — "the len is 37458 but the index is
    /// 37458", one byte past — because every read in this function indexed the slice
    /// directly. None of the nine files in `samples/` contains such a font, which is why
    /// it took a corpus this project did not choose to find it.
    #[test]
    fn an_index_pointing_past_the_end_is_declined_rather_than_panicking() {
        // count = 1, offset size = 1, offsets [1, 200] — the item ends far outside.
        let data = [0x00, 0x01, 0x01, 0x01, 0xC8];
        assert_eq!(get_index_item(&data, 0, 0), None);

        // Truncated before the offsets can even be read.
        assert_eq!(get_index_item(&[0x00, 0x01], 0, 0), None);
        assert_eq!(get_index_item(&[0x00, 0x01, 0x01], 0, 0), None);

        // An `ip` past the end of the buffer entirely.
        assert_eq!(get_index_item(&data, 900, 0), None);

        // Backwards offsets: both ends inside the buffer, and the slice would still
        // panic. The old guard checked only the far end.
        let backwards = [0x00, 0x01, 0x01, 0x05, 0x02, 0, 0, 0, 0, 0];
        assert_eq!(get_index_item(&backwards, 0, 0), None);

        // And an item that is genuinely present still comes back.
        let good = [0x00, 0x01, 0x01, 0x01, 0x03, b'h', b'i'];
        assert_eq!(get_index_item(&good, 0, 0), Some(vec![b'h', b'i']));
    }
}

#[cfg(test)]
mod charstring_number_tests {
    use super::FontReconstructor;

    /// Decodes one operand the way a conforming reader does, and says which form it was.
    ///
    /// The arms are `read-fonts` 0.37.0 `postscript::dict::parse_int` and
    /// `postscript::charstring`, which is the reader this workspace renders with: 28 is a
    /// 16-bit integer in both a DICT and a charstring, 29 a 32-bit integer in a DICT
    /// only, and 255 a 16.16 fixed-point value in a charstring and a real number in a
    /// DICT. Neither reads 255 as an integer.
    fn decode(bytes: &[u8]) -> (&'static str, f64) {
        match bytes[0] {
            32..=246 => ("small", f64::from(i32::from(bytes[0]) - 139)),
            247..=250 => {
                ("small", f64::from((i32::from(bytes[0]) - 247) * 256 + i32::from(bytes[1]) + 108))
            }
            251..=254 => {
                ("small", f64::from(-(i32::from(bytes[0]) - 251) * 256 - i32::from(bytes[1]) - 108))
            }
            28 => ("int16", f64::from(i16::from_be_bytes([bytes[1], bytes[2]]))),
            29 => {
                ("int32", f64::from(i32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]])))
            }
            255 => (
                "fixed16.16",
                f64::from(i32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]])) / 65536.0,
            ),
            other => panic!("no operand form begins {other}"),
        }
    }

    /// A Type 2 operand past ±1131 was written in Type 1's form and read at 1/65536.
    ///
    /// `push_t2_number` emitted 255 followed by a big-endian `i32`. In a Type 2
    /// charstring 255 introduces a 16.16 fixed-point value, so a 2,000-unit coordinate
    /// came back as 0.03. The 28 form — the only integer a Type 2 charstring has — was
    /// never emitted for any value.
    #[test]
    fn a_type_2_operand_is_never_written_in_type_1s_form() {
        for val in [1132, -1132, 2000, -2000, 16384, -16384, 32767, -32768] {
            let mut out = Vec::new();
            FontReconstructor::push_t2_number(&mut out, val);
            let (form, got) = decode(&out);
            assert_eq!(form, "int16", "{val} took the {form} form");
            assert_eq!(got, f64::from(val), "{val} reads back as {got}");
        }
    }

    /// The shared forms are still the shared forms, and still shared.
    #[test]
    fn the_small_forms_encode_alike_in_both_a_dict_and_a_charstring() {
        for val in [0, 1, -1, 107, -107, 108, -108, 1131, -1131] {
            let (mut dict, mut cs) = (Vec::new(), Vec::new());
            FontReconstructor::push_cff_dict_number(&mut dict, val);
            FontReconstructor::push_t2_number(&mut cs, val);
            assert_eq!(dict, cs, "{val} is encoded differently by the two");
            assert_eq!(decode(&dict), ("small", f64::from(val)), "{val}");
        }
    }

    /// A DICT keeps both of its wide forms; only the charstring lost one.
    #[test]
    fn a_dict_operand_still_widens_to_32_bits() {
        for (val, form) in [(1132, "int16"), (-32768, "int16"), (70000, "int32")] {
            let mut out = Vec::new();
            FontReconstructor::push_cff_dict_number(&mut out, val);
            let (got_form, got) = decode(&out);
            assert_eq!(got_form, form, "{val}");
            assert_eq!(got, f64::from(val), "{val}");
        }
    }
}

#[cfg(test)]
mod type1_subroutine_fan_out_tests {
    use super::FontReconstructor;
    use std::sync::mpsc;
    use std::time::Duration;

    /// Encrypts a charstring the way 7.2 of the Type 1 specification does, with `len_iv`
    /// leading zero bytes, so that `decrypt_charstring` gives `plain` back.
    fn encrypt_charstring(plain: &[u8], len_iv: usize) -> Vec<u8> {
        let mut r: u16 = 4330;
        let mut out = Vec::with_capacity(len_iv + plain.len());
        for &p in std::iter::repeat_n(&0u8, len_iv).chain(plain) {
            let c = p ^ (r >> 8) as u8;
            r = u16::from(c).wrapping_add(r).wrapping_mul(52845).wrapping_add(22719);
            out.push(c);
        }
        out
    }

    /// A subroutine that calls itself sixteen times finishes converting.
    ///
    /// The depth cap of 10 bounds how deep the calls go and not how many there are: a
    /// subroutine making k calls of its own costs k^10. Measured 2026-10-07 in a debug
    /// build, k = 5 took 2.3 s, 6 took 15.5 s and 7 took 79 s; at 16 it does not finish.
    /// It is reached through `perform_reconstruction` by a `/FontFile` holding a PFB
    /// program, which a hostile file can embed; a conforming PFA-style program fails
    /// `parse_pfb` before any charstring is converted (ROADMAP Z-2). PrintCraft found the
    /// same shape in Type 3 glyphs that show themselves.
    #[test]
    fn a_subroutine_that_calls_itself_many_times_finishes() {
        // `0 callsubr` sixteen times: 139 is the operand 0, 10 is callsubr.
        let subr: Vec<u8> = std::iter::repeat_n([139u8, 10], 16).flatten().collect();
        let subrs = vec![encrypt_charstring(&subr, 4)];
        let glyph = encrypt_charstring(&[139, 10, 14], 4);

        let (done, finished) = mpsc::channel();
        std::thread::spawn(move || {
            let converted = FontReconstructor::convert_t1_to_t2(&glyph, &subrs, 4);
            let _ = done.send(converted);
        });
        let converted = finished
            .recv_timeout(Duration::from_secs(10))
            .expect("a self-calling subroutine is still converting after 10 s");
        assert_eq!(converted.last(), Some(&14), "the glyph still ends with endchar");
    }
}
