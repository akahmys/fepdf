//! Every entry of the document catalogue (7.7.2), typed or not.
//!
//! The point is the gaps. `ROADMAP.md` says untyped entries survive a round trip but
//! cannot be reasoned about; this makes that concrete per file, so "what does the
//! engine not understand about this document" has an answer that is not a guess.
//!
//! The catalogue is not a Rust type in this engine — it is a dictionary in the arena,
//! reached through `/Root`. That is deliberate: an entry with no typed view still
//! round-trips, which would not survive being modelled as a struct with named fields.

use crate::arena::PdfArena;
use crate::document::PdfCatalog;
use crate::error::PdfResult;
use crate::handle::Handle;
use crate::interpretation::Decision;
use crate::object::PdfSchema;
use crate::object::{Object, PdfName};
use crate::reader;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How far the engine can go with a catalogue entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Support {
    /// A field of [`PdfCatalog`], so the entry has a typed view. Derived from the
    /// struct's `#[pdf_key]` attributes, never asserted here.
    Typed,
    /// No typed view, but a spec type for the entry's contents exists. Nothing names
    /// the key, so it is neither read nor written whatever the type suggests —
    /// `ARCHITECTURE.md` §5.2 calls this building a container before its contents
    /// exist, and this makes it visible per entry.
    TypeOnly,
    /// No typed view. The arena preserves the entry, so it round-trips; anything the
    /// engine does with it is ad hoc — `/ViewerPreferences` and `/Lang` are read
    /// directly in `fepdf-sdk`, for instance, without passing through a type.
    Untyped,
}

/// Every key ISO 32000-2 Table 29 defines for the document catalogue, and whether a
/// type for its *contents* exists anywhere in the engine.
///
/// Whether the entry is *typed* is not recorded here — [`PdfCatalog::pdf_keys`]
/// answers that, so the two cannot drift. This list only distinguishes "a type for
/// this exists but nothing reads the key" from "nothing at all", which is a judgement
/// about the crate's surface rather than a fact the compiler holds.
const TABLE_29: &[(&str, bool)] = &[
    ("Type", false),
    ("Version", false),
    ("Extensions", false),
    ("Pages", false),
    ("PageLabels", true), // PageLabelSpec
    ("Names", false),
    ("Dests", false),
    ("ViewerPreferences", false),
    ("PageLayout", false),
    ("PageMode", false),
    ("Outlines", false),
    ("Threads", true), // ArticleThread
    ("OpenAction", false),
    ("AA", false),
    ("URI", false),
    ("AcroForm", true), // FormFieldSpec
    ("Metadata", false),
    ("StructTreeRoot", false),
    ("MarkInfo", false),
    ("Lang", false),
    ("SpiderInfo", false),
    ("OutputIntents", true), // OutputIntent
    ("PieceInfo", false),
    ("OCProperties", true), // OptionalContentProperties
    ("Perms", false),
    ("Legal", false),
    ("Requirements", false),
    ("Collection", true), // PortfolioCollection
    ("NeedsRendering", false),
    ("DSS", false),
    ("AF", true), // AssociatedFile
    // No type, despite `ROADMAP.md` having listed it beside `DSS` and `AF` as one of
    // the 2.0 additions that "have spec types". Checked: there is no `DPartRoot`,
    // `DPart` or `DocumentPart` anywhere in the workspace.
    ("DPartRoot", false),
];

/// The support level for one key: typed if `PdfCatalog` declares it, otherwise
/// whatever Table 29 above says about a type existing for its contents.
fn support_for(key: &str) -> Support {
    if PdfCatalog::pdf_keys().contains(&key) {
        return Support::Typed;
    }
    match TABLE_29.iter().find(|(k, _)| *k == key) {
        Some((_, true)) => Support::TypeOnly,
        Some((_, false)) | None => Support::Untyped,
    }
}

/// One catalogue entry as this file writes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// The key, without its leading slash.
    pub key: String,
    /// What the value is, in one phrase — enough to tell a dictionary from a name.
    pub value: String,
    /// Whether Table 29 defines this key.
    pub standard: bool,
    /// How far the engine can go with it.
    pub support: Support,
}

/// The catalogue of one document, and what the engine can make of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogReport {
    /// Entries this file's catalogue carries, in key order.
    pub entries: Vec<CatalogEntry>,
    /// Table 29 keys the file does not carry.
    pub absent: Vec<String>,
    /// What the engine decided while reading this file (§5.3). Carried by every
    /// report so that "how was this read" travels with "what was found".
    pub decisions: Vec<Decision>,
}

