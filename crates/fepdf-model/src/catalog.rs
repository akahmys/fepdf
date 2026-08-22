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
use crate::decrypt::Credentials;
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
///
/// `Typed` used to be one level, and it hid the thing this report exists to show. Once
/// every Table 29 key had a field, `inspect catalog` reported `untyped: 0` on every file
/// — a tool whose module doc opens "The point is the gaps" showing none. The count had
/// gone from 15 to 32 while the number of entries the engine could say anything about
/// moved by one. Declaring a key and modelling what it holds are different achievements
/// and are now counted apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Support {
    /// A field of [`PdfCatalog`] whose type says what the entry *holds* —
    /// `Option<ViewerPreferences>`, `Option<DestsDictionary>`, `Option<PageMode>`. The
    /// engine can reason about the value, not merely reach it.
    Modelled,
    /// A field of [`PdfCatalog`] typed `Object` or a bare arena handle. The entry is
    /// reachable by name and round-trips, and its contents are exactly as opaque as they
    /// were before the field existed: `Option<Object>` hands back whatever the arena
    /// already held. Worth having — a caller can find it without walking the raw
    /// dictionary — and worth not counting as understanding.
    Declared,
    /// Not a field, but a spec type for the entry's contents exists. Nothing names
    /// the key, so it is neither read nor written whatever the type suggests —
    /// `ARCHITECTURE.md` §5.2 calls this building a container before its contents
    /// exist, and this makes it visible per entry.
    TypeOnly,
    /// Not a field at all. The arena preserves the entry, so it round-trips; anything
    /// the engine does with it is ad hoc — reached by walking the raw dictionary for one
    /// key at a time. `/ViewerPreferences` and `/Lang` were the examples here until they
    /// were modelled, and what that looked like is worth keeping: `viewer_direction`
    /// resolved `/Root`, then `/ViewerPreferences`, then `/Direction`, and returned the
    /// name as a `String` — so a caller could not tell a value the standard defines from
    /// one it does not, and the other seventeen entries of Table 147 had no reader.
    Untyped,
}

impl Support {
    /// Whether the engine can say anything about the entry's *contents*.
    #[must_use]
    pub fn models_contents(self) -> bool {
        matches!(self, Self::Modelled)
    }
}

/// The arena's own types. A field of one of these names the entry and returns what was
/// already there; anything else is a type this engine wrote to describe the contents.
///
/// Listed rather than inferred because there is no way to ask the compiler "is this a
/// domain type", and the set is small, closed and belongs to this crate.
const PASSTHROUGH_TYPES: &[&str] = &[
    "Object",
    "Handle<Object>",
    "Handle<PdfName>",
    "Handle<Vec<Object>>",
    "Vec<Object>",
    // `Located<T>` pairs a value with the handle it was read from, so `Located<Object>`
    // is an `Object` that has learnt where it lives — reachable, and no more legible
    // than before. Listed so the wrapper cannot become a way to have a passthrough
    // classified as a reader.
    "Located<Object>",
];

