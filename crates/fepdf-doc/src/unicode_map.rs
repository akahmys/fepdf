//! 10-001: every character code shown maps to Unicode.
//!
//! UA1:7.2 asks it by one of the methods ISO 32000-1 9.10.2 lists, as 14.8.2.4.2 requires
//! of a tagged document.
//!
//! **Code by code, in the priority 9.10.2 gives.** A `/ToUnicode` that maps the code; a
//! simple font on MacRomanEncoding, MacExpertEncoding or WinAnsiEncoding, or whose
//! `/Differences` names only the Adobe standard Latin set and the Symbol font's, through
//! the code's name and Adobe's list; a composite font on a Table 118 CMap other than
//! Identity, or on one of Adobe's four CJK collections. A code none of them reaches maps
//! to nothing. A code this cannot split out of a string, or whose name only a program it
//! does not read knows, is left for a reader.

use crate::structure::{AuditFinding, broken, for_a_reader};
use fepdf_font::cmap::CMap;
use fepdf_model::access::{entry, items, name_in};
use fepdf_model::{Document, Handle, Object, PdfArena};
use std::collections::{BTreeMap, BTreeSet};

/// What came of a font's codes: those no method maps, and those this could not settle.
#[derive(Default)]
pub(crate) struct Mapped {
    pub(crate) unmapped: Vec<u32>,
    pub(crate) unsettled: Vec<u32>,
}

/// Whether a Type 0 font's CIDFont is in one of Adobe's four CJK collections.
pub(crate) fn adobe_collection(arena: &PdfArena, font: &Object) -> bool {
    let Some(descendant) = items(arena, font, "DescendantFonts").into_iter().next() else {
        return false;
    };
    let Some(info) = entry(arena, &descendant, "CIDSystemInfo") else { return false };
    let text = |key: &str| match entry(arena, &info, key) {
        Some(Object::String(b) | Object::Hex(b)) => String::from_utf8_lossy(&b).into_owned(),
        Some(Object::Text(t)) => t,
        _ => String::new(),
    };
    text("Registry") == "Adobe"
        && ["GB1", "CNS1", "Japan1", "Korea1"].contains(&text("Ordering").as_str())
}

/// Whether `cmap` maps the code of `width` bytes whose value is `code`.
fn covers(cmap: &CMap, code: u32, width: usize) -> bool {
    let bytes: Vec<u8> = code.to_be_bytes()[4 - width..].to_vec();
    cmap.mappings.contains_key(&bytes)
        || cmap.bf_ranges.iter().any(|r| r.len == width && (r.start..=r.end).contains(&code))
}

/// 10-001 of the one-byte `codes` and two-byte `pairs` shown in `font`.
pub(crate) fn mapped(
    doc: &Document,
    font: Handle<Object>,
    codes: &BTreeSet<u8>,
    pairs: &BTreeSet<u32>,
) -> Mapped {
    let arena = doc.arena();
    let object = Object::Reference(font);
    let to_unicode = match entry(arena, &object, "ToUnicode") {
        Some(stream @ Object::Stream(..)) => {
            doc.decode_stream(&stream).ok().and_then(|b| CMap::parse(&b).ok())
        }
        _ => None,
    };
    if name_in(arena, &object, "Subtype").as_deref() == Some("Type0") {
        return composite(doc, font, to_unicode.as_ref(), (codes, pairs));
    }
    let fallback = simple_names(arena, &object);
    let mut out = Mapped::default();
    for code in codes.iter().map(|c| u32::from(*c)) {
        if to_unicode.as_ref().is_some_and(|m| covers(m, code, 1)) {
            continue;
        }
        match &fallback {
            None => out.unmapped.push(code),
            Some(names) => match u8::try_from(code).ok().and_then(|c| names.get(&c)) {
                Some(Some(name)) if fepdf_font::agl::lookup(name).is_some() => {}
                Some(Some(_)) => out.unmapped.push(code),
                Some(None) => out.unsettled.push(code),
                None => out.unmapped.push(code),
            },
        }
    }
    out
}

