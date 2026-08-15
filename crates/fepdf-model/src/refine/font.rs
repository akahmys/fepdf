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

    // No `/ToUnicode` is synthesised here, and the entry that used to be is gone.
    //
    // `generate_standard_tounicode` keys its map on **glyph** ids, because that is what
    // `unicode_to_gid` holds. Under `Identity-H` the codes in the content stream are
    // CIDs. The two coincide only for a `CIDFontType2` written with `CIDToGIDMap
    // /Identity`; a `CIDFontType0` maps CIDs through the CFF charset, and
    // `CIDToGIDMap` does not apply to it at all. `samples/fy05.pdf` is the second case.
    //
    // Emitting a wrong map is worse than emitting none, because a reader trusts
    // `/ToUnicode` over the registry ordering it would otherwise resolve through.
    // Round-tripping the corpus and reading the output with PDFKit:
    //
    //   fy05.pdf    source 251,922    with the map 251,829    without it 251,930
    //   other eight                   identical               identical
    //
    // Ninety-three pages lost text, five of them all of it — page 1's title among
    // them. The map helped no file in the corpus and harmed one.
    //
    // Reinstating it needs a file that proves the narrow case: a `CIDFontType2` with
    // `CIDToGIDMap /Identity`, no `/ToUnicode`, and text a reader cannot otherwise
    // recover. Without such a file this is a guess, and the guess was measured wrong.
    let _ = resource;
}

/// Normalizes a CMap stream to a canonical PDF 2.0 form.
pub fn normalize_cmap(dict: BTreeMap<PdfName, RefinedObject>, data: Bytes) -> RefinedObject {
    RefinedObject::Stream(dict, data)
}
