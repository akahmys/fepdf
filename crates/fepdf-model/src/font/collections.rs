//! Adobe's character collections, and the maps a font's Unicode is rescued and generated
//! from.

use super::FontResource;
use super::metrics::FontCategory;
use fepdf_font::{cmap, rescue};
use std::collections::BTreeMap;

impl FontResource {
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
    pub(super) fn init_collection_map(&mut self) -> Option<crate::interpretation::Decision> {
        let declined = self.load_cid_collection();
        self.build_unicode_to_gid();
        declined
    }

    /// The character collection `/CIDSystemInfo` names, if it names one.
    ///
    /// `Identity` is not one: it is the file saying the codes are the font's own glyph
    /// order (9.7.4.2), which is a statement about indexing and not about characters.
    pub(super) fn declared_collection(&self) -> Option<String> {
        self.cid_ordering.as_ref().filter(|o| !o.eq_ignore_ascii_case("Identity")).cloned()
    }

    pub(super) fn load_cid_collection(&mut self) -> Option<crate::interpretation::Decision> {
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
    pub(super) fn decline_collection(
        &self,
        ordering: &str,
    ) -> Option<crate::interpretation::Decision> {
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
    pub(super) fn name_suggests_japanese(&self) -> bool {
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
    pub(super) fn load_collection(&mut self, registry: &str, ordering: &str) -> bool {
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

    pub(super) fn format_cid_chars(cmap: &mut String, gid_to_uni: &[(u32, String)]) {
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

    /// Returns the vertical metrics for a CID: `(w1_y, v_x, v_y)`.
    ///
    /// `w1_y` is the vertical advance, natively negative. `(v_x, v_y)` positions the
    /// glyph origin relative to the horizontal one.
    ///
    /// **The default comes from `/DW2` when the font declares one** (9.7.4.3). It was
    /// `(-1000, w0/2, 880)` written into this function until 2026-09-06 — which is the
    /// standard's *default* `[880 -1000]`, so a font that said nothing was laid out
    /// correctly and a font that said something else was laid out as though it had not.
    /// `/DW` beside it had been read from the file all along.
    ///
    /// `v_x` is never declared: 9.7.4.3 fixes it at half the glyph's horizontal width.
    pub fn glyph_vertical_metrics(&self, cid: u32) -> (f32, f32, f32) {
        if let Some(&metrics) = self.vertical_widths.get(&cid) {
            return metrics;
        }
        let w0 = *self.widths.get(&cid).unwrap_or(&self.default_width);
        let (v_y, w1_y) = self.default_vertical;
        (w1_y, w0 / 2.0, v_y)
    }
}
