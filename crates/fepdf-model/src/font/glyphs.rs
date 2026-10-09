//! A glyph asked for by code: its Unicode, its width, its CID and its glyph index.

use super::metrics::FontCategory;
use super::{FontResource, TraceContext, UnicodeSource, resolve_mapping};

impl FontResource {
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
    pub(super) fn finish_unicode(
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
        if let Some(w) = self.cid_width(cid) {
            return w;
        }
        if self.widths.is_empty()
            && self.width_ranges.is_empty()
            && (self.default_width == 1000.0 || self.default_width == 0.0)
        {
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
        if let &[high, low] = code {
            return (u32::from(high) << 8) | u32::from(low);
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
        match *code {
            [high, low] => (u32::from(high) << 8) | u32::from(low),
            [one] => u32::from(one),
            _ => 0,
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

    /// Returns true if the font is embedded in the PDF and has been successfully reconstructed for rendering.
    pub fn is_embedded(&self) -> bool {
        self.reconstructed_data.is_some()
    }

    /// Advance width for a glyph index, read from the font program.
    pub fn get_physical_width(&self, gid: u32) -> f32 {
        self.physical_widths.get(&gid).copied().unwrap_or(0.0)
    }
}
