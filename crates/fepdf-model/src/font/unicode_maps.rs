//! The Unicode a font's glyphs stand for, gathered from `/ToUnicode`, the unified map and
//! the program itself.

use super::{EmbeddedFormat, FontResource};
use crate::Document;
use std::collections::BTreeMap;

impl FontResource {
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

    pub(super) fn populate_u2g_from_data(
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

    pub(super) fn populate_u2g_from_tounicode(&self, u2g: &mut BTreeMap<char, u32>) {
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

    pub(super) fn populate_u2g_from_unified(&self, u2g: &mut BTreeMap<char, u32>) {
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

    pub(super) fn map_cmap_codepoints(
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

    pub(super) fn populate_u2g_from_font_file(&mut self, u2g: &mut BTreeMap<char, u32>) {
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
}