/// Whether a field's declared type describes the entry's contents.
fn models_contents(rust_type: &str) -> bool {
    let inner =
        rust_type.strip_prefix("Option<").and_then(|t| t.strip_suffix('>')).unwrap_or(rust_type);
    !PASSTHROUGH_TYPES.contains(&inner)
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

/// The Table 29 keys that occur in **no file of either corpus**, and are therefore
/// declined a reader.
///
/// Measured 2026-08-22 by running `inspect catalog` over all 524 files — the nine in
/// `samples/` and the 515 of `scripts/test/fetch_external_corpus.sh` — and folding the
/// results together. Twenty-two of Table 29's thirty-two keys occur; these ten do not.
///
/// **Two keys left this list when the corpus grew**, which is what the paragraph below
/// says such a departure means. `/AF` went from zero files to seventeen the moment
/// PDF/A-3 and PDF/A-4f files were fetched — those parts of the standard exist to embed
/// other documents — and `/PieceInfo` to one. Both gained readers in the same change:
/// the rule is that a corpus can justify *building*, and these two are now built rather
/// than declined.
///
/// The list is here rather than in a document because it is a *refusal*, and a refusal
/// that lives only in prose is one nobody is reminded of. Building a reader for one of
/// these would be a container before its contents — the shape Phase D was ordered to
/// avoid and [ADR-0017](../../docs/adr/0017-declaring-a-catalogue-key-is-not-modelling-it.md)
/// records the cost of. `the_keys_no_file_carries_did_not_gain_readers` holds the line.
///
/// A key leaving this list is a *finding*: it means a corpus arrived that presents
/// something these two do not, and the case for reading it has changed.
///
/// A key **staying** on it can still gain a reader, and `BUILT_FOR_A_USE_CASE` is where
/// that is written down. The list records a measurement — no file carries this — and a
/// measurement cannot forbid building something; that is the rule Phase L cost this
/// project. What the list forbids is building on *no* reason at all.
pub const ABSENT_FROM_BOTH_CORPORA: &[&str] = &[
    "Extensions",
    "URI",
    "SpiderInfo",
    "Perms",
    "Legal",
    "Requirements",
    "Collection",
    "NeedsRendering",
    "DSS",
    "DPartRoot",
];

/// Keys no corpus file carries that were built anyway, because a use case named them.
///
/// One so far. `/Requirements` (12.11) is how a document declares it needs a capability
/// the processor may not have, and this engine deliberately does not execute ECMAScript
/// (12.6.4.17) — a subset 6.3.2.1 lets a processor decline. Reading `/Requirements` is
/// the difference between declining it and declining it in silence, and no corpus was
/// going to supply that argument.
pub const BUILT_FOR_A_USE_CASE: &[(&str, &str)] =
    &[("Requirements", "a document declares the subsets it needs; this engine declines one")];

/// The support level for one key: typed if `PdfCatalog` declares it, otherwise
/// whatever Table 29 above says about a type existing for its contents.
fn support_for(key: &str) -> Support {
    if let Some((_, rust_type)) = PdfCatalog::pdf_key_types().iter().find(|(k, _)| *k == key) {
        return if models_contents(rust_type) { Support::Modelled } else { Support::Declared };
    }
    match TABLE_29.iter().find(|(k, _)| *k == key) {
        Some((_, true)) => Support::TypeOnly,
        Some((_, false)) | None => Support::Untyped,
    }
}

/// How much of a *modelled* entry's own table the reader for it covers.
///
/// The question ADR-0017 asked about the catalogue, asked one level down. Phase K took
/// the catalogue from 6 modelled entries to 20, and "modelled" there means the reader
/// reads that entry's scalars — not that everything beneath it is understood.
/// `/AcroForm` reads `/NeedAppearances` and `/SigFlags` and leaves `/DR`, a resource
/// dictionary, as an `Object`. Without this the headline figure would say 19 of 20 and
/// invite exactly the reading ADR-0017 was written to prevent.
///
/// `None` for an entry whose reader is not a fixed set of keys: a number tree, a name
/// tree, an XMP packet, or a value that is one of two shapes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InnerCoverage {
    /// Keys of the entry's own table that the reader models the contents of.
    pub modelled: usize,
    /// Keys the reader names at all.
    pub declared: usize,
}

/// The reader for each modelled entry whose table is a fixed set of keys.
///
/// A list, because there is no way to ask the compiler "which type reads this key" —
/// the field's type is a string by the time `pdf_key_types` reports it. It is checked
/// rather than trusted: `every_entry_with_a_fixed_table_is_listed` fails when a
/// modelled entry gains a reader and nobody adds it here.
type TableKeys = fn() -> &'static [(&'static str, &'static str)];
const ENTRY_TABLES: &[(&str, TableKeys)] = &[
    ("AcroForm", <crate::document::entries::AcroForm as PdfSchema>::pdf_key_types),
    ("AA", <crate::document::entries::AdditionalActions as PdfSchema>::pdf_key_types),
    ("MarkInfo", <crate::document::entries::MarkInfo as PdfSchema>::pdf_key_types),
    ("OCProperties", <crate::document::entries::OptionalContent as PdfSchema>::pdf_key_types),
    ("Outlines", <crate::document::entries::OutlineRoot as PdfSchema>::pdf_key_types),
    ("Pages", <crate::document::entries::PageTreeRoot as PdfSchema>::pdf_key_types),
    ("StructTreeRoot", <crate::document::entries::StructTreeRoot as PdfSchema>::pdf_key_types),
    ("ViewerPreferences", <crate::document::ViewerPreferences as PdfSchema>::pdf_key_types),
];

