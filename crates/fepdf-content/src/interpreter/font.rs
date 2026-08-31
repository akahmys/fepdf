use crate::RenderBackend;
use crate::interpreter::Interpreter;
use fepdf_model::font::FontResource;
use fepdf_model::{Handle, Object, PdfName, PdfResult};
use std::sync::Arc;

impl Interpreter<'_> {
    pub(crate) fn resolve_font_resource(&mut self, name: &PdfName) -> PdfResult<Arc<FontResource>> {
        if name.as_str() == "Fallback-Sans" {
            let res = FontResource::load_fallback(
                fepdf_model::font::FallbackFontType::SansSerif,
                self.doc,
            )?;
            return Ok(Arc::new(res));
        }

        let entry =
            self.find_resource(&self.doc.arena().intern_name(PdfName::new("Font")), name)?;
        let h =
            entry.as_reference().unwrap_or_else(|| self.doc.arena().alloc_object(entry.clone()));
        self.get_font(h, Some(name))
    }

    pub(crate) fn get_font(
        &mut self,
        h: Handle<Object>,
        res_name: Option<&PdfName>,
    ) -> PdfResult<Arc<FontResource>> {
        let res = self.doc.get_font(h)?;

        // Resolve a unique name for the backend to prevent subset collisions
        let default_name = format!("Font_{}", h.index());
        let name = res_name.map_or_else(
            || self.font_name_map.get(&h).cloned().unwrap_or(default_name),
            |n| n.as_str().to_string(),
        );
        let backend_name = format!("{}_{}", name, h.index());

        if !self.defined_fonts.contains(backend_name.as_str()) {
            let mut data = res.reconstructed_data.clone().or_else(|| res.data.clone());

            // Check if the font data is in a format supported by the renderer (SFNT).
            // Raw Type 1 (PFB/PFA) is not supported and must be replaced by fallback font data.
            let is_sfnt = data.as_ref().is_some_and(|d| {
                let sig_match = d.len() >= 4
                    && (d.starts_with(b"OTTO")
                        || d.starts_with(&[0, 1, 0, 0])
                        || d.starts_with(b"true"));
                log::debug!(
                    "[SDK] Font {backend_name} data size: {}, is_sfnt: {}",
                    d.len(),
                    sig_match
                );
                sig_match
            });

            if !is_sfnt {
                // Deliberately silent. This warned "Font X is not SFNT, using fallback",
                // and measurement said it fired **469 times across three of the nine
                // conforming samples** — 423 of them on `fugaku.pdf`, whose 72 fonts are
                // all Type 3. A Type 3 font (9.6.5) has no font program at all, so it can
                // never be SFNT; the rest were fonts with `/FontFile` absent, where
                // substituting a system font is what 9.8 asks for. Neither is a departure.
                //
                // The departure that *is* real — a font that embeds a program in no
                // recognised format — is already recorded as a 9.9 `Violation` in
                // `fepdf-model/src/font/mod.rs`, gated on the font actually embedding
                // something. This site was a second, wronger copy of that test, and
                // converting it to a `Decision` would have put 469 false departures on
                // clean files: ADR-0008's mistake, made again.
                let fallback_type =
                    res.fallback_type.unwrap_or(fepdf_model::font::FallbackFontType::Default);
                data = self.doc.system_fonts.get(&fallback_type).cloned();
            }

            self.backend.define_font(
                backend_name.as_str(),
                Some(res.base_font.as_str()),
                data,
                None,
                res.cid_to_gid_map.clone(),
                res.fallback_type.unwrap_or(fepdf_model::font::FallbackFontType::Default),
                res.is_cid_keyed,
            );
            self.defined_fonts.insert(backend_name.clone());
        }

        self.backend.set_font(backend_name.as_str());
        Ok(res)
    }
}
