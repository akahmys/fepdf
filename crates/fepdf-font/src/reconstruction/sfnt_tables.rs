//! The SFNT tables a reconstructed program needs: `name`, `hmtx`, a `cmap` bridged from
//! the font's mapping, and the container assembled around them.

use super::{
    CffInfo, DisassembledSfnt, FontInfo, FontReconstructor, ReconstructedFont, find_table_range,
};
use crate::{FontError, FontResult};
use std::collections::BTreeMap;

impl FontReconstructor {
    pub(super) fn build_name_table(resource: &impl FontInfo) -> Vec<u8> {
        let font_name = resource.base_font().as_bytes();
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

    pub(super) fn build_hmtx_table(resource: &impl FontInfo, num_glyphs: usize) -> Vec<u8> {
        let mut hmtx = Vec::with_capacity(num_glyphs * 4);
        for gid in 0..num_glyphs {
            let width = resource.glyph_width_by_gid(gid as u32);
            hmtx.extend_from_slice(&(width as i16).to_be_bytes());
            hmtx.extend_from_slice(&0i16.to_be_bytes());
        }
        hmtx
    }

    pub(super) fn synthesize_naked_tables(
        tag: [u8; 4],
        outline_data: &[u8],
        resource: &impl FontInfo,
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

    pub(super) fn wrap_naked_outline(
        tag: [u8; 4],
        outline_data: &[u8],
        resource: &impl FontInfo,
    ) -> FontResult<ReconstructedFont> {
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
                // `debug!`, not a warning, and not a `Decision`. This is a failure of
                // *this engine's* SFNT assembler, not a conclusion about the file: the
                // font data arrived intact and we could not rebuild a container round it.
                //
                // The document-level consequence — a font program that will not parse —
                // is already recorded upstream, because the raw outline returned here is
                // a naked CFF that `populate_u2g_from_data` then fails to read, raising
                // the 9.9 `Violation` "the embedded program for X did not parse".
                //
                // Never fired on either corpus: 524 files, zero. Kept as a diagnostic
                // rather than deleted because if it ever does fire it is a bug here, and
                // the message names the font it happened to.
                log::debug!(
                    "[RECONSTRUCT] SFNT assembly failed for {}, falling back to the raw \
                     outline: {:?}",
                    resource.base_font(),
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

    pub(super) fn resolve_via_name_and_unicode(
        cid: u32,
        c: char,
        resource: &impl FontInfo,
        discovered_map: Option<&BTreeMap<u32, u32>>,
        name_to_gid: Option<&BTreeMap<String, u32>>,
    ) -> Option<u32> {
        if let Some(nmap) = name_to_gid
            && let Some(glyph_name) = resource.encoding().and_then(|e| e.map(&[cid as u8]))
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

    pub(super) fn resolve_via_internal_cmap(
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

    pub(super) fn resolve_final_fallback(
        cid: u32,
        c: char,
        actual_gid: Option<u32>,
        resource: &impl FontInfo,
        info: &CffInfo,
    ) -> u32 {
        let mut gid_opt = actual_gid;
        if gid_opt.unwrap_or(0) == 0 && info.num_glyphs == 2 {
            log::debug!("[RECONSTRUCT] Resolved '{c}' (CID {cid}) -> GID 1 via Greedy-Tiny-Subset");
            gid_opt = Some(1);
        }
        if gid_opt.is_none() {
            let is_identity = resource.cid_ordering().is_none_or(|o| o == "Identity");
            if resource.is_cid_keyed() && is_identity && !resource.is_cjk() && cid != 0 {
                gid_opt = Some(cid);
            }
        }
        if let Some(gid) = gid_opt {
            gid
        } else if resource.is_cid_keyed()
            && (resource.cid_to_gid_map().is_some()
                || crate::subset::subset_tag(resource.base_font()).is_none()
                || resource.is_cjk())
        {
            resource.to_gid_hint(cid, None)
        } else {
            0
        }
    }

    pub(super) fn parse_internal_cmaps(
        raw_data: &[u8],
    ) -> (BTreeMap<u32, u32>, BTreeMap<u32, u32>) {
        let mut internal_unicode_map = BTreeMap::new();
        let mut internal_code_map = BTreeMap::new();
        if let Ok(face) = ttf_parser::Face::parse(raw_data, 0) {
            for table in face.tables().cmap.iter().flat_map(|t| t.subtables) {
                let is_unicode = table.is_unicode();
                table.codepoints(|cp| {
                    if let Some(gid) = table.glyph_index(cp) {
                        if is_unicode {
                            internal_unicode_map.insert(cp, u32::from(gid.0));
                        } else {
                            internal_code_map.insert(cp, u32::from(gid.0));
                        }
                    }
                });
            }
        }
        (internal_unicode_map, internal_code_map)
    }

    pub(super) fn synthesize_bridged_cmap(
        resource: &impl FontInfo,
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
            if resource.unified_map().is_empty() && !resource.is_cid_keyed() {
                default_map = (0..=255u32)
                    .map(|c| (String::from_utf8_lossy(&[c as u8]).to_string(), c))
                    .collect::<Vec<_>>();
                Box::new(default_map.into_iter())
            } else {
                Box::new(resource.unified_map().iter().map(|(s, &c)| (s.clone(), c)))
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
                    resource.is_cid_keyed(),
                );
            }
            let final_gid = Self::resolve_final_fallback(cid, c, actual_gid, resource, &info);
            mappings.push((c as u32, final_gid));
            cid_to_gid_map.insert(cid, final_gid);
        }

        (Self::assemble_cmap_table(&mappings), cid_to_gid_map)
    }

    pub(super) fn assemble_cmap_table(mappings: &[(u32, u32)]) -> Option<Vec<u8>> {
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

    pub(crate) fn disassemble_sfnt(sfnt: &[u8]) -> FontResult<DisassembledSfnt> {
        if sfnt.len() < 12 {
            return Err(FontError::Internal("SFNT too short".into()));
        }

        let mut base_offset = 0;
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&sfnt[0..4]);

        if &magic == b"ttcf" {
            // A collection's header runs to 16 bytes, the first font's offset in 12..16.
            // This checked for 12, and a 15-byte one indexed past its end (ROADMAP Z-1).
            if sfnt.len() < 16 {
                return Err(FontError::Internal("TTC header too short".into()));
            }
            let num_fonts = u32::from_be_bytes([sfnt[8], sfnt[9], sfnt[10], sfnt[11]]) as usize;
            if num_fonts == 0 {
                return Err(FontError::Internal("TTC contains no fonts".into()));
            }
            base_offset = u32::from_be_bytes([sfnt[12], sfnt[13], sfnt[14], sfnt[15]]) as usize;
            if base_offset + 12 > sfnt.len() {
                return Err(FontError::Internal("TTC offset out of bounds".into()));
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

    pub(crate) fn assemble_sfnt(
        magic: &[u8; 4],
        tables: &[([u8; 4], Vec<u8>)],
    ) -> FontResult<Vec<u8>> {
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

    pub(super) fn calc_checksum(data: &[u8]) -> u32 {
        let mut sum: u32 = 0;
        let (chunks, remainder) = data.as_chunks::<4>();
        for &chunk in chunks {
            sum = sum.wrapping_add(u32::from_be_bytes(chunk));
        }
        if !remainder.is_empty() {
            let mut padded = [0u8; 4];
            padded[..remainder.len()].copy_from_slice(remainder);
            sum = sum.wrapping_add(u32::from_be_bytes(padded));
        }
        sum
    }

    pub(super) fn patch_hmtx_direct(
        tables: &mut [([u8; 4], Vec<u8>)],
        resource: &impl FontInfo,
        native_upem: u16,
    ) {
        if let Some(idx) = tables.iter().position(|(t, _)| t == b"hmtx") {
            let hmtx = &mut tables[idx].1;
            let n = hmtx.len() / 4;
            let scale = f32::from(native_upem) / 1000.0;

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
