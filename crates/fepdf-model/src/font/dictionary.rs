//! A font dictionary read: its subtype, name, matrix, `/ToUnicode`, encoding, CID system
//! and descendant.

use super::metrics::FontMetrics;
use super::{DescendantResult, FontResource, loader};
use crate::handle::Handle;
use crate::{Document, Object, PdfArena, PdfError, PdfName, PdfResult};
use fepdf_font::cmap;
use std::collections::BTreeMap;
use std::sync::Arc;

impl FontResource {
    pub(super) fn extract_subtype(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> PdfResult<PdfName> {
        let subtype_name = dict
            .get(&arena.name("Subtype"))
            .and_then(|o| o.resolve(arena).as_name())
            .and_then(|h| arena.get_name(h));

        if subtype_name.is_none() {
            let keys: Vec<String> = dict
                .keys()
                .filter_map(|k| arena.get_name(*k).map(|n| n.as_str().to_string()))
                .collect();
            decisions.push(crate::interpretation::Decision::violation(
                "9.6.2",
                format!("font dictionary has no /Subtype; it holds {keys:?}"),
                "rejected the font resource",
            ));
        }

        subtype_name.ok_or_else(|| PdfError::violation("9.5", "Missing font subtype"))
    }

    pub(super) fn extract_base_font(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
    ) -> PdfName {
        dict.get(&arena.name("BaseFont"))
            .and_then(|o| o.resolve(arena).as_name())
            .and_then(|h| arena.get_name(h))
            .unwrap_or_else(|| PdfName::new("Untitled"))
    }

    pub(super) fn parse_char_procs(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
    ) -> Option<BTreeMap<String, Handle<Object>>> {
        let cp_key = arena.name("CharProcs");
        if let Some(cp_obj) = dict.get(&cp_key) {
            let cp_resolved = cp_obj.resolve(arena);
            if let Object::Dictionary(dfh) = cp_resolved
                && let Some(cp_dict) = arena.get_dict(dfh)
            {
                let mut map = BTreeMap::new();
                for (name_h, obj) in cp_dict {
                    if let Some(name) = arena.get_name(name_h)
                        && let Some(h) = obj.as_reference()
                    {
                        map.insert(name.as_str().to_string(), h);
                    }
                }
                return Some(map);
            }
        }
        None
    }

    pub(super) fn parse_font_matrix(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
    ) -> Option<[f32; 6]> {
        let fm_key = arena.name("FontMatrix");
        if let Some(fm_obj) = dict.get(&fm_key)
            && let Object::Array(ah) = fm_obj.resolve(arena)
            && let Some(arr) = arena.get_array(ah)
            && arr.len() == 6
        {
            let mut matrix = [0.0; 6];
            for (i, item) in arr.iter().enumerate() {
                matrix[i] = item.as_f64().unwrap_or(0.0) as f32;
            }
            Some(matrix)
        } else {
            None
        }
    }

    pub(super) fn parse_to_unicode(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        doc: &Document,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<cmap::CMap> {
        let arena = doc.arena();
        let tu_obj = dict.get(&arena.name("ToUnicode"))?;
        let base_font = if let Some(h) = dict.get(&arena.name("BaseFont")).and_then(|o| o.as_name())
        {
            arena
                .get_name(h)
                .map(|n| n.as_str().to_string())
                .unwrap_or_else(|| "Unknown".to_string())
        } else {
            "Unknown".to_string()
        };
        log::debug!("[FONT] Font {base_font} has ToUnicode obj: {tu_obj:?}");
        Self::try_load_cmap(doc, &tu_obj.resolve(arena), "ToUnicode", decisions)
    }

    pub(super) fn try_load_cmap(
        doc: &Document,
        obj: &Object,
        context: &str,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<cmap::CMap> {
        match doc.decode_stream(obj) {
            Ok(data) => match cmap::CMap::parse(&data) {
                Ok(m) => Some(m),
                Err(e) => {
                    decisions.push(crate::interpretation::Decision::violation(
                        "9.10.3",
                        format!("the {context} CMap did not parse: {e:?}"),
                        "left the font without that mapping",
                    ));
                    None
                }
            },
            Err(e) => {
                decisions.push(crate::interpretation::Decision::violation(
                    "9.10.3",
                    format!("the {context} CMap stream did not decode: {e:?}"),
                    "left the font without that mapping",
                ));
                None
            }
        }
    }

    pub(super) fn detect_legacy_distiller(to_unicode: &Option<cmap::CMap>) -> bool {
        let Some(tu) = to_unicode else { return false };
        tu.mappings.iter().any(|(code_vec, uni_str)| {
            code_vec.len() == 1
                && code_vec[0] >= 0x20
                && code_vec[0] <= 0x7E
                && uni_str.chars().any(|c| (c as u32) > 0xFF)
        })
    }

    pub(super) fn parse_encoding(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        doc: &Document,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<cmap::CMap> {
        let arena = doc.arena();
        let enc_obj = dict.get(&arena.name("Encoding"))?;
        let enc = enc_obj.resolve(arena);
        match enc {
            Object::Name(h) => {
                let name = arena.get_name(h)?;
                let name_str = name.as_str();
                // An Annex D base encoding is asked for first, because it is not a CMap
                // and the CMap loader searches a collection that has never held one.
                // Reaching `load_named` with `/WinAnsiEncoding` returned `None` and left
                // the font with no encoding at all: 36,914 glyphs of `intel_sdm.pdf`.
                fepdf_font::annex_d::base_encoding(name_str)
                    .or_else(|| cmap::CMap::load_named(name_str))
                    .or_else(|| match name_str {
                        "Identity-H" => Some(cmap::CMap::identity_h()),
                        "Identity-V" => Some(cmap::CMap::identity_v()),
                        "90ms-RKSJ-H" => Some(cmap::CMap::rksj_h()),
                        "UniJIS-UTF16-H" => Some(cmap::CMap::unijis_h()),
                        _ => {
                            Self::record_unknown_encoding(name_str, decisions);
                            None
                        }
                    })
            }
            Object::Stream(_, _) => Self::try_load_cmap(doc, &enc, "Encoding", decisions),
            Object::Dictionary(h) => Self::parse_encoding_dict(h, arena, decisions),
            _ => None,
        }
    }

    /// Says that an `/Encoding` name resolved to nothing, and which kind of nothing.
    ///
    /// **A name this engine does not carry and a name that means nothing are different
    /// failures**, and both used to produce the same silence: a font with no encoding,
    /// and every code above `U+007E` unnamed with no record of why. `/WinAnsiEncoding`
    /// was the first kind for the whole life of this code.
    pub(super) fn record_unknown_encoding(
        name: &str,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) {
        let (clause, what, took) = if fepdf_font::annex_d::is_base_encoding_name(name) {
            (
                "D.2",
                format!("/{name} is a base encoding this engine does not carry"),
                "left the font without one; codes above U+007E cannot be named",
            )
        } else {
            (
                "9.6.6.1",
                format!("/{name} names neither a CMap nor a base encoding"),
                "left the font without one; codes above U+007E cannot be named",
            )
        };
        decisions.push(crate::interpretation::Decision::violation(clause, what, took));
    }

    pub(super) fn parse_encoding_dict(
        h: Handle<BTreeMap<Handle<PdfName>, Object>>,
        arena: &PdfArena,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<cmap::CMap> {
        let enc_dict = arena.get_dict(h)?;
        let mut cmap = cmap::CMap::default();

        if let Some(base_name) = enc_dict
            .get(&arena.name("BaseEncoding"))
            .and_then(|o: &Object| o.resolve(arena).as_name())
            .and_then(|h: Handle<PdfName>| arena.get_name(h))
        {
            let name_str: String = base_name.as_str().to_string();
            if let Some(base_cmap) = fepdf_font::annex_d::base_encoding(&name_str)
                .or_else(|| cmap::CMap::load_named(&name_str))
            {
                cmap = base_cmap;
            } else {
                Self::record_unknown_encoding(&name_str, decisions);
            }
        }

        if let Some(Object::Array(ah)) =
            enc_dict.get(&arena.name("Differences")).map(|o: &Object| o.resolve(arena))
            && let Some(arr) = arena.get_array(ah)
        {
            let mut new_mappings = (*cmap.mappings).clone();
            let mut current_code = 0u32;
            for item in arr {
                let resolved: Object = item.resolve(arena);
                match resolved {
                    Object::Integer(code) => current_code = code as u32,
                    Object::Name(name_h) => {
                        if let Some(glyph_name) = arena.get_name(name_h) {
                            let glyph_name_str: String = glyph_name.as_str().to_string();
                            new_mappings
                                .insert(vec![current_code as u8], format!("/{glyph_name_str}"));
                            current_code += 1;
                        }
                    }
                    _ => {}
                }
            }
            cmap.mappings = Arc::new(new_mappings);
        }
        Some(cmap)
    }

    /// `/Ordering` and `/Registry` of `dict`'s `/CIDSystemInfo`, if it carries one.
    pub(super) fn read_csi(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
    ) -> (Option<String>, Option<String>) {
        let csi = dict
            .get(&arena.name("CIDSystemInfo"))
            .and_then(|o| o.resolve(arena).as_dict_handle())
            .and_then(|h| arena.get_dict(h));
        Self::parse_csi_info(csi.as_ref(), arena)
    }

    /// `/Ordering` and `/Registry` of a `/CIDSystemInfo` dictionary (9.7.3, Table 114).
    ///
    /// **Both are strings there, and this asked for names.** A name is a different object
    /// type (7.3.5), so `as_name` answered `None` for every conforming file: measured over
    /// both corpora, 116 of 116 Type0 fonts declare the pair and the engine read `None`
    /// for all 116. Everything downstream that asks which character collection a font
    /// belongs to — [`FontResource::is_cjk`], [`FontResource::init_collection_map`],
    /// `resolve_gid`'s identity fallback — was therefore deciding from the `/BaseFont`
    /// name alone while the file said so outright.
    ///
    /// A name is still accepted. Nothing in either corpus writes one, so this is not a
    /// measured need; it is what the code already did, and narrowing it would be a
    /// behavioural change with no document behind it.
    pub(super) fn parse_csi_info(
        csi_dict: Option<&BTreeMap<Handle<PdfName>, Object>>,
        arena: &PdfArena,
    ) -> (Option<String>, Option<String>) {
        let read = |d: &BTreeMap<Handle<PdfName>, Object>, key: &str| -> Option<String> {
            let value = d.get(&arena.name(key))?.resolve(arena);
            // `as_string` covers both the literal and the hexadecimal form (7.3.4);
            // `fy05.pdf` writes literals and `intel_sdm.pdf` writes hex.
            if let Some(bytes) = value.as_string() {
                return Some(String::from_utf8_lossy(bytes).to_string());
            }
            value.as_name().and_then(|n| arena.get_name(n)).map(|n| n.as_str().to_string())
        };
        csi_dict.map_or((None, None), |d| (read(d, "Ordering"), read(d, "Registry")))
    }

    pub(super) fn extract_descendant_font_data(
        df_dict: &BTreeMap<Handle<PdfName>, Object>,
        font_resource: &Option<Arc<FontResource>>,
        doc: &Document,
    ) -> Option<loader::FontData> {
        let arena = doc.arena();
        font_resource
            .as_ref()
            .and_then(|fr| {
                fr.data.as_ref().or(fr.reconstructed_data.as_ref()).map(|d| loader::FontData {
                    data: d.to_vec(),
                    length1: fr.length1,
                    length2: fr.length2,
                    length3: fr.length3,
                })
            })
            .or_else(|| {
                if let Some(fd_obj) = df_dict.get(&arena.name("FontDescriptor")) {
                    loader::FontLoader::extract_data(fd_obj, doc, Some(df_dict))
                } else {
                    None
                }
            })
    }

    pub(super) fn get_descendant_font_obj(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<Object> {
        let df_obj = dict.get(&arena.name("DescendantFonts"))?;
        let df_resolved = df_obj.resolve(arena);
        let df_array_h = df_resolved.as_array()?;
        let df_array = arena.get_array(df_array_h)?;
        if let Some(first) = df_array.first() {
            Some(first.clone())
        } else {
            decisions.push(crate::interpretation::Decision::violation(
                "9.7.4",
                "a Type0 font's /DescendantFonts array is empty",
                "left the font without a descendant",
            ));
            None
        }
    }

    pub(super) fn extract_font_file_handle(
        df_dict: &BTreeMap<Handle<PdfName>, Object>,
        arena: &PdfArena,
    ) -> Option<Handle<Object>> {
        let fd_obj = df_dict.get(&arena.name("FontDescriptor"))?;
        let fd_dict = fd_obj.resolve(arena).as_dict_handle().and_then(|dh| arena.get_dict(dh))?;
        let (f1, f2, f3) =
            (arena.name("FontFile"), arena.name("FontFile2"), arena.name("FontFile3"));
        for k in [f1, f2, f3] {
            if let Some(ff) = fd_dict.get(&k) {
                return ff.as_reference();
            }
        }
        None
    }

    pub(super) fn parse_descendant_font(
        dict: &BTreeMap<Handle<PdfName>, Object>,
        doc: &Document,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<DescendantResult> {
        let arena = doc.arena();
        let df_dict_obj = Self::get_descendant_font_obj(dict, arena, decisions)?;
        let df_dict_resolved = df_dict_obj.resolve(arena);
        let df_h = df_dict_obj.as_reference();

        let mut font_resource = None;
        if let Some(h) = df_h
            && let Ok(res) = doc.get_font(h)
        {
            font_resource = Some(res);
        }

        let dfh = df_dict_resolved.as_dict_handle()?;
        let df_dict = arena.get_dict(dfh)?;

        let font_file_handle = Self::extract_font_file_handle(&df_dict, arena);

        // Favor the mapping from the FontResource (which includes embedded CFF truth)
        // over the potentially missing or "Identity" PDF dictionary mapping.
        let cid_to_gid_map = if let Some(ref fr) = font_resource {
            fr.cid_to_gid_map
                .clone()
                .or_else(|| Self::parse_cid_to_gid_map(&df_dict, doc, decisions))
        } else {
            Self::parse_cid_to_gid_map(&df_dict, doc, decisions)
        };

        let (ordering, registry) = Self::read_csi(&df_dict, arena);

        let res = DescendantResult {
            base_font: df_dict
                .get(&arena.name("BaseFont"))
                .and_then(|o| o.resolve(arena).as_name())
                .and_then(|h| arena.get_name(h)),
            font_data: Self::extract_descendant_font_data(&df_dict, &font_resource, doc),
            font_descriptor: df_dict
                .get(&arena.name("FontDescriptor"))
                .and_then(|fd_obj| fd_obj.as_reference()),
            font_file_handle,
            cid_to_gid_map,
            metrics: FontMetrics::parse_cid(&df_dict, arena),
            ordering,
            registry,
        };

        Some(res)
    }

    pub(super) fn parse_cid_to_gid_map(
        df_dict: &BTreeMap<Handle<PdfName>, Object>,
        doc: &Document,
        decisions: &mut Vec<crate::interpretation::Decision>,
    ) -> Option<BTreeMap<u32, u32>> {
        let arena = doc.arena();
        let map_obj = df_dict.get(&arena.name("CIDToGIDMap"))?;
        let resolved = map_obj.resolve(arena);
        if let Some(name) = resolved.as_name().and_then(|h| arena.get_name(h))
            && name.as_str() == "Identity"
        {
            return None;
        }
        let data = match doc.decode_stream(&resolved) {
            Ok(d) => d,
            Err(e) => {
                decisions.push(crate::interpretation::Decision::violation(
                    "9.7.4.3",
                    format!("the /CIDToGIDMap stream did not decode: {e:?}"),
                    "treated the map as absent, so CIDs map to themselves",
                ));
                return None;
            }
        };
        let mut map = BTreeMap::new();
        let (chunks, _) = data.as_chunks::<2>();
        for (i, &chunk) in chunks.iter().enumerate() {
            let gid = u32::from(u16::from_be_bytes(chunk));
            if gid != 0 {
                map.insert(i as u32, gid);
            }
        }
        Some(map)
    }
}
