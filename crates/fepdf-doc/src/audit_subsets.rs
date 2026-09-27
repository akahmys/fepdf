//! 31-012 to 31-015 (UA1:7.21.4.2): a font descriptor's `/CharSet` or `/CIDSet` against
//! the program beside it.
//!
//! **Both directions, and each only as far as the program can be read.** A glyph the
//! program holds and the claim leaves out breaks the first condition of each pair; one the
//! claim lists and the program lacks breaks the second. `.notdef`, which `/CharSet` is to
//! leave out (ISO 32000-1 Table 122), and CID 0, its place in a CIDFont, are asked about
//! in neither direction.

use crate::audit_objects::{entry, name_of};
use crate::structure::{AuditFinding, broken};
use fepdf_font::program_glyphs;
use fepdf_model::{Document, Object};
use std::collections::BTreeSet;

/// Asks 31-012 to 31-015 of one font, named `name` in the findings.
pub(crate) fn subset_claims(
    doc: &Document,
    font: &Object,
    name: &str,
    findings: &mut Vec<AuditFinding>,
) {
    let arena = doc.arena();
    match name_of(arena, font, "Subtype").as_deref() {
        Some("Type1" | "MMType1") => char_set(doc, font, name, findings),
        Some("Type0") => {
            let Some(Object::Array(descendants)) = entry(arena, font, "DescendantFonts") else {
                return;
            };
            if let Some(descendant) = arena.get_array(descendants).unwrap_or_default().first() {
                cid_set(doc, &descendant.resolve(arena), name, findings);
            }
        }
        _ => {}
    }
}

/// Reports what the program holds that the claim does not, and the other way about.
fn compare<T: Ord + std::fmt::Debug>(
    (unlisted, absent): (&str, &str),
    name: &str,
    held: &BTreeSet<T>,
    listed: &BTreeSet<T>,
    what: &str,
    findings: &mut Vec<AuditFinding>,
) {
    if let Some(first) = held.difference(listed).next() {
        let count = held.difference(listed).count();
        findings.push(broken(
            unlisted,
            format!(
                "/{name}: its program holds {count} {what} its {} leaves out, {first:?} first",
                key(unlisted)
            ),
        ));
    }
    if let Some(first) = listed.difference(held).next() {
        let count = listed.difference(held).count();
        findings.push(broken(
            absent,
            format!(
                "/{name}: its {} lists {count} {what} its program lacks, {first:?} first",
                key(absent)
            ),
        ));
    }
}

/// The descriptor entry a condition is about.
fn key(condition: &str) -> &'static str {
    if matches!(condition, "31-012" | "31-013") { "/CharSet" } else { "/CIDSet" }
}

/// 31-012 and 31-013: an embedded Type 1 font's `/CharSet`, a string of names each
/// preceded by a slash, against the names the program's glyphs have.
fn char_set(doc: &Document, font: &Object, name: &str, findings: &mut Vec<AuditFinding>) {
    let arena = doc.arena();
    let Some(descriptor) = entry(arena, font, "FontDescriptor") else { return };
    let listed: BTreeSet<String> = match entry(arena, &descriptor, "CharSet") {
        Some(Object::String(bytes) | Object::Hex(bytes)) => String::from_utf8_lossy(&bytes)
            .split('/')
            .map(str::trim)
            .filter(|n| !n.is_empty() && *n != ".notdef")
            .map(str::to_owned)
            .collect(),
        Some(Object::Text(text)) => text
            .split('/')
            .map(str::trim)
            .filter(|n| !n.is_empty() && *n != ".notdef")
            .map(str::to_owned)
            .collect(),
        _ => return,
    };
    let held = if let Some(file) = entry(arena, &descriptor, "FontFile") {
        let cleartext = entry(arena, &file, "Length1").and_then(|l| l.as_integer());
        let (Ok(program), Some(cleartext)) = (doc.decode_stream(&file), cleartext) else { return };
        usize::try_from(cleartext)
            .ok()
            .and_then(|at| program_glyphs::type1_glyph_names(&program, at))
    } else if let Some(file) = entry(arena, &descriptor, "FontFile3") {
        doc.decode_stream(&file).ok().and_then(|p| program_glyphs::cff_glyph_names(&p))
    } else {
        return;
    };
    // A program that will not read, or reads as holding nothing, says nothing either way.
    if let Some(held) = held.filter(|h| !h.is_empty()) {
        compare(("31-012", "31-013"), name, &held, &listed, "glyph names", findings);
    }
}

