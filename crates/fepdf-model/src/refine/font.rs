use super::RefinedObject;
use crate::font::FontResource;
use crate::object::PdfName;
use bytes::Bytes;
use std::collections::BTreeMap;

/// Normalizes a font dictionary to a canonical PDF 2.0 form.
pub fn normalize_font(
    mut dict: BTreeMap<PdfName, RefinedObject>,
    resource: Option<&FontResource>,
) -> RefinedObject {
    let type_key = PdfName::new("Type");
    let subtype_key = PdfName::new("Subtype");

    // Only process if it's actually a Font
    if let Some(RefinedObject::Name(t)) = dict.get(&type_key)
        && t.as_str() != "Font"
    {
        return RefinedObject::Dictionary(dict);
    }

    let subtype = dict.get(&subtype_key).and_then(|o| o.as_str()).map(|s| s.to_string());
    if let Some(resource) = resource
        && resource.subtype.as_str() == "Type0"
    {
        normalize_type0_font(&mut dict, resource);
    }

    if let Some(st_str) = subtype {
        // CIDFonts (descendants) need CIDToGIDMap Identity if missing
        if st_str == "CIDFontType0" || st_str == "CIDFontType2" {
            dict.entry(PdfName::new("CIDToGIDMap"))
                .or_insert_with(|| RefinedObject::Name(PdfName::new("Identity")));
        }
    }

    RefinedObject::Dictionary(dict)
}

fn normalize_type0_font(dict: &mut BTreeMap<PdfName, RefinedObject>, resource: &FontResource) {
    let encoding_name = if resource.wmode == 1 { "Identity-V" } else { "Identity-H" };
    dict.insert(PdfName::new("Encoding"), RefinedObject::Name(PdfName::new(encoding_name)));

    // HARDENING: Only inject a generated ToUnicode map if it's missing.
    // This prevents clobbering authoritative subset mappings in documents like unicode_16.pdf.
    if let std::collections::btree_map::Entry::Vacant(e) = dict.entry(PdfName::new("ToUnicode"))
        && let Some(uni_map) = resource.generate_standard_tounicode()
        && !uni_map.is_empty()
    {
        e.insert(RefinedObject::Stream(BTreeMap::new(), bytes::Bytes::from(uni_map)));
    }
}

/// Normalizes a CMap stream to a canonical PDF 2.0 form.
pub fn normalize_cmap(dict: BTreeMap<PdfName, RefinedObject>, data: Bytes) -> RefinedObject {
    RefinedObject::Stream(dict, data)
}
