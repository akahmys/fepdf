use crate::interpreter::Interpreter;
use fepdf_model::font::FontResource;
use fepdf_model::{Handle, Object, PdfError, PdfName, PdfResult};
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
        let cached = self.doc.font_cache.read().get(&h).cloned();
        let res = if let Some(res) = cached {
            res
        } else {
            let arena = self.doc.arena();
            let font_obj = self.doc.resolve(&h)?;

            let res_raw = if let Object::Dictionary(dfh) = font_obj {
                let dict = arena
                    .get_dict(dfh)
                    .ok_or_else(|| PdfError::Other("Invalid font dictionary".into()))?;
                let mut initial_res = FontResource::load(&dict, self.doc)?;

                // Handle Type0 DescendantFonts
                if initial_res.subtype.as_str() == "Type0"
                    && let Some(desc_fonts_obj) = dict.get(&arena.name("DescendantFonts"))
                    && let Object::Array(ah) = desc_fonts_obj.resolve(arena)
                    && let Some(arr) = arena.get_array(ah)
                    && let Some(desc_font) = arr.first()
                    && let Object::Dictionary(dfh) = desc_font.resolve(arena)
                    && let Some(df_dict) = arena.get_dict(dfh)
                {
                    let mut desc_res = FontResource::load(&df_dict, self.doc)?;
                    // Propagate Encoding, ToUnicode, and WMode from Type0 parent
                    desc_res.encoding.clone_from(&initial_res.encoding);
                    desc_res.wmode = initial_res.wmode;
                    if desc_res.to_unicode.is_none() {
                        desc_res.to_unicode.clone_from(&initial_res.to_unicode);
                    }
                    // RE-POPULATE after propagating ToUnicode/Encoding
                    desc_res.build_unified_map();
                    desc_res.populate_embedded_unicode_map(self.doc);

                    // CRITICAL: Re-trigger reconstruction after unified_map is populated with inherited resources
                    let _ = desc_res.perform_reconstruction();

                    initial_res = desc_res;
                }
                Arc::new(initial_res)
            } else {
                return Err(PdfError::Other("Font dictionary not found".into()));
            };

            self.doc.font_cache.write().insert(h, Arc::clone(&res_raw));
            res_raw
        };

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
                let fallback_type =
                    res.fallback_type.unwrap_or(fepdf_model::font::FallbackFontType::Default);
                log::warn!(
                    "[SDK] Font {backend_name} is not SFNT, using fallback data for type {fallback_type:?}"
                );
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
