//! What loading settles once: resources and page attributes pushed down, and fonts
//! grouped so a `/ToUnicode` one has is shared.

use super::{BestToUnicodeMap, Document, FontGroupMap};
use crate::error::PdfError;
use crate::handle::DictHandle;
use crate::{Handle, Object, PdfName, PdfResult};
use std::collections::BTreeMap;

impl Document {
    /// Normalizes document resources at load-time (Phase 3).
    /// Group fonts by BaseFont and CIDSystemInfo to share ToUnicode mappings.
    pub fn normalize_resources(&mut self) {
        let (font_groups, best_to_unicode) = self.discover_font_groups();
        self.propagate_tounicode_mappings(font_groups, best_to_unicode);
    }

    /// Normalizes the page tree by pushing down inherited attributes (Phase 4).
    pub fn normalize_page_tree(&mut self) {
        let root_h = match self.get_pages_root() {
            Ok(h) => h,
            Err(_) => return,
        };

        let mut inherited = BTreeMap::new();
        let _ = self.push_down_attributes_recursive(root_h, &mut inherited, 0);
    }

    pub(super) fn process_leaf_page(
        &self,
        dict_h: Handle<BTreeMap<Handle<PdfName>, Object>>,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        local_inherited: BTreeMap<Handle<PdfName>, Object>,
    ) -> PdfResult<()> {
        let mut leaf_dict = dict.clone();
        for (key, val) in local_inherited {
            leaf_dict.entry(key).or_insert(val);
        }

        // Ensure CropBox and Rotate are explicitly set for Acrobat standardization
        let mb_key = self.arena.name("MediaBox");
        let cb_key = self.arena.name("CropBox");
        let rot_key = self.arena.name("Rotate");

        if !leaf_dict.contains_key(&cb_key)
            && let Some(mb_val) = leaf_dict.get(&mb_key)
        {
            leaf_dict.insert(cb_key, mb_val.clone());
        }
        leaf_dict.entry(rot_key).or_insert(Object::Integer(0));

        self.arena.set_dict(dict_h, leaf_dict);
        Ok(())
    }

    pub(super) fn process_pages_node(
        &self,
        dict_h: Handle<BTreeMap<Handle<PdfName>, Object>>,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        local_inherited: &mut BTreeMap<Handle<PdfName>, Object>,
        depth: usize,
    ) -> PdfResult<()> {
        let kids_key = self.arena.name("Kids");
        let kids_obj = dict
            .get(&kids_key)
            .ok_or_else(|| PdfError::violation("7.7.3.2", "Missing Kids in Pages node"))?;
        let ah = kids_obj
            .resolve(&self.arena)
            .as_array()
            .ok_or_else(|| PdfError::violation("7.7.3.2", "Invalid Kids array"))?;
        let kids = self
            .arena
            .get_array(ah)
            .ok_or_else(|| PdfError::internal("Invalid kids array handle"))?;
        for kid in kids {
            if let Some(kh) = kid.as_reference() {
                self.push_down_attributes_recursive(kh, local_inherited, depth + 1)?;
            }
        }

        let mut pages_dict = dict.clone();
        for attr in ["Resources", "MediaBox", "CropBox", "Rotate"] {
            pages_dict.remove(&self.arena.name(attr));
        }
        self.arena.set_dict(dict_h, pages_dict);
        Ok(())
    }

    #[allow(clippy::needless_pass_by_ref_mut)]
    pub(super) fn push_down_attributes_recursive(
        &self,
        node_h: Handle<Object>,
        inherited: &mut BTreeMap<Handle<PdfName>, Object>,
        depth: usize,
    ) -> PdfResult<()> {
        if depth > 32 {
            return Err(PdfError::DepthLimitExceeded(32));
        }

        let dict_h = self.resolve_to_dict(node_h)?;
        let dict = self.arena.get_dict(dict_h).ok_or_else(|| PdfError::internal("Invalid node"))?;

        let type_key = self.arena.name("Type");
        let node_type = dict
            .get(&type_key)
            .and_then(|o| o.resolve(&self.arena).as_name())
            .and_then(|h| self.arena.get_name(h));

        // Update inherited attributes for this level
        let attrs = ["Resources", "MediaBox", "CropBox", "Rotate"];
        let mut local_inherited = inherited.clone();
        for attr in attrs {
            let key = self.arena.name(attr);
            if let Some(val) = dict.get(&key) {
                local_inherited.insert(key, val.clone());
            }
        }

        if let Some(name) = &node_type
            && name.as_str() == "Page"
        {
            return self.process_leaf_page(dict_h, &dict, local_inherited);
        }

        if let Some(name) = &node_type
            && name.as_str() == "Pages"
        {
            return self.process_pages_node(dict_h, &dict, &mut local_inherited, depth);
        }

        Err(PdfError::violation("7.7.3", "Invalid node type in page tree"))
    }