/// ISO 32000-1 Table 118's predefined CMaps, less `Identity-H` and `Identity-V`, which
/// 9.10.2's third method excludes — read out of `PDF32000_2008.pdf`.
const TABLE_118: [&str; 59] = [
    "83pv-RKSJ-H",
    "90ms-RKSJ-H",
    "90ms-RKSJ-V",
    "90msp-RKSJ-H",
    "90msp-RKSJ-V",
    "90pv-RKSJ-H",
    "Add-RKSJ-H",
    "Add-RKSJ-V",
    "B5pc-H",
    "B5pc-V",
    "CNS-EUC-H",
    "CNS-EUC-V",
    "ETen-B5-H",
    "ETen-B5-V",
    "ETenms-B5-H",
    "ETenms-B5-V",
    "EUC-H",
    "EUC-V",
    "Ext-RKSJ-H",
    "Ext-RKSJ-V",
    "GB-EUC-H",
    "GB-EUC-V",
    "GBK-EUC-H",
    "GBK-EUC-V",
    "GBK2K-H",
    "GBK2K-V",
    "GBKp-EUC-H",
    "GBKp-EUC-V",
    "GBpc-EUC-H",
    "GBpc-EUC-V",
    "H",
    "HKscs-B5-H",
    "HKscs-B5-V",
    "KSC-EUC-H",
    "KSC-EUC-V",
    "KSCms-UHC-H",
    "KSCms-UHC-HW-H",
    "KSCms-UHC-HW-V",
    "KSCms-UHC-V",
    "KSCpc-EUC-H",
    "UniCNS-UCS2-H",
    "UniCNS-UCS2-V",
    "UniCNS-UTF16-H",
    "UniCNS-UTF16-V",
    "UniGB-UCS2-H",
    "UniGB-UCS2-V",
    "UniGB-UTF16-H",
    "UniGB-UTF16-V",
    "UniJIS-UCS2-H",
    "UniJIS-UCS2-HW-H",
    "UniJIS-UCS2-HW-V",
    "UniJIS-UCS2-V",
    "UniJIS-UTF16-H",
    "UniJIS-UTF16-V",
    "UniKS-UCS2-H",
    "UniKS-UCS2-V",
    "UniKS-UTF16-H",
    "UniKS-UTF16-V",
    "V",
];

/// A composite font's codes. 9.10.2's third method maps every code of a font on a Table 118
/// CMap or on one of Adobe's four CJK collections; otherwise each two-byte code — where the
/// CMap was Identity, or the `/ToUnicode` says codes are two bytes — is asked of the
/// `/ToUnicode`, and codes this cannot split out are left for a reader.
fn composite(
    doc: &Document,
    font: Handle<Object>,
    to_unicode: Option<&CMap>,
    shown: (&BTreeSet<u8>, &BTreeSet<u32>),
) -> Mapped {
    let arena = doc.arena();
    let object = Object::Reference(font);
    let named =
        doc.get_font(font).ok().and_then(|f| f.encoding.as_ref().map(|e| e.name().to_owned()));
    if adobe_collection(arena, &object) || named.as_deref().is_some_and(|n| TABLE_118.contains(&n))
    {
        return Mapped::default();
    }
    let identity = named.as_deref().is_some_and(|n| n.starts_with("Identity"));
    let two_byte = identity
        || to_unicode.is_some_and(|m| {
            !m.codespace_ranges.is_empty() && m.codespace_ranges.iter().all(|(s, _)| s.len() == 2)
        });
    if !two_byte {
        let every = shown.0.iter().map(|c| u32::from(*c)).chain(shown.1.iter().copied());
        return Mapped { unmapped: Vec::new(), unsettled: every.collect() };
    }
    let unmapped =
        shown.1.iter().copied().filter(|c| !to_unicode.is_some_and(|m| covers(m, *c, 2))).collect();
    Mapped { unmapped, unsettled: Vec::new() }
}

