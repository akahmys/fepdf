//! Which glyph of a substituted face draws a code the font does not map, scored among
//! candidates.

use super::{FontResource, TraceContext, system_fallback_gid};
use fepdf_font::agl;

impl FontResource {
    /// Resolves a character identifier (CID) to a physical Glyph ID (GID).
    ///
    /// This method follows a prioritized resolution chain:
    /// 1. Unicode-to-GID (CMap)
    /// 2. Glyph Name to GID (via Encoding and reconstruction map)
    /// 3. CFF SID fallback (for production names like cXXX)
    /// 4. Direct CID-to-GID mapping
    pub(super) fn score_source_intent(
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
    pub(super) fn score_candidate(
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

    pub(super) fn resolve_gid_font(&self, hint: Option<char>) -> Option<u32> {
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

    pub(super) fn gather_candidates(
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

    pub(super) fn resolve_fallback_gid(
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
    pub(super) fn apply_threshold_and_fallback(
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

    pub(super) fn find_best_candidate(
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

    pub(super) fn is_suspicious_hint(&self, hint: Option<char>) -> bool {
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

    pub(super) fn check_immediate_resolve(
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

    pub(super) fn resolve_name_from_encoding(&self, cid: u32) -> Option<(String, Option<char>)> {
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
}
