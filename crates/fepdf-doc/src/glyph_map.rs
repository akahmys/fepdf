//! Which glyph each character code of a font selects, as ISO 32000-1 says and no further.
//!
//! **Only the fonts it can map without a guess.** The engine's own resolution tries every
//! route it knows and scores the candidates, which is right for drawing a page and wrong
//! for saying whether a code selects `.notdef` (31-030) or a glyph the program lacks
//! (31-011). This follows 9.6.6 for simple fonts and an Identity CMap for composite ones,
//! and a font it cannot follow that way has no map: its codes are left for a reader.

use crate::audit_subsets::GidMap;
use fepdf_font::program_glyphs;
use fepdf_font::sfnt_cmap::CmapSubtable;
use fepdf_model::access::{entry, name_in};
use fepdf_model::{Document, Handle, Object, PdfArena};
use std::collections::{BTreeMap, BTreeSet};

/// What a code selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Glyph {
    /// A glyph the program holds.
    Present,
    /// `.notdef` — by name, by glyph 0, or by a code the encoding leaves undefined.
    NotDef,
    /// A glyph the program does not hold.
    Missing,
}

/// Which glyph a code reaches, by what the program keys its glyphs by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    /// A glyph index — TrueType programs, and CFF ones through a CIDFont's charset.
    Gid(u16),
    /// A glyph name — Type 1 programs, CFF ones in a simple font, and Type 3 procedures.
    Name(String),
}

/// A font's codes, and what each selects.
pub(crate) enum GlyphMap {
    /// One-byte codes, and the glyph each reaches where it reaches one. A code not in the map
    /// is one this could not settle.
    Simple(BTreeMap<u8, (Glyph, Option<Target>)>),
    /// Two-byte codes under an Identity CMap, each the CID it is, and what each CID selects.
    Cids(Cids),
}

/// What a CIDFont's CIDs select.
pub(crate) enum Cids {
    /// A TrueType program, through `/CIDToGIDMap`, with its glyph count.
    TrueType(GidMap, u16),
    /// A CFF program, whose charset names the CIDs it holds and their glyph indices.
    Cff(BTreeMap<u32, u16>),
}

impl GlyphMap {
    /// What `code` selects, when this can say.
    pub(crate) fn select(&self, code: u32) -> Option<Glyph> {
        match self {
            Self::Simple(table) => u8::try_from(code).ok().and_then(|c| table.get(&c)).map(|g| g.0),
            Self::Cids(_) if code == 0 => Some(Glyph::NotDef),
            Self::Cids(Cids::Cff(held)) => {
                Some(if held.contains_key(&code) { Glyph::Present } else { Glyph::Missing })
            }
            Self::Cids(Cids::TrueType(map, slots)) => Some(match map.glyph(code) {
                None => Glyph::Missing,
                Some(0) => Glyph::NotDef,
                Some(g) if g >= *slots => Glyph::Missing,
                Some(_) => Glyph::Present,
            }),
        }
    }

    /// The glyph `code` reaches, when it reaches one this can name.
    pub(crate) fn target(&self, code: u32) -> Option<Target> {
        match self {
            Self::Simple(table) => {
                u8::try_from(code).ok().and_then(|c| table.get(&c)).and_then(|g| g.1.clone())
            }
            Self::Cids(Cids::Cff(held)) => held.get(&code).copied().map(Target::Gid),
            Self::Cids(Cids::TrueType(map, _)) => map.glyph(code).map(Target::Gid),
        }
    }

    /// Whether codes are two bytes.
    pub(crate) const fn two_byte(&self) -> bool {
        matches!(self, Self::Cids(_))
    }
}

/// The map of `font`, when it is a kind this can map and its program reads.
pub(crate) fn of(doc: &Document, font: Handle<Object>) -> Option<GlyphMap> {
    let arena = doc.arena();
    let font_object = Object::Reference(font);
    let descriptor = entry(arena, &font_object, "FontDescriptor");
    match name_in(arena, &font_object, "Subtype").as_deref()? {
        "TrueType" => true_type(doc, &font_object, &descriptor?),
        "Type1" | "MMType1" => {
            let descriptor = descriptor?;
            if let Some(file) = entry(arena, &descriptor, "FontFile3") {
                type1c(doc, &font_object, &file)
            } else {
                type1(doc, &font_object, &entry(arena, &descriptor, "FontFile")?)
            }
        }
        "Type3" => type3(arena, &font_object),
        "Type0" => composite(doc, font),
        _ => None,
    }
}

/// A simple font's `/Differences`, as code to name.
pub(crate) fn differences(arena: &PdfArena, encoding: &Object) -> BTreeMap<u8, String> {
    let mut names = BTreeMap::new();
    let Some(Object::Array(array)) = entry(arena, encoding, "Differences") else { return names };
    let mut code: Option<u8> = None;
    for item in arena.get_array(array).unwrap_or_default() {
        match item.resolve(arena) {
            Object::Integer(start) => code = u8::try_from(start).ok(),
            other => {
                let (Some(at), Some(name)) =
                    (code, other.as_name().and_then(|n| arena.get_name(n)))
                else {
                    continue;
                };
                names.insert(at, name.as_str().to_string());
                code = at.checked_add(1);
            }
        }
    }
    names
}

