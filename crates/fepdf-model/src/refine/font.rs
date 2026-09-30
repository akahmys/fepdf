use super::RefinedObject;
use crate::object::PdfName;
use std::collections::BTreeMap;

/// Normalizes a font dictionary to a canonical PDF 2.0 form.
pub fn normalize_font(mut dict: BTreeMap<PdfName, RefinedObject>) -> RefinedObject {
    let type_key = PdfName::new("Type");
    let subtype_key = PdfName::new("Subtype");

    // Only process if it's actually a Font
    if let Some(RefinedObject::Name(t)) = dict.get(&type_key)
        && t.as_str() != "Font"
    {
        return RefinedObject::Dictionary(dict);
    }

    let subtype = dict.get(&subtype_key).and_then(|o| o.as_str()).map(|s| s.to_string());

    if let Some(st_str) = subtype {
        // CIDFonts (descendants) need CIDToGIDMap Identity if missing
        if st_str == "CIDFontType0" || st_str == "CIDFontType2" {
            dict.entry(PdfName::new("CIDToGIDMap"))
                .or_insert_with(|| RefinedObject::Name(PdfName::new("Identity")));
        }
    }

    RefinedObject::Dictionary(dict)
}
