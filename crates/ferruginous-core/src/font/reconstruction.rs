use crate::font::FontResource;
use crate::{PdfError, PdfResult};
use std::collections::BTreeMap;

/// A surgical patcher for SFNT binaries.
pub struct FontReconstructor;

struct DisassembledSfnt {
    magic: [u8; 4],
    tables: Vec<([u8; 4], Vec<u8>)>,
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
    pub fn detect_with_resource(data: &[u8], resource: &crate::font::FontResource) -> Self {
        let format = Self::detect(data);

        // Hardening (RR-15): If metadata says CIDFontType0 (CFF) but we detected Type1Pfb,
        // it might be a misidentified CFF font or a CID-keyed Type 1 font.
        // We check if it's actually CFF but just has a weird start (coincidental 0x80 0x01).
        if format == FontFormat::Type1Pfb && resource.subtype.as_str() == "CIDFontType0" {
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
    pub fn reconstruct(resource: &FontResource, raw_data: &[u8]) -> PdfResult<ReconstructedFont> {
        let format = FontFormat::detect_with_resource(raw_data, resource);
        let sig = if raw_data.len() >= 4 {
            format!("{:02x}{:02x}{:02x}{:02x}", raw_data[0], raw_data[1], raw_data[2], raw_data[3])
        } else {
            "short".to_string()
        };
        log::debug!(
            "[RECONSTRUCT] Starting reconstruction for {} (format: {:?}, size: {} bytes, sig: {})",
            resource.base_font.as_str(),
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
        resource: &FontResource,
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
                log::debug!("[RECONSTRUCT] Discovered native num_glyphs: {}", n);
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
                num_glyphs: native_num_glyphs.map(|n| n as u32),
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
        resource: &FontResource,
        name_to_gid: &mut BTreeMap<String, u32>,
    ) -> Option<BTreeMap<u32, u32>> {
        let mut map = BTreeMap::new();
        if let Ok(face) = ttf_parser::Face::parse(data, 0) {
            // 1. Build an internal Unicode -> GID map from the font's own cmap tables
            let mut internal_u2g = BTreeMap::new();

            // 1.1 Extract Glyph Names if available (Essential for subsetted fonts)
            for gid in 0..face.number_of_glyphs() {
                if let Some(name) = face.glyph_name(ttf_parser::GlyphId(gid)) {
                    name_to_gid.insert(name.to_string(), gid as u32);
                }
            }

            for table in face.tables().cmap.iter().flat_map(|t| t.subtables) {
                if table.is_unicode() {
                    table.codepoints(|cp| {
                        if let Some(gid) = table.glyph_index(cp) {
                            internal_u2g.insert(cp, gid.0 as u32);
                        }
                    });
                } else {
                    table.codepoints(|cp| {
                        if let Some(gid) = table.glyph_index(cp) {
                            map.insert(cp, gid.0 as u32);
                        }
                    });
                }
            }

            // 2. Bridge via unified_map (ToUnicode): maps logical Unicode back to character codes.
            // This allows us to find the correct GID for a character code even if the font's
            // internal cmap is strictly Unicode or has lying Identity entries.
            // Priority: PDF metrics (ToUnicode) ALWAYS override font internal cmaps.
            for (uni_str, &code) in &resource.unified_map {
                if let Some(c) = uni_str.chars().next()
                    && let Some(&gid) = internal_u2g.get(&(c as u32))
                {
                    map.insert(code, gid);
                }
            }
        }
        if map.is_empty() { None } else { Some(map) }
    }

    fn normalize_sfnt_format(data: &[u8], resource: &FontResource) -> PdfResult<ReconstructedFont> {
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
            log::debug!(
                "[RECONSTRUCT] Patching SFNT cmap table for {}",
                resource.base_font.as_str()
            );
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
        resource: &FontResource,
    ) -> PdfResult<ReconstructedFont> {
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
                log::warn!(
                    "[RECONSTRUCT] Unrecognized font format, using raw data as placeholder SFNT"
                );
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

    fn transcode_type1_to_cff(
        data: &[u8],
        resource: &FontResource,
    ) -> PdfResult<ReconstructedFont> {
        let segments = Self::parse_pfb(data)?;
        log::info!(
            "[RECONSTRUCT] Type 1 segments extracted for {}: ASCII={} bytes, Binary={} bytes, Trailer={} bytes",
            resource.base_font.as_str(),
            segments.ascii.len(),
            segments.binary.len(),
            segments.trailer.len()
        );

        // 1. Decrypt the eexec segment
        let decrypted_eexec = Self::decrypt_type1(&segments.binary, 55665, 4);

        // 2. Parse Type 1 Data
        let t1_data = Self::parse_type1_data(&segments.ascii, &decrypted_eexec)?;

        // 3. Transcode CharStrings from T1 to T2
        let mut t2_charstrings = Vec::new();
        let mut glyph_names = Vec::new();
        for (name, t1_bytes) in t1_data.charstrings {
            let t2_bytes = Self::convert_t1_to_t2(&t1_bytes, &t1_data.subrs, t1_data.len_iv);
            t2_charstrings.push(t2_bytes);
            glyph_names.push(name);
        }

        // 4. Serialize to CFF
        let cff_data = Self::serialize_cff(&glyph_names, &t2_charstrings);

        // 5. Wrap in SFNT
        Self::wrap_naked_outline(*b"CFF ", &cff_data, resource)
    }

    fn serialize_cff(_names: &[String], charstrings: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        // Header
        out.extend_from_slice(&[1, 0, 4, 4]); // major, minor, hdrSize, offSize=4

        // Name INDEX
        Self::push_cff_index(&mut out, &[b"TranscodedFont"]);

        // Top DICT INDEX (Pre-calculate offsets)
        // We'll build the rest first to know the offsets.
        let mut charstrings_buf = Vec::new();
        let charstring_refs: Vec<&[u8]> = charstrings.iter().map(|v| v.as_slice()).collect();
        Self::push_cff_index(&mut charstrings_buf, &charstring_refs);

        let mut private_dict = Vec::new();
        Self::push_cff_dict_entry(&mut private_dict, 20, &[0]); // defaultWidthX
        Self::push_cff_dict_entry(&mut private_dict, 21, &[0]); // nominalWidthX

        // 1st Pass: Build Top DICT with fixed-size placeholders for offsets
        let mut top_dict = Vec::new();
        Self::push_cff_dict_number_fixed(&mut top_dict, 0); // Placeholder CharStrings
        top_dict.push(17);
        Self::push_cff_dict_number_fixed(&mut top_dict, 0); // Placeholder Private size
        Self::push_cff_dict_number_fixed(&mut top_dict, 0); // Placeholder Private offset
        top_dict.push(18);

        let top_dict_size = top_dict.len();
        let top_dict_index_header_size = 2 + 1 + 1 + 4; // count(2) + offSize(1) + offset1(1) + offset2(4)

        let mut string_idx = Vec::new();
        Self::push_cff_index(&mut string_idx, &[]);
        let mut gsubr_idx = Vec::new();
        Self::push_cff_index(&mut gsubr_idx, &[]);

        let charstrings_pos = out.len()
            + top_dict_index_header_size
            + top_dict_size
            + string_idx.len()
            + gsubr_idx.len();
        let private_pos = charstrings_pos + charstrings_buf.len();

        // 2nd Pass: Build actual Top DICT using fixed-size numbers to match calculated size
        top_dict.clear();
        Self::push_cff_dict_number_fixed(&mut top_dict, charstrings_pos as i32);
        top_dict.push(17);
        Self::push_cff_dict_number_fixed(&mut top_dict, private_dict.len() as i32);
        Self::push_cff_dict_number_fixed(&mut top_dict, private_pos as i32);
        top_dict.push(18);

        Self::push_cff_index(&mut out, &[&top_dict]);
        out.extend_from_slice(&string_idx);
        out.extend_from_slice(&gsubr_idx);
        out.extend_from_slice(&charstrings_buf);
        out.extend_from_slice(&private_dict);

        out
    }

    fn push_cff_index(out: &mut Vec<u8>, entries: &[&[u8]]) {
        let count = entries.len() as u16;
        out.extend_from_slice(&count.to_be_bytes());
        if count == 0 {
            return;
        }

        out.push(4); // offSize
        let mut offset = 1u32;
        out.extend_from_slice(&offset.to_be_bytes());
        for entry in entries {
            offset += entry.len() as u32;
            out.extend_from_slice(&offset.to_be_bytes());
        }
        for entry in entries {
            out.extend_from_slice(entry);
        }
    }

    fn push_cff_dict_entry(out: &mut Vec<u8>, op: u8, args: &[i32]) {
        for &arg in args {
            Self::push_cff_dict_number(out, arg);
        }
        out.push(op);
    }

    fn push_cff_dict_number(out: &mut Vec<u8>, val: i32) {
        if (-107..=107).contains(&val) {
            out.push((val + 139) as u8);
        } else if (108..=1131).contains(&val) {
            let v = val - 108;
            out.push((v / 256 + 247) as u8);
            out.push((v % 256) as u8);
        } else if (-1131..=-108).contains(&val) {
            let v = -val - 108;
            out.push((v / 256 + 251) as u8);
            out.push((v % 256) as u8);
        } else if (-32768..=32767).contains(&val) {
            out.push(28);
            out.extend_from_slice(&(val as i16).to_be_bytes());
        } else {
            out.push(29);
            out.extend_from_slice(&val.to_be_bytes());
        }
    }

    fn push_cff_dict_number_fixed(out: &mut Vec<u8>, val: i32) {
        out.push(29);
        out.extend_from_slice(&val.to_be_bytes());
    }

    fn parse_subrs(full_text: &[u8], pos: usize, subrs: &mut Vec<Vec<u8>>) {
        let mut search_pos = pos;
        while let Some(dup_pos) = Self::find_subslice(&full_text[search_pos..], b"dup") {
            let current_dup = search_pos + dup_pos;
            let chunk = &full_text[current_dup..std::cmp::min(current_dup + 50, full_text.len())];
            let chunk_str = String::from_utf8_lossy(chunk);
            let parts: Vec<&str> = chunk_str.split_whitespace().collect();
            if parts.len() >= 3
                && parts[0] == "dup"
                && let Ok(index) = parts[1].parse::<usize>()
                && let Some((data, next_pos)) =
                    Self::extract_rd_data(full_text, current_dup + 4 + parts[1].len())
            {
                if index >= subrs.len() {
                    subrs.resize(index + 1, Vec::new());
                }
                subrs[index] = data;
                search_pos = next_pos;
                continue;
            }
            search_pos = current_dup + 3;
            if search_pos >= full_text.len()
                || &full_text[search_pos..std::cmp::min(search_pos + 3, full_text.len())] == b"def"
            {
                break;
            }
        }
    }

    fn parse_charstrings(
        full_text: &[u8],
        pos: usize,
        charstrings: &mut BTreeMap<String, Vec<u8>>,
    ) {
        let mut search_pos = pos;
        while let Some(name_pos) = Self::find_next_name(full_text, search_pos) {
            let name = Self::extract_name(full_text, name_pos);
            if name == "CharStrings" || name == "dict" || name == "begin" || name == "end" {
                search_pos = name_pos + name.len() + 1;
                continue;
            }

            if let Some((data, next_pos)) = Self::extract_rd_data(full_text, name_pos + name.len())
            {
                charstrings.insert(name, data);
                search_pos = next_pos;
            } else {
                search_pos = name_pos + name.len() + 1;
            }

            if search_pos >= full_text.len()
                || &full_text[search_pos..std::cmp::min(search_pos + 3, full_text.len())] == b"end"
            {
                break;
            }
        }
    }

    fn parse_type1_data(ascii: &[u8], binary: &[u8]) -> PdfResult<Type1Data> {
        let mut charstrings = BTreeMap::new();
        let mut subrs = Vec::new();
        let mut len_iv = 4;

        let mut full_text = Vec::with_capacity(ascii.len() + binary.len());
        full_text.extend_from_slice(ascii);
        full_text.extend_from_slice(binary);

        if let Some(pos) = Self::find_subslice(&full_text, b"/lenIV") {
            let chunk = &full_text[pos..std::cmp::min(pos + 20, full_text.len())];
            if let Some(val) = Self::extract_number(chunk) {
                len_iv = val as usize;
            }
        }

        if let Some(pos) = Self::find_subslice(&full_text, b"/Subrs") {
            Self::parse_subrs(&full_text, pos, &mut subrs);
        }

        if let Some(pos) = Self::find_subslice(&full_text, b"/CharStrings") {
            Self::parse_charstrings(&full_text, pos, &mut charstrings);
        }

        Ok(Type1Data { charstrings, subrs, len_iv })
    }

    fn convert_t1_to_t2(t1_bytes: &[u8], subrs: &[Vec<u8>], len_iv: usize) -> Vec<u8> {
        let mut t2_bytes = Vec::new();
        let mut stack = Vec::new();
        let mut width_written = false;

        let decrypted = Self::decrypt_charstring(t1_bytes, len_iv);

        Self::convert_recursive(
            &decrypted,
            subrs,
            len_iv,
            &mut t2_bytes,
            &mut stack,
            &mut width_written,
            0,
        );

        // Ensure it ends with endchar if not already present
        if t2_bytes.last() != Some(&14) {
            t2_bytes.push(14);
        }

        t2_bytes
    }

    fn parse_t1_number(t1_bytes: &[u8], b: u8, i: usize) -> (i32, usize) {
        if b <= 246 {
            (b as i32 - 139, i + 1)
        } else if b <= 250 {
            ((b as i32 - 247) * 256 + t1_bytes[i + 1] as i32 + 108, i + 2)
        } else if b <= 254 {
            (-(b as i32 - 251) * 256 - t1_bytes[i + 1] as i32 - 108, i + 2)
        } else {
            let v = i32::from_be_bytes([
                t1_bytes[i + 1],
                t1_bytes[i + 2],
                t1_bytes[i + 3],
                t1_bytes[i + 4],
            ]);
            (v, i + 5)
        }
    }

    fn handle_escape_sequence(
        b2: u8,
        t2_bytes: &mut Vec<u8>,
        stack: &mut Vec<i32>,
        width_written: &mut bool,
    ) {
        match b2 {
            6 => {
                if stack.len() >= 5 {
                    let adx = stack[1];
                    let ady = stack[2];
                    let bchar = stack[3];
                    let achar = stack[4];
                    Self::push_t2_number(t2_bytes, adx);
                    Self::push_t2_number(t2_bytes, ady);
                    Self::push_t2_number(t2_bytes, bchar);
                    Self::push_t2_number(t2_bytes, achar);
                    t2_bytes.push(14);
                }
                stack.clear();
            }
            7 => {
                if stack.len() >= 4 {
                    let wx = stack[2];
                    if !*width_written {
                        Self::push_t2_number(t2_bytes, wx);
                        *width_written = true;
                    }
                }
                stack.clear();
            }
            _ => {
                stack.clear();
            }
        }
    }

    fn push_operator(t2_bytes: &mut Vec<u8>, stack: &mut Vec<i32>, op: u8) {
        for &val in stack.iter() {
            Self::push_t2_number(t2_bytes, val);
        }
        t2_bytes.push(op);
        stack.clear();
    }

    fn handle_callsubr(
        subrs: &[Vec<u8>],
        len_iv: usize,
        t2_bytes: &mut Vec<u8>,
        stack: &mut Vec<i32>,
        width_written: &mut bool,
        depth: usize,
    ) {
        if let Some(idx) = stack.pop()
            && idx >= 0
            && (idx as usize) < subrs.len()
        {
            let decrypted = Self::decrypt_charstring(&subrs[idx as usize], len_iv);
            Self::convert_recursive(
                &decrypted,
                subrs,
                len_iv,
                t2_bytes,
                stack,
                width_written,
                depth + 1,
            );
        }
    }

    fn handle_hsbw(t2_bytes: &mut Vec<u8>, stack: &mut Vec<i32>, width_written: &mut bool) {
        if stack.len() >= 2 {
            let width = stack[stack.len() - 1];
            if !*width_written {
                Self::push_t2_number(t2_bytes, width);
                *width_written = true;
            }
        }
        stack.clear();
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_operator(
        b: u8,
        subrs: &[Vec<u8>],
        len_iv: usize,
        t2_bytes: &mut Vec<u8>,
        stack: &mut Vec<i32>,
        width_written: &mut bool,
        depth: usize,
        i_ref: &mut usize,
        t1_bytes: &[u8],
    ) -> bool {
        match b {
            1 | 3 | 4 | 5 | 6 | 7 | 8 | 21 | 22 | 30 | 31 => {
                Self::push_operator(t2_bytes, stack, b);
            }
            9 => {
                stack.clear();
            }
            10 => {
                Self::handle_callsubr(subrs, len_iv, t2_bytes, stack, width_written, depth);
            }
            11 => {
                return true;
            }
            13 => {
                Self::handle_hsbw(t2_bytes, stack, width_written);
            }
            14 => {
                Self::push_operator(t2_bytes, stack, 14);
            }
            12 => {
                if *i_ref < t1_bytes.len() {
                    let b2 = t1_bytes[*i_ref];
                    *i_ref += 1;
                    Self::handle_escape_sequence(b2, t2_bytes, stack, width_written);
                }
            }
            _ => {
                stack.clear();
            }
        }
        false
    }

    fn convert_recursive(
        t1_bytes: &[u8],
        subrs: &[Vec<u8>],
        len_iv: usize,
        t2_bytes: &mut Vec<u8>,
        stack: &mut Vec<i32>,
        width_written: &mut bool,
        depth: usize,
    ) {
        if depth > 10 {
            return;
        }

        let mut i = 0;
        while i < t1_bytes.len() {
            let b = t1_bytes[i];
            if b >= 32 {
                let (val, next_i) = Self::parse_t1_number(t1_bytes, b, i);
                stack.push(val);
                i = next_i;
            } else {
                i += 1;
                if Self::handle_operator(
                    b,
                    subrs,
                    len_iv,
                    t2_bytes,
                    stack,
                    width_written,
                    depth,
                    &mut i,
                    t1_bytes,
                ) {
                    return;
                }
            }
        }
    }

    fn push_t2_number(out: &mut Vec<u8>, val: i32) {
        if (-107..=107).contains(&val) {
            out.push((val + 139) as u8);
        } else if (108..=1131).contains(&val) {
            let v = val - 108;
            out.push((v / 256 + 247) as u8);
            out.push((v % 256) as u8);
        } else if (-1131..=-108).contains(&val) {
            let v = -val - 108;
            out.push((v / 256 + 251) as u8);
            out.push((v % 256) as u8);
        } else {
            out.push(255);
            out.extend_from_slice(&val.to_be_bytes());
        }
    }

    fn decrypt_charstring(data: &[u8], len_iv: usize) -> Vec<u8> {
        Self::decrypt_type1(data, 4330, len_iv)
    }

    fn find_subslice(data: &[u8], sub: &[u8]) -> Option<usize> {
        data.windows(sub.len()).position(|w| w == sub)
    }

    fn extract_number(data: &[u8]) -> Option<i32> {
        let s = String::from_utf8_lossy(data);
        s.split_whitespace().find_map(|p| p.parse::<i32>().ok())
    }

    fn find_next_name(data: &[u8], start: usize) -> Option<usize> {
        data[start..].iter().position(|&b| b == b'/').map(|p| start + p)
    }

    fn extract_name(data: &[u8], pos: usize) -> String {
        let mut end = pos + 1;
        while end < data.len()
            && !data[end].is_ascii_whitespace()
            && data[end] != b'/'
            && data[end] != b'{'
            && data[end] != b'['
        {
            end += 1;
        }
        String::from_utf8_lossy(&data[pos + 1..end]).to_string()
    }

    fn extract_rd_data(data: &[u8], pos: usize) -> Option<(Vec<u8>, usize)> {
        // Look for "<number> RD" or "<number> -|"
        let mut i = pos;
        while i < data.len() && data[i].is_ascii_whitespace() {
            i += 1;
        }
        let start_num = i;
        while i < data.len() && !data[i].is_ascii_whitespace() {
            i += 1;
        }
        let num_str = String::from_utf8_lossy(&data[start_num..i]);
        let len = num_str.parse::<usize>().ok()?;

        while i < data.len() && data[i].is_ascii_whitespace() {
            i += 1;
        }
        let op_start = i;
        while i < data.len() && !data[i].is_ascii_whitespace() {
            i += 1;
        }
        let op = &data[op_start..i];

        if op == b"RD" || op == b"-|" {
            let data_start = i + 1; // Usually a space after RD
            if data_start + len <= data.len() {
                return Some((data[data_start..data_start + len].to_vec(), data_start + len));
            }
        }
        None
    }

    fn decrypt_type1(data: &[u8], mut r: u16, n: usize) -> Vec<u8> {
        if data.len() <= n {
            return Vec::new();
        }
        let mut output = Vec::with_capacity(data.len() - n);
        let c1: u16 = 52845;
        let c2: u16 = 22719;

        for (i, &b) in data.iter().enumerate() {
            let plain = b ^ (r >> 8) as u8;
            if i >= n {
                output.push(plain);
            }
            r = (b as u16).wrapping_add(r).wrapping_mul(c1).wrapping_add(c2);
        }
        output
    }

    fn parse_pfb(data: &[u8]) -> PdfResult<Type1Segments> {
        let mut ascii = Vec::new();
        let mut binary = Vec::new();
        let mut trailer = Vec::new();
        let mut pos = 0;

        while pos + 6 <= data.len() {
            if data[pos] != 0x80 {
                break;
            }
            let tag = data[pos + 1];
            let len =
                u32::from_le_bytes([data[pos + 2], data[pos + 3], data[pos + 4], data[pos + 5]])
                    as usize;
            pos += 6;

            if pos + len > data.len() {
                return Err(PdfError::Other("Malformed PFB: segment exceeds data length".into()));
            }

            match tag {
                1 => ascii.extend_from_slice(&data[pos..pos + len]),
                2 => binary.extend_from_slice(&data[pos..pos + len]),
                3 => trailer.extend_from_slice(&data[pos..pos + len]),
                _ => {}
            }
            pos += len;
        }

        if ascii.is_empty() && binary.is_empty() {
            return Err(PdfError::Other("Malformed PFB: no valid segments found".into()));
        }

        Ok(Type1Segments { ascii, binary, trailer })
    }

    fn build_name_table(resource: &FontResource) -> Vec<u8> {
        let font_name = resource.base_font.as_str().as_bytes();
        let name_count = 1;
        let mut name_table = vec![0u8; 6 + 12 * name_count + font_name.len()];
        name_table[2..4].copy_from_slice(&(name_count as u16).to_be_bytes());
        name_table[4..6].copy_from_slice(&(6 + 12 * name_count as u16).to_be_bytes());

        // Record 1: Full Name (ID 4)
        name_table[6..8].copy_from_slice(&3u16.to_be_bytes());
        name_table[8..10].copy_from_slice(&1u16.to_be_bytes());
        name_table[10..12].copy_from_slice(&0u16.to_be_bytes());
        name_table[12..14].copy_from_slice(&4u16.to_be_bytes());
        name_table[14..16].copy_from_slice(&(font_name.len() as u16).to_be_bytes());
        name_table[16..18].copy_from_slice(&0u16.to_be_bytes());
        name_table[18..18 + font_name.len()].copy_from_slice(font_name);
        name_table
    }

    fn build_hmtx_table(resource: &FontResource, num_glyphs: usize) -> Vec<u8> {
        let mut hmtx = Vec::with_capacity(num_glyphs * 4);
        for gid in 0..num_glyphs {
            let width = resource.glyph_width_by_gid(gid as u32);
            hmtx.extend_from_slice(&(width as i16).to_be_bytes());
            hmtx.extend_from_slice(&0i16.to_be_bytes());
        }
        hmtx
    }

    fn synthesize_naked_tables(
        tag: [u8; 4],
        outline_data: &[u8],
        resource: &FontResource,
        info: &CffInfo,
    ) -> Vec<([u8; 4], Vec<u8>)> {
        let mut tables = Vec::new();
        tables.push((tag, outline_data.to_vec()));

        let num_glyphs = info.num_glyphs;

        let mut head = vec![0u8; 54];
        head[0..4].copy_from_slice(&[0, 1, 0, 0]);
        head[12..16].copy_from_slice(&0x5F0F3CF5u32.to_be_bytes());
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        tables.push((*b"head", head));

        let mut hhea = vec![0u8; 36];
        hhea[0..4].copy_from_slice(&[0, 1, 0, 0]);
        hhea[34..36].copy_from_slice(&(num_glyphs as u16).to_be_bytes());
        tables.push((*b"hhea", hhea));

        let mut maxp = vec![0u8; 32];
        maxp[0..4].copy_from_slice(&[0, 0, 0x50, 0]);
        maxp[4..6].copy_from_slice(&(num_glyphs as u16).to_be_bytes());
        tables.push((*b"maxp", maxp));

        tables.push((*b"hmtx", Self::build_hmtx_table(resource, num_glyphs)));

        // Synthesize a minimal OS/2 table (Required for OpenType)
        let mut os2 = vec![0u8; 96];
        os2[0..2].copy_from_slice(&3u16.to_be_bytes());
        os2[64..66].copy_from_slice(&400u16.to_be_bytes());
        os2[66..68].copy_from_slice(&5u16.to_be_bytes());
        tables.push((*b"OS/2", os2));

        tables.push((*b"name", Self::build_name_table(resource)));

        // Synthesize a minimal post table (Version 3.0)
        let mut post = vec![0u8; 32];
        post[0..4].copy_from_slice(&[0, 0, 3, 0]); // version 3.0
        tables.push((*b"post", post));

        tables
    }

    fn wrap_naked_outline(
        tag: [u8; 4],
        outline_data: &[u8],
        resource: &FontResource,
    ) -> PdfResult<ReconstructedFont> {
        let info = if tag == *b"CFF " || tag == *b"CFF2" {
            Self::inspect_cff(outline_data).unwrap_or(CffInfo::empty())
        } else {
            CffInfo::empty()
        };

        let mut tables = Self::synthesize_naked_tables(tag, outline_data, resource, &info);

        // Synthesize a bridged cmap table
        let (cmap_data_opt, synthesized_cid_map) = Self::synthesize_bridged_cmap(
            resource,
            outline_data,
            info.sid_to_gid.as_ref(),
            info.name_to_gid.as_ref(),
            info.is_cid,
        );

        if let Some(cmap_data) = cmap_data_opt {
            tables.push((*b"cmap", cmap_data));
        }

        let sfnt_data = match Self::assemble_sfnt(b"OTTO", &tables) {
            Ok(new_data) => new_data,
            Err(e) => {
                log::error!(
                    "[RECONSTRUCT] SFNT assembly FAILED for {}: {:?}",
                    resource.base_font.as_str(),
                    e
                );
                outline_data.to_vec()
            }
        };

        let mut cid_to_gid_map = Self::build_cid_to_gid_map(&info);
        if !synthesized_cid_map.is_empty() {
            cid_to_gid_map = Some(synthesized_cid_map);
        }

        Ok(ReconstructedFont {
            data: sfnt_data,
            is_cid: info.is_cid,
            cid_to_gid_map,
            name_to_gid_map: info.name_to_gid,
            sid_to_gid_map: info.sid_to_gid,
            num_glyphs: Some(info.num_glyphs as u32),
        })
    }

    fn resolve_via_name_and_unicode(
        cid: u32,
        c: char,
        resource: &FontResource,
        discovered_map: Option<&BTreeMap<u32, u32>>,
        name_to_gid: Option<&BTreeMap<String, u32>>,
    ) -> Option<u32> {
        if let Some(nmap) = name_to_gid
            && let Some(glyph_name) = resource.encoding.as_ref().and_then(|e| e.map(&[cid as u8]))
        {
            let name = glyph_name.strip_prefix('/').unwrap_or(&glyph_name);
            if let Some(gid) = nmap.get(name).copied() {
                return Some(gid);
            }
            let sid_candidate = if let Some(stripped) = name.strip_prefix('c') {
                stripped.parse::<u32>().ok()
            } else if let Some(stripped) = name.strip_prefix("uni") {
                u32::from_str_radix(stripped, 16).ok()
            } else {
                None
            };
            if let Some(sid) = sid_candidate
                && let Some(map) = discovered_map
                && let Some(gid) = map.get(&sid).copied()
            {
                return Some(gid);
            }
        }
        if !resource.is_cjk()
            && let Some(nmap) = name_to_gid
        {
            if let Some(gid) = nmap.get(&c.to_string()) {
                return Some(*gid);
            }
            let agl_name = Self::unicode_to_agl_name(c);
            if let Some(gid) = nmap.get(agl_name) {
                return Some(*gid);
            }
            let hex_name = format!("uni{:04X}", c as u32);
            if let Some(gid) = nmap.get(&hex_name) {
                return Some(*gid);
            }
        }
        None
    }

    fn resolve_via_internal_cmap(
        cid: u32,
        c: char,
        discovered_map: Option<&BTreeMap<u32, u32>>,
        internal_unicode_map: &BTreeMap<u32, u32>,
        internal_code_map: &BTreeMap<u32, u32>,
        _is_cid_keyed: bool,
    ) -> Option<u32> {
        if let Some(map) = discovered_map
            && let Some(gid) = map.get(&cid).copied()
        {
            return Some(gid);
        }
        if let Some(&gid) = internal_unicode_map.get(&(c as u32))
            && gid != 0
        {
            return Some(gid);
        }
        if let Some(&gid) = internal_code_map.get(&cid)
            && gid != 0
        {
            return Some(gid);
        }
        None
    }

    fn resolve_final_fallback(
        cid: u32,
        c: char,
        actual_gid: Option<u32>,
        resource: &FontResource,
        info: &CffInfo,
    ) -> u32 {
        let mut gid_opt = actual_gid;
        if gid_opt.unwrap_or(0) == 0 && info.num_glyphs == 2 {
            log::debug!(
                "[RECONSTRUCT] Resolved '{}' (CID {}) -> GID 1 via Greedy-Tiny-Subset",
                c,
                cid
            );
            gid_opt = Some(1);
        }
        if gid_opt.is_none() {
            let is_identity = resource.cid_ordering.as_deref().is_none_or(|o| o == "Identity");
            if resource.is_cid_keyed && is_identity && !resource.is_cjk() && cid != 0 {
                gid_opt = Some(cid);
            }
        }
        if let Some(gid) = gid_opt {
            gid
        } else if resource.is_cid_keyed
            && (resource.cid_to_gid_map.is_some()
                || !resource.base_font.as_str().contains('+')
                || resource.is_cjk())
        {
            resource.to_gid(cid, None)
        } else {
            0
        }
    }

    fn parse_internal_cmaps(raw_data: &[u8]) -> (BTreeMap<u32, u32>, BTreeMap<u32, u32>) {
        let mut internal_unicode_map = BTreeMap::new();
        let mut internal_code_map = BTreeMap::new();
        if let Ok(face) = ttf_parser::Face::parse(raw_data, 0) {
            for table in face.tables().cmap.iter().flat_map(|t| t.subtables) {
                let is_unicode = table.is_unicode();
                table.codepoints(|cp| {
                    if let Some(gid) = table.glyph_index(cp) {
                        if is_unicode {
                            internal_unicode_map.insert(cp, gid.0 as u32);
                        } else {
                            internal_code_map.insert(cp, gid.0 as u32);
                        }
                    }
                });
            }
        }
        (internal_unicode_map, internal_code_map)
    }

    fn synthesize_bridged_cmap(
        resource: &FontResource,
        raw_data: &[u8],
        discovered_map: Option<&BTreeMap<u32, u32>>,
        name_to_gid: Option<&BTreeMap<String, u32>>,
        _is_cid: bool,
    ) -> (Option<Vec<u8>>, BTreeMap<u32, u32>) {
        let (internal_unicode_map, internal_code_map) = Self::parse_internal_cmaps(raw_data);

        let info = Self::inspect_cff(raw_data).unwrap_or(CffInfo::empty());
        let mut mappings = Vec::new();
        let default_map;
        let it: Box<dyn Iterator<Item = (String, u32)>> =
            if resource.unified_map.is_empty() && !resource.is_cid_keyed {
                default_map = (0..=255u32)
                    .map(|c| (String::from_utf8_lossy(&[c as u8]).to_string(), c))
                    .collect::<Vec<_>>();
                Box::new(default_map.into_iter())
            } else {
                Box::new(resource.unified_map.iter().map(|(s, &c)| (s.clone(), c)))
            };

        let mut cid_to_gid_map = discovered_map.cloned().unwrap_or_default();
        for (uni_str, cid) in it {
            let Some(c) = uni_str.chars().next() else {
                continue;
            };
            let mut actual_gid =
                Self::resolve_via_name_and_unicode(cid, c, resource, discovered_map, name_to_gid);
            if actual_gid.is_none() {
                actual_gid = Self::resolve_via_internal_cmap(
                    cid,
                    c,
                    discovered_map,
                    &internal_unicode_map,
                    &internal_code_map,
                    resource.is_cid_keyed,
                );
            }
            let final_gid = Self::resolve_final_fallback(cid, c, actual_gid, resource, &info);
            mappings.push((c as u32, final_gid));
            cid_to_gid_map.insert(cid, final_gid);
        }

        (Self::assemble_cmap_table(&mappings), cid_to_gid_map)
    }

    fn assemble_cmap_table(mappings: &[(u32, u32)]) -> Option<Vec<u8>> {
        if mappings.is_empty() {
            return None;
        }
        let mut m = mappings.to_vec();
        m.sort_by_key(|v| v.0);
        m.dedup_by_key(|v| v.0);

        let mut cmap = Vec::new();
        cmap.extend_from_slice(&0u16.to_be_bytes()); // version
        cmap.extend_from_slice(&1u16.to_be_bytes()); // numTables
        cmap.extend_from_slice(&3u16.to_be_bytes()); // Windows
        cmap.extend_from_slice(&10u16.to_be_bytes()); // UCS-4
        cmap.extend_from_slice(&12u32.to_be_bytes()); // offset

        let mut groups = Vec::new();
        let mut cur_start = m[0].0;
        let mut cur_gid = m[0].1;
        let mut cur_len = 1;
        for &(cv, gv) in m.iter().skip(1) {
            if cv == cur_start + cur_len && gv == cur_gid + cur_len {
                cur_len += 1;
            } else {
                groups.push((cur_start, cur_start + cur_len - 1, cur_gid));
                cur_start = cv;
                cur_gid = gv;
                cur_len = 1;
            }
        }
        groups.push((cur_start, cur_start + cur_len - 1, cur_gid));

        let sub_len = 16 + (groups.len() as u32) * 12;
        cmap.extend_from_slice(&12u16.to_be_bytes()); // Format 12
        cmap.extend_from_slice(&0u16.to_be_bytes());
        cmap.extend_from_slice(&sub_len.to_be_bytes());
        cmap.extend_from_slice(&0u32.to_be_bytes());
        cmap.extend_from_slice(&(groups.len() as u32).to_be_bytes());
        for (s, e, g) in groups {
            cmap.extend_from_slice(&s.to_be_bytes());
            cmap.extend_from_slice(&e.to_be_bytes());
            cmap.extend_from_slice(&g.to_be_bytes());
        }
        Some(cmap)
    }

    fn disassemble_sfnt(sfnt: &[u8]) -> PdfResult<DisassembledSfnt> {
        if sfnt.len() < 12 {
            return Err(PdfError::Internal("SFNT too short".into()));
        }

        let mut base_offset = 0;
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&sfnt[0..4]);

        if &magic == b"ttcf" {
            if sfnt.len() < 12 {
                return Err(PdfError::Internal("TTC header too short".into()));
            }
            let num_fonts = u32::from_be_bytes([sfnt[8], sfnt[9], sfnt[10], sfnt[11]]) as usize;
            if num_fonts == 0 {
                return Err(PdfError::Internal("TTC contains no fonts".into()));
            }
            base_offset = u32::from_be_bytes([sfnt[12], sfnt[13], sfnt[14], sfnt[15]]) as usize;
            if base_offset + 12 > sfnt.len() {
                return Err(PdfError::Internal("TTC offset out of bounds".into()));
            }
            magic.copy_from_slice(&sfnt[base_offset..base_offset + 4]);
        }

        let num_tables =
            u16::from_be_bytes([sfnt[base_offset + 4], sfnt[base_offset + 5]]) as usize;
        let mut tables = Vec::new();
        for i in 0..num_tables {
            let entry = base_offset + 12 + i * 16;
            if entry + 16 > sfnt.len() {
                break;
            }
            let mut tag = [0; 4];
            tag.copy_from_slice(&sfnt[entry..entry + 4]);
            let offset = u32::from_be_bytes([
                sfnt[entry + 8],
                sfnt[entry + 9],
                sfnt[entry + 10],
                sfnt[entry + 11],
            ]) as usize;
            let length = u32::from_be_bytes([
                sfnt[entry + 12],
                sfnt[entry + 13],
                sfnt[entry + 14],
                sfnt[entry + 15],
            ]) as usize;
            if offset + length <= sfnt.len() {
                tables.push((tag, sfnt[offset..offset + length].to_vec()));
            }
        }
        Ok(DisassembledSfnt { magic, tables })
    }

    fn assemble_sfnt(magic: &[u8; 4], tables: &[([u8; 4], Vec<u8>)]) -> PdfResult<Vec<u8>> {
        let mut output = Vec::new();
        output.extend_from_slice(magic);

        let mut tables = tables.to_vec();
        tables.sort_by_key(|t| t.0);
        output.extend_from_slice(&(tables.len() as u16).to_be_bytes());
        log::debug!("[RECONSTRUCT] SFNT: tables={}, sig={:02x?}", tables.len(), &output[0..4]);
        let search_range = (tables.len() as f64).log2().floor().exp2() as u16 * 16;
        output.extend_from_slice(&search_range.to_be_bytes());
        output.extend_from_slice(&((tables.len() as f64).log2().floor() as u16).to_be_bytes());
        output.extend_from_slice(&(tables.len() as u16 * 16 - search_range).to_be_bytes());
        let mut offset = 12 + tables.len() * 16;
        for (tag, data) in &tables {
            output.extend_from_slice(tag);
            output.extend_from_slice(&Self::calc_checksum(data).to_be_bytes());
            output.extend_from_slice(&(offset as u32).to_be_bytes());
            output.extend_from_slice(&(data.len() as u32).to_be_bytes());
            offset += (data.len() + 3) & !3;
        }
        for (_tag, data) in &tables {
            output.extend_from_slice(data);
            let padding = (4 - (data.len() % 4)) % 4;
            output.extend(std::iter::repeat_n(0, padding));
        }
        if let Some(h_off) = find_table_range(&output, b"head") {
            let adj = h_off.0 + 8;
            if adj + 4 <= output.len() {
                output[adj..adj + 4].copy_from_slice(&[0, 0, 0, 0]);
                let sum = 0xB1B0AFBAu32.wrapping_sub(Self::calc_checksum(&output));
                output[adj..adj + 4].copy_from_slice(&sum.to_be_bytes());
            }
        }
        log::debug!("[RECONSTRUCT] Final SFNT Header: {:02x?}", &output[0..16]);
        Ok(output)
    }

    fn calc_checksum(data: &[u8]) -> u32 {
        let mut sum: u32 = 0;
        let chunks = data.chunks_exact(4);
        let remainder = chunks.remainder();
        for chunk in chunks {
            sum = sum.wrapping_add(u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        if !remainder.is_empty() {
            let mut padded = [0u8; 4];
            padded[..remainder.len()].copy_from_slice(remainder);
            sum = sum.wrapping_add(u32::from_be_bytes(padded));
        }
        sum
    }

    fn patch_hmtx_direct(
        tables: &mut [([u8; 4], Vec<u8>)],
        resource: &FontResource,
        native_upem: u16,
    ) {
        if let Some(idx) = tables.iter().position(|(t, _)| t == b"hmtx") {
            let hmtx = &mut tables[idx].1;
            let n = hmtx.len() / 4;
            let scale = native_upem as f32 / 1000.0;

            for gid in 0..n {
                let w_pdf = resource.glyph_width_by_gid(gid as u32);
                let w_native = (w_pdf * scale) as i16;
                let off = gid * 4;
                if off + 2 <= hmtx.len() {
                    let b = w_native.to_be_bytes();
                    hmtx[off] = b[0];
                    hmtx[off + 1] = b[1];
                }
            }
        }
    }
}

pub struct CffInfo {
    pub num_glyphs: usize,
    pub sid_to_gid: Option<BTreeMap<u32, u32>>,
    pub name_to_gid: Option<BTreeMap<String, u32>>,
    pub is_cid: bool,
    pub string_index: Vec<String>,
}

impl CffInfo {
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

    pub fn inspect_cff(data: &[u8]) -> Result<CffInfo, Box<dyn std::error::Error>> {
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
        log::debug!("[RECONSTRUCT] Global Subrs INDEX at {}, count: {}", gsubr_pos, gsubr_count);

        let (cso, cso2, is_cid) = Self::parse_cff_top_dict(cff_data, top_dict_pos, tc);
        let ng = Self::determine_glyph_count(cff_data, cso);
        if let Some(o) = cso {
            log::debug!("[RECONSTRUCT] CharStrings INDEX at {}, count: {}", o, ng);
        }
        log::debug!("[RECONSTRUCT] CFF glyphs: {}, Top DICT: {}, is_cid: {}", ng, tc, is_cid);

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

    fn extract_cff_stream(data: &[u8]) -> Result<&[u8], Box<dyn std::error::Error>> {
        let is_sfnt =
            data.len() >= 4 && (data.starts_with(b"OTTO") || data.starts_with(&[0, 1, 0, 0]));
        if is_sfnt {
            if let Some((o, e)) = find_table_range(data, b"CFF ") {
                log::debug!("[RECONSTRUCT] Found CFF table at {}-{} (size: {})", o, e, e - o);
                Ok(&data[o..e])
            } else if let Some((o, e)) = find_table_range(data, b"CFF2") {
                log::debug!("[RECONSTRUCT] Found CFF2 table at {}-{} (size: {})", o, e, e - o);
                Ok(&data[o..e])
            } else {
                log::warn!("[RECONSTRUCT] CFF table not found in SFNT container");
                Err("CFF table not found in SFNT container".into())
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
                map.insert(gid as u32, gid as u32);
            }
        } else {
            // ISOAdobe fallback for simple fonts
            for gid in 1..std::cmp::min(num_glyphs, 229) {
                map.insert(gid as u32, gid as u32);
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
        use crate::font::cff_standard::CFF_STANDARD_STRINGS;

        for (&sid, &gid) in sid_map {
            let name = if sid <= CFF_LAST_STANDARD_SID {
                CFF_STANDARD_STRINGS[sid as usize].to_string()
            } else {
                let custom_idx = (sid - (CFF_LAST_STANDARD_SID + 1)) as usize;
                if let Some(item) = get_index_item(data, string_idx_pos, custom_idx) {
                    String::from_utf8_lossy(&item).to_string()
                } else {
                    format!("c{:03}", sid)
                }
            };
            nm.insert(name.clone(), gid);
            log::debug!("[RECONSTRUCT] Derived name: {} -> GID {}", name, gid);

            // Add Unicode alias for standard SIDs (e.g. "!" for "exclam")
            if sid <= CFF_LAST_STANDARD_SID {
                if let Some(c) = Self::standard_sid_to_unicode(sid) {
                    nm.insert(c.to_string(), gid);
                    log::debug!("[RECONSTRUCT] Added Unicode alias: {} -> GID {}", c, gid);
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
        log::debug!("[RECONSTRUCT] FDArray: offset {}", offset);
        if offset > 0 && offset < data.len() {
            let (_, count, _) = Self::parse_index_header(data, offset).unwrap_or((0, 0, 0));
            for i in 0..count {
                if let Some(fd) = get_index_item(data, offset, i.into()) {
                    let mut fdp = 0;
                    let mut fdops = Vec::new();
                    while fdp < fd.len() {
                        let b0 = fd[fdp];
                        if b0 <= 21 {
                            let mut op = b0 as u16;
                            fdp += 1;
                            if op == 12 && fdp < fd.len() {
                                op = (op << 8) | fd[fdp] as u16;
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
                    let mut op = b0 as u16;
                    dpos += 1;
                    if op == 12 && dpos < dd.len() {
                        op = (op << 8) | dd[dpos] as u16;
                        dpos += 1;
                    }
                    match op {
                        17 => cso = ops.last().copied().map(|v| v as usize),
                        18 => {
                            if ops.len() >= 2 {
                                let size = ops[ops.len() - 2] as usize;
                                let offset = ops[ops.len() - 1] as usize;
                                log::debug!(
                                    "[RECONSTRUCT] Private DICT: offset {}, size {}",
                                    offset,
                                    size
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
                map.insert(cid as u32, gid as u32);
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
                    data[cpos + 2] as u16
                } else {
                    u16::from_be_bytes([data[cpos + 2], data[cpos + 3]])
                };
                cpos += sz;
                for i in 0..=nl {
                    if (fc as u32 + i as u32) < 65536 {
                        let cid = fc as u32 + i as u32;
                        map.insert(cid, gid as u32);
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

fn skip_index(data: &[u8], pos: usize) -> usize {
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

fn get_index_item(data: &[u8], ip: usize, i: usize) -> Option<Vec<u8>> {
    let count = u16::from_be_bytes([data[ip], data[ip + 1]]) as usize;
    if i >= count {
        return None;
    }
    let os = data[ip + 2] as usize;
    let mut s = 0;
    let mut e = 0;
    let sp = ip + 3 + i * os;
    let ep = sp + os;
    for j in 0..os {
        s = (s << 8) | data[sp + j] as usize;
    }
    for j in 0..os {
        e = (e << 8) | data[ep + j] as usize;
    }
    let ds = ip + 3 + (count + 1) * os + s - 1;
    let de = ip + 3 + (count + 1) * os + e - 1;
    if de <= data.len() { Some(data[ds..de].to_vec()) } else { None }
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
        (u16::from_be_bytes([d[1], d[2]]) as i16 as i32, 3)
    } else if b0 == 29 {
        (i32::from_be_bytes([d[1], d[2], d[3], d[4]]), 5)
    } else if (32..=246).contains(&b0) {
        (b0 as i32 - 139, 1)
    } else if (247..=250).contains(&b0) {
        ((b0 as i32 - 247) * 256 + d[1] as i32 + 108, 2)
    } else if (251..=254).contains(&b0) {
        (-(b0 as i32 - 251) * 256 - d[1] as i32 - 108, 2)
    } else {
        (0, 1)
    }
}

fn find_table_range(s: &[u8], t: &[u8; 4]) -> Option<(usize, usize)> {
    if s.len() < 12 {
        return None;
    }
    let nt = u16::from_be_bytes([s[4], s[5]]) as usize;
    for i in 0..nt {
        let e = 12 + i * 16;
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

    #[test]
    fn test_cff2_wrapping() {
        let dummy_cff2 = vec![2, 0, 5, 1, 2, 3, 4, 5];
        let resource = FontResource::new_test();

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

    #[test]
    #[ignore]
    fn test_reconstructed_font_parsing() {
        let path = "exports/font-0003.otf";
        if let Ok(data) = std::fs::read(path) {
            println!("Testing font: {} ({} bytes)", path, data.len());
            match ttf_parser::Face::parse(&data, 0) {
                Ok(face) => {
                    println!("Success! Glyphs: {}", face.number_of_glyphs());
                    assert!(face.tables().cff.is_some() || face.tables().cff2.is_some());

                    // Try to draw GID 1
                    let mut builder = NoopBuilder;
                    let outline_res = face.outline_glyph(ttf_parser::GlyphId(1), &mut builder);
                    assert!(outline_res.is_some(), "GID 1 outline extraction FAILED");
                }
                Err(e) => {
                    panic!("Failed to parse reconstructed font: {e:?}");
                }
            }
        }
    }

    struct NoopBuilder;
    impl ttf_parser::OutlineBuilder for NoopBuilder {
        fn move_to(&mut self, _: f32, _: f32) {}
        fn line_to(&mut self, _: f32, _: f32) {}
        fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {}
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {}
        fn close(&mut self) {}
    }
}