    pub(super) fn discover_font_groups(&self) -> (FontGroupMap, BestToUnicodeMap) {
        let arena = &self.arena;
        let mut font_groups = BTreeMap::new();
        let mut best_to_unicode = BTreeMap::new();
        let mut best_to_unicode_count = BTreeMap::new();

        let type_key = arena.name("Type");
        let font_val = arena.name("Font");

        for h in arena.all_dict_handles() {
            // **Ask before copying.** This copied every dictionary in the arena to read
            // one entry of it — 720,603 of the 1,882,351 `get_dict` calls opening
            // `samples/intel_sdm.pdf`, and all but the font dictionaries discarded on the
            // next line (ROADMAP W-A4).
            let Some(t_h) = arena.dict_entry(h, type_key).and_then(|o| o.resolve(arena).as_name())
            else {
                continue;
            };
            if t_h != font_val {
                continue;
            }
            let Some(dict) = arena.get_dict(h) else { continue };
            self.group_one_font(
                h,
                &dict,
                &mut font_groups,
                &mut best_to_unicode,
                &mut best_to_unicode_count,
            );
        }
        (font_groups, best_to_unicode)
    }

    /// One font dictionary, into its group and that group's best `/ToUnicode`.
    ///
    /// Split out of [`Self::discover_font_groups`] when the loop above it grew past
    /// RR-15 Rule 1's fifty lines. **Only a CID font is grouped**: the group is keyed by
    /// base name and `CIDSystemInfo`, which a simple font has none of.
    pub(super) fn group_one_font(
        &self,
        handle: DictHandle,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        font_groups: &mut FontGroupMap,
        best_to_unicode: &mut BestToUnicodeMap,
        best_count: &mut BTreeMap<(String, String), usize>,
    ) {
        let arena = &self.arena;
        if !dict.contains_key(&arena.name("DescendantFonts")) {
            return;
        }
        let base_font = dict
            .get(&arena.name("BaseFont"))
            .and_then(|o| o.resolve(arena).as_name())
            .and_then(|h| arena.get_name_str(h))
            .unwrap_or_else(|| "Untitled".to_string());
        let key = (base_font, self.extract_csi_string(dict));
        font_groups.entry(key.clone()).or_default().push(handle);

        let Some(tu) = dict.get(&arena.name("ToUnicode")) else { return };
        let Ok(data) = self.decode_stream(&tu.resolve(arena)) else { return };
        let Ok(map) = crate::font::cmap::CMap::parse(&data) else { return };

        // The group keeps the richest `/ToUnicode` any of its members carries.
        let count = map.mappings.len();
        if best_count.get(&key).is_none_or(|best| count > *best) {
            best_count.insert(key.clone(), count);
            best_to_unicode.insert(key, tu.clone());
        }
    }

    pub(super) fn extract_csi_string(&self, dict: &BTreeMap<Handle<PdfName>, Object>) -> String {
        let arena = &self.arena;
        if let Some(df_obj) = dict.get(&arena.name("DescendantFonts"))
            && let Some(ah) = df_obj.resolve(arena).as_array()
            && let Some(arr) = arena.get_array(ah)
            && let Some(df_h) = arr.first().and_then(|o| o.resolve(arena).as_dict_handle())
            && let Some(df_dict) = arena.get_dict(df_h)
            && let Some(csi_obj) = df_dict.get(&arena.name("CIDSystemInfo"))
            && let Some(csi_h) = csi_obj.resolve(arena).as_dict_handle()
            && let Some(csi_dict) = arena.get_dict(csi_h)
        {
            let r = csi_dict
                .get(&arena.name("Registry"))
                .map(|o| o.resolve(arena))
                .as_ref()
                .and_then(|o| o.as_string())
                .map(|s| String::from_utf8_lossy(s).to_string())
                .unwrap_or_default();
            let o = csi_dict
                .get(&arena.name("Ordering"))
                .map(|o| o.resolve(arena))
                .as_ref()
                .and_then(|o| o.as_string())
                .map(|s| String::from_utf8_lossy(s).to_string())
                .unwrap_or_default();
            return format!("{r}-{o}");
        }
        String::new()
    }

    pub(super) fn propagate_tounicode_mappings(
        &self,
        font_groups: FontGroupMap,
        best_to_unicode: BestToUnicodeMap,
    ) {
        let arena = &self.arena;
        let to_unicode_key = arena.name("ToUnicode");
        for (key, fonts) in font_groups {
            if let Some(best_tu) = best_to_unicode.get(&key) {
                for font_h in fonts {
                    if let Some(mut dict) = arena.get_dict(font_h)
                        && !dict.contains_key(&to_unicode_key)
                    {
                        dict.insert(to_unicode_key, best_tu.clone());
                        arena.set_dict(font_h, dict);
                    }
                }
            }
        }
    }
}