/// 31-014 and 31-015: an embedded CIDFont's `/CIDSet`, a bit per CID from the high-order
/// bit of the first byte, against the CIDs its program holds.
fn cid_set(doc: &Document, descendant: &Object, name: &str, findings: &mut Vec<AuditFinding>) {
    let arena = doc.arena();
    let Some(descriptor) = entry(arena, descendant, "FontDescriptor") else { return };
    let Some(stream @ Object::Stream(..)) = entry(arena, &descriptor, "CIDSet") else { return };
    let Ok(bits) = doc.decode_stream(&stream) else { return };
    let listed: BTreeSet<u32> = (0..bits.len() * 8)
        .filter(|bit| bits[bit / 8] & (0x80 >> (bit % 8)) != 0)
        .filter_map(|bit| u32::try_from(bit).ok())
        .filter(|cid| *cid != 0)
        .collect();
    match name_of(arena, descendant, "Subtype").as_deref() {
        Some("CIDFontType0") => {
            let Some(file) = entry(arena, &descriptor, "FontFile3") else { return };
            let held = doc.decode_stream(&file).ok().and_then(|p| program_glyphs::cff_cids(&p));
            if let Some(mut held) = held {
                held.remove(&0);
                compare(("31-014", "31-015"), name, &held, &listed, "CIDs", findings);
            }
        }
        Some("CIDFontType2") => {
            true_type_cids(doc, descendant, &descriptor, &listed, name, findings);
        }
        _ => {}
    }
}

/// 31-014 and 31-015 for a TrueType CIDFont, through its `/CIDToGIDMap`.
///
/// **Held means an outline for 31-014, and a slot for 31-015.** A subset commonly keeps
/// every glyph slot and empties the ones it drops, so a slot is not evidence the glyph is
/// there; and a glyph with no outline, a space, can be there all the same, so the lack of
/// one is not evidence it is missing. Each direction is asked only what it can be sure of.
fn true_type_cids(
    doc: &Document,
    descendant: &Object,
    descriptor: &Object,
    listed: &BTreeSet<u32>,
    name: &str,
    findings: &mut Vec<AuditFinding>,
) {
    let arena = doc.arena();
    let Some(program) =
        entry(arena, descriptor, "FontFile2").and_then(|f| doc.decode_stream(&f).ok())
    else {
        return;
    };
    let (Some(slots), Some(outlined)) = (
        program_glyphs::sfnt_glyph_count(&program),
        program_glyphs::sfnt_outlined_glyphs(&program),
    ) else {
        return;
    };
    let map = GidMap::of(doc, descendant);
    let held: BTreeSet<u32> = map
        .cids(slots)
        .filter(|cid| *cid != 0 && map.glyph(*cid).is_some_and(|g| g != 0 && outlined.contains(&g)))
        .collect();
    let lacking: BTreeSet<u32> =
        listed.iter().copied().filter(|cid| map.glyph(*cid).is_none_or(|g| g >= slots)).collect();
    if let Some(first) = held.difference(listed).next() {
        let count = held.difference(listed).count();
        findings.push(broken(
            "31-014",
            format!(
                "/{name}: its program holds {count} CIDs its /CIDSet leaves out, {first} first"
            ),
        ));
    }
    if let Some(first) = lacking.first() {
        findings.push(broken(
            "31-015",
            format!(
                "/{name}: its /CIDSet lists {} CIDs its program lacks, {first} first",
                lacking.len()
            ),
        ));
    }
}

/// A TrueType CIDFont's `/CIDToGIDMap`: a stream of two bytes a CID, or — `/Identity`, or
/// absent — each CID to the glyph of the same index (Table 117).
struct GidMap(Option<Vec<u8>>);

impl GidMap {
    fn of(doc: &Document, descendant: &Object) -> Self {
        Self(match entry(doc.arena(), descendant, "CIDToGIDMap") {
            Some(stream @ Object::Stream(..)) => {
                doc.decode_stream(&stream).ok().map(|b| b.to_vec())
            }
            _ => None,
        })
    }

    /// The glyph `cid` maps to, when the map reaches it.
    fn glyph(&self, cid: u32) -> Option<u16> {
        match &self.0 {
            None => u16::try_from(cid).ok(),
            Some(bytes) => {
                let at = usize::try_from(cid).ok()? * 2;
                bytes.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]))
            }
        }
    }

    /// Every CID the map says anything about, given the program's glyph count.
    fn cids(&self, slots: u16) -> std::ops::Range<u32> {
        match &self.0 {
            None => 0..u32::from(slots),
            Some(bytes) => 0..u32::try_from(bytes.len() / 2).unwrap_or(0),
        }
    }
}