/// How much of `key`'s own table its reader covers, when that table is a fixed set.
fn inner_coverage(key: &str) -> Option<InnerCoverage> {
    let (_, keys) = ENTRY_TABLES.iter().find(|(k, _)| *k == key)?;
    let types = keys();
    let modelled = types.iter().filter(|(_, t)| models_contents(t)).count();
    Some(InnerCoverage { modelled, declared: types.len() - modelled })
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
    /// For a modelled entry whose own table is a fixed set of keys, how much of that
    /// table the reader covers. See [`InnerCoverage`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inner: Option<InnerCoverage>,
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
    /// The catalogue as the engine *read* it, when it reads as Table 29.
    ///
    /// The entries above say which keys are present and how far the engine can go with
    /// each; this is how far it actually went on this file. `None` when the catalogue
    /// does not parse — which is itself reported, by the audit, with the entry named.
    ///
    /// It exists so a round trip can be checked on contents rather than on shape:
    /// `crosscheck_selfread.sh` could compare `dictionary[3]` with `dictionary[3]` and
    /// nothing finer while 26 of the 32 entries were `Option<Object>`.
    ///
    /// Written but not read back: `skip_deserializing`, because a `PdfCatalog` is a view
    /// of an arena and reconstructing one from JSON would be a second, unowned copy of
    /// the document's state — the thing ADR-0013 exists to prevent.
    #[serde(skip_serializing_if = "Option::is_none", default, skip_deserializing)]
    pub reading: Option<PdfCatalog>,
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
        crate::decrypt::unlock_raw(&raw, Credentials::default(), &mut decisions)?;
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
                inner: inner_coverage(&key),
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

        let reading =
            <PdfCatalog as crate::object::FromPdfObject>::from_pdf_object(root, &raw.arena).ok();
        Ok(Self { entries, absent, decisions: decisions.entries(), reading })
    }

    /// How many present entries fall at each level of support: modelled, declared,
    /// type-only, untyped.
    #[must_use]
    pub fn support_counts(&self) -> (usize, usize, usize, usize) {
        let n = |s: Support| self.entries.iter().filter(|e| e.support == s).count();
        (n(Support::Modelled), n(Support::Declared), n(Support::TypeOnly), n(Support::Untyped))
    }

    /// Present entries whose contents the engine cannot say anything about — what
    /// `ROADMAP.md` means by an entry that survives a round trip but cannot be reasoned
    /// about, for one specific file.
    ///
    /// A `Declared` entry counts here. Its field makes the entry reachable; it does not
    /// make the value legible, and this list is about the value.
    #[must_use]
    pub fn unmodelled(&self) -> Vec<&CatalogEntry> {
        self.entries.iter().filter(|e| !e.support.models_contents()).collect()
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

    /// Naming an entry and modelling it are counted apart, and the difference is derived
    /// from the struct rather than listed here.
    ///
    /// This replaces a test that asserted all 32 entries were `Typed`. That assertion was
    /// true and locked in the overstatement it should have caught: 26 of the 32 fields are
    /// `Option<Object>`, which returns what the arena already held. No ratio is asserted
    /// below — a ratio would fail on progress — only that the distinction is live and that
    /// the two counts still add up to what the struct declares.
    #[test]
    fn declaring_a_key_is_counted_apart_from_modelling_it() {
        let coverage = CatalogReport::coverage();
        let count = |want: Support| coverage.iter().filter(|(_, s)| *s == want).count();
        let modelled = count(Support::Modelled);
        let declared = count(Support::Declared);

        assert_eq!(
            modelled + declared,
            PdfCatalog::pdf_key_types().len(),
            "coverage must agree with the struct"
        );
        assert_eq!(modelled + declared, 32, "every Table 29 key has a field");
        assert!(modelled > 0 && declared > 0, "both levels must be reachable to be a signal");

        // The two ends of the distinction, by name, so a change to the classifier that
        // collapsed it would fail here rather than quietly report full coverage again.
        assert_eq!(support_for("ViewerPreferences"), Support::Modelled, "a domain type");
        assert_eq!(support_for("PageMode"), Support::Modelled);
        assert_eq!(support_for("Dests"), Support::Modelled);
        assert_eq!(support_for("DSS"), Support::Declared, "Option<Object>, zero in the corpus");
        assert_eq!(support_for("DPartRoot"), Support::Declared);
        assert_eq!(support_for("Metadata"), Support::Modelled, "Phase K: the XMP packet");

        // `/Type` is the one key the corpus carries that stays `Declared`, and it is
        // deliberate: 7.7.2 fixes its value at `/Catalog`, so a reader for it is an
        // assertion rather than a type, and it belongs with the audit.
        assert_eq!(support_for("Type"), Support::Declared);
    }

    /// The refusal holds: no key that neither corpus carries has a reader.
    ///
    /// This is the container rule as a test rather than as a paragraph. Phase K built
    /// fourteen readers, one for each entry the corpora present, and stopped — and the
    /// way to be sure it stopped is to check the other twelve from the other side.
    #[test]
    fn the_keys_no_file_carries_did_not_gain_readers() {
        for key in ABSENT_FROM_BOTH_CORPORA {
            assert!(
                TABLE_29.iter().any(|(k, _)| k == key),
                "/{key} is not a Table 29 key, so the list has drifted"
            );
            if let Some((_, why)) = BUILT_FOR_A_USE_CASE.iter().find(|(k, _)| k == key) {
                // Built on a reason that is not a count, which the list's own note allows.
                assert_eq!(
                    support_for(key),
                    Support::Modelled,
                    "/{key} is listed as built for a use case ({why}) and has no reader"
                );
                continue;
            }
            if *key == "NeedsRendering" {
                // The one exception, and it predates the rule being enforced here:
                // ADR-0017 records it as the single field added in that session whose
                // type describes what it holds — a `bool` — for an entry no file carries.
                // Kept visible rather than quietly excluded.
                assert_eq!(support_for(key), Support::Modelled);
                continue;
            }
            assert_eq!(
                support_for(key),
                Support::Declared,
                "/{key} occurs in no file of either corpus and must not have a reader"
            );
        }
        assert_eq!(ABSENT_FROM_BOTH_CORPORA.len(), 10);
    }

    /// Every key the corpora *do* carry has a reader, save the one that is a check.
    #[test]
    fn every_key_the_corpora_carry_is_modelled_except_the_one_that_is_an_assertion() {
        let unread: Vec<&str> = TABLE_29
            .iter()
            .map(|(k, _)| *k)
            .filter(|k| !ABSENT_FROM_BOTH_CORPORA.contains(k))
            .filter(|k| support_for(k) != Support::Modelled)
            .collect();
        assert_eq!(
            unread,
            vec!["Type"],
            "7.7.2 fixes /Type at /Catalog, so a reader for it is an assertion; \
             anything else here is an entry the corpus presents and nothing reads"
        );
    }

    /// Every modelled entry whose table is a fixed set of keys is listed in
    /// `ENTRY_TABLES`, so the nested figure cannot quietly stop covering one.
    ///
    /// The exceptions are named rather than skipped: a number tree, a name tree, an XMP
    /// packet, an array of dictionaries, a value that is one of two shapes, and two
    /// scalars. None of those is a table of keys, so there is nothing to report a
    /// fraction of.
    #[test]
    fn every_entry_with_a_fixed_table_is_listed() {
        const NOT_A_TABLE_OF_KEYS: &[&str] = &[
            "PageLabels",     // a number tree (7.9.7)
            "Names",          // ten name trees (7.9.6)
            "Metadata",       // an XMP packet, which is XML
            "OpenAction",     // a destination or an action
            "OutputIntents",  // an array of them
            "Threads",        // an array of them
            "Dests",          // keyed by the destination names themselves
            "PageMode",       // a name
            "PageLayout",     // a name
            "Lang",           // a text string
            "Version",        // a name of the form M.m
            "NeedsRendering", // a boolean
            "AF",             // an array of file specifications (7.11.3)
            "PieceInfo",      // keyed by the names applications call themselves (14.5)
            "Requirements",   // an array of requirement dictionaries (12.11)
        ];
        for (key, _) in TABLE_29 {
            if support_for(key) != Support::Modelled || NOT_A_TABLE_OF_KEYS.contains(key) {
                continue;
            }
            assert!(
                inner_coverage(key).is_some(),
                "/{key} is modelled and its table is a fixed set of keys, so ENTRY_TABLES \
                 must name its reader"
            );
        }
    }

    /// The nested figure says what the headline cannot: `/AcroForm` is a modelled
    /// catalogue entry that reads **four of its own eight**.
    ///
    /// The expectation written here first was two, on the reasoning that `/DR` and
    /// `/XFA` were the opaque ones. The measurement said four: `/Fields` and `/CO` are
    /// arrays of objects, named and no more legible than that. Being wrong about it by a
    /// factor of two, in the file that defines the classifier, is the argument for
    /// deriving the figure rather than asserting it in a paragraph.
    #[test]
    fn a_modelled_entry_reports_how_much_of_its_own_table_it_reads() {
        let form = inner_coverage("AcroForm").expect("listed");
        assert_eq!(form.modelled + form.declared, 8, "Table 224 has eight entries");
        assert_eq!(form.modelled, 4, "/NeedAppearances, /SigFlags, /DA and /Q");
        assert_eq!(form.declared, 4, "/Fields, /CO, /DR and /XFA are named, not read");

        let mark = inner_coverage("MarkInfo").expect("listed");
        assert_eq!(mark.declared, 0, "three booleans, all read");
    }

    /// The classifier reads the type as the macro writes it, whitespace already stripped.
    #[test]
    fn the_arenas_own_types_do_not_count_as_modelling() {
        assert!(!models_contents("Option<Object>"));
        assert!(!models_contents("Object"));
        assert!(!models_contents("Option<Handle<Object>>"));
        assert!(!models_contents("Handle<Object>"));
        assert!(!models_contents("Option<Handle<PdfName>>"));

        assert!(models_contents("Option<ViewerPreferences>"));
        assert!(models_contents("Option<PageMode>"));
        assert!(models_contents("Option<String>"), "a text string is 7.9.2.2, not the arena");
        assert!(models_contents("Option<bool>"));
    }
}