/// A simple font's names for 9.10.2's second method, when the method applies: each code
/// to its name, or to `None` where only the font's built-in encoding — which this does not
/// read — names it. **Nothing** when the method does not apply.
fn simple_names(arena: &PdfArena, font: &Object) -> Option<BTreeMap<u8, Option<String>>> {
    let predefined =
        |name: &str| matches!(name, "MacRomanEncoding" | "MacExpertEncoding" | "WinAnsiEncoding");
    let base = base_font(arena, font);
    let standard_latin = crate::audit_fonts::STANDARD_LATIN.contains(&base.as_str());
    let Some(encoding) = entry(arena, font, "Encoding") else {
        // One of the standard 14 with no `/Encoding` is on its built-in encoding —
        // StandardEncoding, or Symbol's or ZapfDingbats' own (D.5, D.6) — which the second
        // method's list does not name and readers map all the same: left for a reader
        // rather than called unmapped.
        let standard_14 = standard_latin || matches!(base.as_str(), "Symbol" | "ZapfDingbats");
        return standard_14.then(|| (0..=255).map(|c| (c, None)).collect());
    };
    if let Some(name) = encoding.as_name().and_then(|n| arena.get_name(n)) {
        let table = crate::glyph_map::annex(name.as_str())?;
        return Some(table.into_iter().map(|(c, n)| (c, Some(n))).collect());
    }
    let base = name_in(arena, &encoding, "BaseEncoding");
    let differences = crate::glyph_map::differences(arena, &encoding);
    let standard = |name: &str| {
        latin_names().contains(name) || fepdf_font::latin_names::SYMBOL_NAMES.contains(&name)
    };
    if !base.as_deref().is_some_and(predefined) && !differences.values().all(|n| standard(n)) {
        return None;
    }
    let base_names: BTreeMap<u8, Option<String>> = match base.as_deref() {
        Some(name) => {
            crate::glyph_map::annex(name)?.into_iter().map(|(c, n)| (c, Some(n))).collect()
        }
        None if crate::audit_fonts::STANDARD_LATIN.contains(&base_font(arena, font).as_str()) => {
            fepdf_font::latin_names::STANDARD_ENCODING
                .iter()
                .map(|(c, n)| (*c, Some((*n).to_owned())))
                .collect()
        }
        None => (0..=255).map(|c| (c, None)).collect(),
    };
    let mut table = base_names;
    table.extend(differences.into_iter().map(|(c, n)| (c, Some(n))));
    Some(table)
}

/// The Adobe standard Latin character set: every name D.2 gives a code in any encoding.
fn latin_names() -> BTreeSet<&'static str> {
    fepdf_font::latin_names::STANDARD_ENCODING
        .iter()
        .chain(fepdf_font::latin_names::MAC_ROMAN.iter())
        .chain(fepdf_font::latin_names::WIN_ANSI.iter())
        .map(|(_, name)| *name)
        .collect()
}

/// A font's `/BaseFont`.
fn base_font(arena: &PdfArena, font: &Object) -> String {
    name_in(arena, font, "BaseFont").unwrap_or_default()
}

/// 10-001's findings for one font, named `name` — or 17-003's, the same requirement of the
/// text in a `<Formula>` (UA1:7.7).
pub(crate) fn report(
    condition: &'static str,
    name: &str,
    mapped: &Mapped,
    findings: &mut Vec<AuditFinding>,
) {
    let within = if condition == "17-003" { " inside a <Formula>" } else { "" };
    if let Some(first) = mapped.unmapped.first() {
        findings.push(broken(
            condition,
            format!(
                "/{name}: {} codes its text shows{within} map to Unicode by none of the \
                 methods ISO 32000-1 9.10.2 lists, 0x{first:02X} first",
                mapped.unmapped.len()
            ),
        ));
    }
    if let Some(first) = mapped.unsettled.first() {
        findings.push(for_a_reader(
            condition,
            format!(
                "/{name}: {} codes its text shows{within} are named only by an encoding or \
                 CMap not read here, 0x{first:02X} first — look at what they read as",
                mapped.unsettled.len()
            ),
        ));
    }
}
