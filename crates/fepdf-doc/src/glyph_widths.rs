//! 31-016: a glyph's width in the font dictionary and in the program.
//!
//! UA1:7.21.5 asks that a rendered glyph's two widths agree to 1/1000 unit.
//!
//! **The dictionary's width for the code, the program's for the glyph the code reaches.**
//! Which glyph that is comes from [`crate::glyph_map`]; the program's width is read in
//! units of 1/1000 em, as the dictionary's are.

use crate::glyph_map::Target;
use fepdf_font::program_glyphs;
use fepdf_model::access::{entry, items, name_in};
use fepdf_model::{Document, Handle, Object, PdfArena};
use std::collections::BTreeMap;

/// The widths a program states, by what it keys its glyphs by.
pub(crate) enum ProgramWidths {
    /// By glyph index.
    ByGid(Vec<f64>),
    /// By glyph name.
    ByName(BTreeMap<String, f64>),
}

impl ProgramWidths {
    /// The width of the glyph `target` names, when the program states one.
    pub(crate) fn of(&self, target: &Target) -> Option<f64> {
        match (self, target) {
            (Self::ByGid(widths), Target::Gid(g)) => widths.get(usize::from(*g)).copied(),
            (Self::ByName(widths), Target::Name(n)) => widths.get(n).copied(),
            (Self::ByGid(_), Target::Name(_)) | (Self::ByName(_), Target::Gid(_)) => None,
        }
    }
}

/// The widths `font`'s embedded program states, when this reads them: a TrueType program's
/// `hmtx`, a name-keyed CFF program's charstrings, a Type 1 program's `hsbw`s. **Nothing**
/// for a CID-keyed CFF program, a Type 3 font, or a program that will not read.
pub(crate) fn program_widths(doc: &Document, font: Handle<Object>) -> Option<ProgramWidths> {
    let arena = doc.arena();
    let font = Object::Reference(font);
    let decoded = |file: &Object| doc.decode_stream(file).ok();
    match name_in(arena, &font, "Subtype").as_deref()? {
        "TrueType" => {
            let descriptor = entry(arena, &font, "FontDescriptor")?;
            let program = decoded(&entry(arena, &descriptor, "FontFile2")?)?;
            program_glyphs::sfnt_advances(&program).map(ProgramWidths::ByGid)
        }
        "Type1" | "MMType1" => {
            let descriptor = entry(arena, &font, "FontDescriptor")?;
            if let Some(file) = entry(arena, &descriptor, "FontFile3") {
                return program_glyphs::cff_name_advances(&decoded(&file)?)
                    .map(ProgramWidths::ByName);
            }
            let file = entry(arena, &descriptor, "FontFile")?;
            let cleartext = usize::try_from(entry(arena, &file, "Length1")?.as_integer()?).ok()?;
            program_glyphs::type1_advances(&decoded(&file)?, cleartext).map(ProgramWidths::ByName)
        }
        "Type0" => {
            let descendant = items(arena, &font, "DescendantFonts").into_iter().next()?;
            if name_in(arena, &descendant, "Subtype").as_deref() != Some("CIDFontType2") {
                return None;
            }
            let descriptor = entry(arena, &descendant, "FontDescriptor")?;
            let program = decoded(&entry(arena, &descriptor, "FontFile2")?)?;
            program_glyphs::sfnt_advances(&program).map(ProgramWidths::ByGid)
        }
        _ => None,
    }
}

/// The width the dictionary gives `code`: a simple font's `/Widths` entry, or its
/// descriptor's `/MissingWidth` (0 when absent) for a code outside `/FirstChar` to
/// `/LastChar`; a Type 0 font's CIDFont `/W` entry for the CID, or its `/DW` (1000 when
/// absent) (ISO 32000-1 Tables 111, 117).
pub(crate) fn dictionary_width(arena: &PdfArena, font: Handle<Object>, code: u32) -> Option<f64> {
    let font = Object::Reference(font);
    let number = |o: &Object| o.as_f64();
    if name_in(arena, &font, "Subtype").as_deref() == Some("Type0") {
        let descendant = items(arena, &font, "DescendantFonts").into_iter().next()?;
        let default = entry(arena, &descendant, "DW").as_ref().and_then(number).unwrap_or(1000.0);
        return Some(cid_width(arena, &items(arena, &descendant, "W"), code).unwrap_or(default));
    }
    let first = entry(arena, &font, "FirstChar").and_then(|f| f.as_integer())?;
    let widths = items(arena, &font, "Widths");
    let at = i64::from(code) - first;
    let listed = usize::try_from(at).ok().and_then(|i| widths.get(i)).and_then(number);
    Some(listed.unwrap_or_else(|| {
        entry(arena, &font, "FontDescriptor")
            .and_then(|d| entry(arena, &d, "MissingWidth"))
            .as_ref()
            .and_then(number)
            .unwrap_or(0.0)
    }))
}

/// A CID's width in a `/W` array: `c [w1 w2 …]` gives consecutive CIDs from `c`, and
/// `c_first c_last w` one width to a range (9.7.4.3).
fn cid_width(arena: &PdfArena, w: &[Object], cid: u32) -> Option<f64> {
    let cid = i64::from(cid);
    let mut at = 0;
    while let Some(first) = w.get(at).and_then(Object::as_integer) {
        match w.get(at + 1).map(|o| o.resolve(arena)) {
            Some(Object::Array(list)) => {
                let list = arena.get_array(list).unwrap_or_default();
                if let Some(width) = usize::try_from(cid - first).ok().and_then(|i| list.get(i)) {
                    return width.resolve(arena).as_f64();
                }
                at += 2;
            }
            Some(last) => {
                let (last, width) = (last.as_integer()?, w.get(at + 2)?.as_f64()?);
                if (first..=last).contains(&cid) {
                    return Some(width);
                }
                at += 3;
            }
            None => return None,
        }
    }
    None
}