/// One of the two Annex D encodings a font dictionary may name, as code to name.
pub(crate) fn annex(name: &str) -> Option<BTreeMap<u8, String>> {
    let table: &[(u8, &str)] = match name {
        "WinAnsiEncoding" => &fepdf_font::latin_names::WIN_ANSI,
        "MacRomanEncoding" => &fepdf_font::latin_names::MAC_ROMAN,
        _ => return None,
    };
    Some(table.iter().map(|(code, name)| (*code, (*name).to_owned())).collect())
}

/// The code-to-name table 9.6.6.4 builds for a non-symbolic TrueType font: a named
/// encoding's Annex D names, or a dictionary's base, its `/Differences` over it, and
/// StandardEncoding for what is left. An `/Encoding` naming anything else, or none,
/// has no table (31-019 and 31-021 say so).
pub(crate) fn true_type_names(arena: &PdfArena, font: &Object) -> Option<BTreeMap<u8, String>> {
    let encoding = entry(arena, font, "Encoding")?;
    if let Some(name) = encoding.as_name().and_then(|n| arena.get_name(n)) {
        return annex(name.as_str());
    }
    let mut table = match name_in(arena, &encoding, "BaseEncoding") {
        Some(base) => annex(&base)?,
        None => BTreeMap::new(),
    };
    table.extend(differences(arena, &encoding));
    for (code, name) in fepdf_font::latin_names::STANDARD_ENCODING {
        table.entry(code).or_insert_with(|| name.to_owned());
    }
    Some(table)
}

/// A Type 1 or Type 3 font's code-to-name table (9.6.6.1, 9.6.6.2): its built-in encoding
/// where `/Encoding` is absent, the named Annex D one, or a dictionary's base — named, or
/// the built-in one — with its `/Differences` over it. **Nothing when the table cannot be
/// known whole**: a base this does not carry, or a built-in encoding it could not read.
fn simple_names(
    arena: &PdfArena,
    font: &Object,
    built_in: Option<BTreeMap<u8, String>>,
) -> Option<BTreeMap<u8, String>> {
    let Some(encoding) = entry(arena, font, "Encoding") else { return built_in };
    if let Some(name) = encoding.as_name().and_then(|n| arena.get_name(n)) {
        return annex(name.as_str());
    }
    let mut table = match name_in(arena, &encoding, "BaseEncoding") {
        Some(base) => annex(&base)?,
        None => built_in?,
    };
    table.extend(differences(arena, &encoding));
    Some(table)
}

/// Every code's glyph, from its name: `.notdef` for a code the table leaves undefined or
/// names `.notdef`, present or missing as `held` says of the rest.
fn by_name(names: &BTreeMap<u8, String>, held: impl Fn(&str) -> bool) -> GlyphMap {
    GlyphMap::Simple(
        (0..=255_u8)
            .map(|code| {
                let glyph = match names.get(&code).map(String::as_str) {
                    None | Some(".notdef") => (Glyph::NotDef, None),
                    Some(name) if held(name) => {
                        (Glyph::Present, Some(Target::Name(name.to_owned())))
                    }
                    Some(_) => (Glyph::Missing, None),
                };
                (code, glyph)
            })
            .collect(),
    )
}

/// A TrueType font (9.6.6.4). Non-symbolic: each code's name, through (3,1) by its Unicode
/// value or (1,0) by its Mac OS Roman code, a code with no usable name left unsettled.
/// Symbolic: (3,0) with the byte in whichever of the four ranges the subtable uses, or
/// (1,0) with the byte. Glyph 0 — mapped, or not mapped at all — is `.notdef`.
fn true_type(doc: &Document, font: &Object, descriptor: &Object) -> Option<GlyphMap> {
    let arena = doc.arena();
    let program = doc.decode_stream(&entry(arena, descriptor, "FontFile2")?).ok()?;
    let slots = program_glyphs::sfnt_glyph_count(&program)?;
    let settle = |glyph: u16| match glyph {
        0 => (Glyph::NotDef, None),
        g if g >= slots => (Glyph::Missing, None),
        g => (Glyph::Present, Some(Target::Gid(g))),
    };
    let flags = entry(arena, descriptor, "Flags").and_then(|f| f.as_f64()).unwrap_or(0.0);
    #[allow(clippy::cast_possible_truncation)] // a flag word, written as an integer
    let symbolic = (flags as i64) & 4 != 0;
    if !symbolic {
        let names = true_type_names(arena, font)?;
        let lookup = crate::truetype_lookup::Lookup::of(&program)?;
        let table = names
            .iter()
            .filter_map(|(code, name)| Some((*code, settle(lookup.glyph_of(name)?))))
            .collect();
        return Some(GlyphMap::Simple(table));
    }
    let byte = |subtable: &CmapSubtable<'_>, high: u32| -> BTreeMap<u8, (Glyph, Option<Target>)> {
        (0..=255_u8).map(|c| (c, settle(subtable.glyph_or_notdef(high | u32::from(c))))).collect()
    };
    if let Some(symbol) = CmapSubtable::find(&program, 3, 0) {
        let high = [0x0000, 0xF000, 0xF100, 0xF200]
            .into_iter()
            .find(|high| (0..=255_u32).any(|c| symbol.glyph(high | c).is_some()))?;
        return Some(GlyphMap::Simple(byte(&symbol, high)));
    }
    CmapSubtable::find(&program, 1, 0).map(|mac| GlyphMap::Simple(byte(&mac, 0)))
}