impl CatalogReport {
    /// Reads `bytes` and reports its catalogue.
    ///
    /// # Errors
    /// Fails when the file cannot be read, or names no catalogue to report.
    pub fn survey(bytes: &[u8]) -> PdfResult<Self> {
        let raw = reader::load_document(bytes)?;
        // Pass 0, as `Document::open` runs it. Without this the report describes the
        // file's *ciphertext*: `samples/unicode_16.pdf` listed `/Lang` as a 32-byte
        // string, which is one AES block and an IV, not a language tag.
        let mut decisions = raw.decisions.clone();
        crate::decrypt::unlock(&raw.arena, raw.trailer, "", &mut decisions)?;
        let root = raw
            .trailer
            .and_then(|t| raw.arena.get_dict(t))
            .and_then(|d| d.get(&raw.arena.name("Root")).cloned())
            .ok_or_else(|| crate::error::PdfError::Arena("the file names no /Root".into()))?;
        let dict = resolve_dict(&raw.arena, &root).ok_or_else(|| {
            crate::error::PdfError::Arena("/Root does not resolve to a dictionary".into())
        })?;

        let mut entries = Vec::new();
        for (name, value) in &dict {
            let Some(key) = raw.arena.get_name_str(*name) else { continue };
            entries.push(CatalogEntry {
                value: describe(&raw.arena, value),
                standard: TABLE_29.iter().any(|(k, _)| *k == key),
                support: support_for(&key),
                key,
            });
        }
        entries.sort_by(|a, b| a.key.cmp(&b.key));

        let present: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        let absent = TABLE_29
            .iter()
            .filter(|(k, _)| !present.contains(k))
            .map(|(k, _)| (*k).to_string())
            .collect();

        Ok(Self { entries, absent, decisions: decisions.entries().to_vec() })
    }

    /// How many present entries fall at each level of support.
    #[must_use]
    pub fn support_counts(&self) -> (usize, usize, usize) {
        let n = |s: Support| self.entries.iter().filter(|e| e.support == s).count();
        (n(Support::Typed), n(Support::TypeOnly), n(Support::Untyped))
    }

    /// Present entries with no typed view — what `ROADMAP.md` means by an entry that
    /// survives a round trip but cannot be reasoned about, for one specific file.
    #[must_use]
    pub fn untyped(&self) -> Vec<&CatalogEntry> {
        self.entries.iter().filter(|e| e.support != Support::Typed).collect()
    }

    /// Every Table 29 key, with the support the engine claims for it. Independent of
    /// any file, so a caller can see the whole gap rather than one document's slice.
    #[must_use]
    pub fn coverage() -> Vec<(String, Support)> {
        TABLE_29.iter().map(|(k, _)| ((*k).to_string(), support_for(k))).collect()
    }
}

/// One phrase for a value: enough to tell a dictionary from a name from an array.
fn describe(arena: &PdfArena, object: &Object) -> String {
    match object {
        Object::Reference(h) => match arena.get_object(*h) {
            Some(inner) => format!("{} 0 R -> {}", h.index(), describe(arena, &inner)),
            None => format!("{} 0 R -> missing", h.index()),
        },
        Object::Dictionary(h) => arena
            .get_dict(*h)
            .map_or_else(|| "dictionary".into(), |d| format!("dictionary[{}]", d.len())),
        Object::Array(h) => {
            arena.get_array(*h).map_or_else(|| "array".into(), |a| format!("array[{}]", a.len()))
        }
        Object::Name(h) => {
            arena.get_name_str(*h).map_or_else(|| "name".into(), |n| format!("/{n}"))
        }
        Object::String(b) => format!("string[{}]", b.len()),
        Object::Hex(b) => format!("hex string[{}]", b.len()),
        Object::Text(t) => format!("text[{}]", t.chars().count()),
        Object::Integer(n) => n.to_string(),
        Object::Real(r) => r.to_string(),
        Object::Boolean(b) => b.to_string(),
        Object::Stream(_, _) => "stream".into(),
        Object::Null => "null".into(),
    }
}

fn resolve_dict(arena: &PdfArena, object: &Object) -> Option<BTreeMap<Handle<PdfName>, Object>> {
    match object {
        Object::Dictionary(h) => arena.get_dict(*h),
        Object::Reference(h) => match arena.get_object(*h)? {
            Object::Dictionary(d) => arena.get_dict(d),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_29_has_every_key_once() {
        let mut keys: Vec<&str> = TABLE_29.iter().map(|(k, _)| *k).collect();
        let before = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), before, "a key is listed twice");
        assert_eq!(before, 32, "ISO 32000-2 Table 29 defines 32 entries");
    }

    #[test]
    fn coverage_reports_more_gaps_than_support() {
        // The claim this command exists to make checkable: most of the catalogue is
        // not understood. If this ever inverts, the roadmap's framing has changed and
        // the documents need revisiting rather than this assertion relaxing.
        let coverage = CatalogReport::coverage();
        let typed = coverage.iter().filter(|(_, s)| *s == Support::Typed).count();
        assert_eq!(typed, PdfCatalog::pdf_keys().len(), "coverage must agree with the struct");
        assert!(typed < coverage.len() / 2, "{typed} of {} are typed", coverage.len());
    }
}