/// A Type 1 font whose program is CFF (`/FontFile3`): each code's name, from the
/// dictionary or the program's own encoding, looked up in the program's charset.
fn type1c(doc: &Document, font: &Object, file: &Object) -> Option<GlyphMap> {
    let program = doc.decode_stream(file).ok()?;
    let table = program_glyphs::cff_table(&program)?;
    let built_in: BTreeMap<u8, String> = (0..=255_u8)
        .filter_map(|code| {
            let gid = table.glyph_index(code)?;
            Some((code, table.glyph_name(gid)?.to_owned()))
        })
        .collect();
    let names = simple_names(doc.arena(), font, Some(built_in))?;
    // The charset's names, read glyph by glyph as 31-012 reads them: `glyph_index_by_name`
    // missed `space` in a Calluna subset whose charset has it.
    let held = program_glyphs::cff_glyph_names(&program)?;
    Some(by_name(&names, |name| held.contains(name)))
}

/// A Type 1 font whose program is Type 1 (`/FontFile`): each code's name, from the
/// dictionary or the encoding the program's cleartext states, in its `/CharStrings`.
fn type1(doc: &Document, font: &Object, file: &Object) -> Option<GlyphMap> {
    let arena = doc.arena();
    let cleartext = usize::try_from(entry(arena, file, "Length1")?.as_integer()?).ok()?;
    let program = doc.decode_stream(file).ok()?;
    // A program that reads as holding nothing has not been read, and says nothing.
    let held = program_glyphs::type1_glyph_names(&program, cleartext).filter(|h| !h.is_empty())?;
    let built_in = program_glyphs::type1_built_in_encoding(program.get(..cleartext)?);
    let names = simple_names(arena, font, built_in)?;
    Some(by_name(&names, |name| held.contains(name)))
}

/// A Type 3 font: each code's name from its `/Encoding`, in its `/CharProcs`.
fn type3(arena: &PdfArena, font: &Object) -> Option<GlyphMap> {
    let procedures = entry(arena, font, "CharProcs")?.as_dict_handle()?;
    let held: BTreeSet<String> = arena
        .get_dict(procedures)?
        .into_keys()
        .filter_map(|k| arena.get_name(k).map(|n| n.as_str().to_string()))
        .collect();
    let names = simple_names(arena, font, None)?;
    Some(by_name(&names, |name| held.contains(name)))
}

/// A Type 0 font whose CMap was an Identity one, so that each two-byte code is its CID.
///
/// **The CMap as the loaded font read it**, which is the one the file named: ingestion
/// keeps a Type 0 font's `/Encoding` as written
/// ([ADR-0105](../../../docs/adr/0105-ingestion-never-rewrote-a-real-type-0-cmap.md)).
fn composite(doc: &Document, font: Handle<Object>) -> Option<GlyphMap> {
    let loaded = doc.get_font(font).ok()?;
    if !loaded.encoding.as_ref()?.name().starts_with("Identity") {
        return None;
    }
    let arena = doc.arena();
    let Some(Object::Array(descendants)) =
        entry(arena, &Object::Reference(font), "DescendantFonts")
    else {
        return None;
    };
    let descendant = arena.get_array(descendants)?.first()?.resolve(arena);
    let descriptor = entry(arena, &descendant, "FontDescriptor")?;
    match name_in(arena, &descendant, "Subtype").as_deref()? {
        "CIDFontType2" => {
            let program = doc.decode_stream(&entry(arena, &descriptor, "FontFile2")?).ok()?;
            let slots = program_glyphs::sfnt_glyph_count(&program)?;
            Some(GlyphMap::Cids(Cids::TrueType(GidMap::of(doc, &descendant), slots)))
        }
        "CIDFontType0" => {
            let program = doc.decode_stream(&entry(arena, &descriptor, "FontFile3")?).ok()?;
            Some(GlyphMap::Cids(Cids::Cff(program_glyphs::cff_cid_glyphs(&program)?)))
        }
        _ => None,
    }
}
