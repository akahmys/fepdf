//! What the catalogue's entries *hold* (7.7.2, Table 29).
//!
//! Every key of Table 29 has been a field of [`crate::document::PdfCatalog`] since Phase
//! D, and 26 of the 32 were `Option<Object>`: reachable by name, contents as opaque as
//! before the field existed ([ADR-0017](../../../docs/adr/0017-declaring-a-catalogue-key-is-not-modelling-it.md)).
//! This module is the other half — a reader for each entry the corpora actually present,
//! built in the order they present it:
//!
//! | Files | Entry | |
//! | ---: | :--- | :--- |
//! | 217 | `/Outlines` | [`OutlineRoot`] |
//! | 210 | `/Metadata` | [`XmpMetadata`] |
//! | 208 | `/OpenAction` | [`TriggeredAction`] |
//! | 64 | `/OutputIntents` | [`OutputIntent`] |
//! | 34 | `/Names` | [`NameDictionary`] |
//! | 5 | `/AcroForm` | [`AcroForm`] |
//! | 4 | `/MarkInfo`, `/PageLabels`, `/StructTreeRoot` | [`MarkInfo`], [`PageLabels`], [`StructTreeRoot`] |
//! | 3 | `/Version` | [`DeclaredVersion`] |
//! | 1 | `/AA`, `/OCProperties`, `/Threads` | [`AdditionalActions`], [`OptionalContent`], [`ArticleThread`] |
//!
//! **Twelve keys occur in no file of either corpus** — `/Extensions`, `/URI`,
//! `/SpiderInfo`, `/PieceInfo`, `/Perms`, `/Legal`, `/Requirements`, `/Collection`,
//! `/NeedsRendering`, `/DSS`, `/AF` and `/DPartRoot` — and none of them gets a reader
//! here. Building one would be a container before its contents, which is the shape Phase
//! D was ordered to avoid and ADR-0017 records the cost of.
//!
//! **What "modelled" means at this level, and what it does not.** These types read the
//! *scalars* of their table and name the sub-objects that hang off it. `/AcroForm` reads
//! `/NeedAppearances`, `/SigFlags`, `/DA` and `/Q`, and leaves `/DR` — a resource
//! dictionary, a subsystem — as an `Object` that a caller can reach. Saying a catalogue
//! entry is modelled is therefore a statement about that entry, not a claim that
//! everything beneath it is understood; each field below says which of the two it is.
//!
//! The types in [`super::extensions`] are **not** these readers, and the difference is
//! not a detail: `OutputIntent` there carries `icc_profile_bytes: Option<Vec<u8>>` and
//! `AssociatedFile` carries `data: Vec<u8>`, because they are arguments to an
//! `Operation` that *writes* one. What a reader returns is a view of a dictionary that
//! is already in the arena.

use crate::arena::PdfArena;
use crate::error::{PdfError, PdfResult};
use crate::handle::Handle;
use crate::object::{FromPdfObject, Object, PdfName};
use fepdf_macros::FromPdfObject;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// `/MarkInfo` (14.7.1, Table 321): whether the document is tagged.
///
/// Three booleans, and every one is an `Option`. A document that does not write
/// `/Suspects` has said nothing about suspect content, which is not the same as writing
/// `false` — the distinction `ViewerPreferences` makes for Table 147, for the reason
/// ADR-0008 gives: a reader that defaults cannot report what the file declares.
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "14.7.1")]
pub struct MarkInfo {
    #[pdf_key("Marked")]
    /// `/Marked`: the document conforms to Tagged PDF.
    pub marked: Option<bool>,
    #[pdf_key("UserProperties")]
    /// `/UserProperties`: some structure elements carry user properties (14.6.3).
    pub user_properties: Option<bool>,
    #[pdf_key("Suspects")]
    /// `/Suspects`: the tag structure may not be reliable, so a reader should not trust
    /// it for reflow or reading order.
    pub suspects: Option<bool>,
}

/// `/Version` (7.7.2): a version that overrides the header's.
///
/// The header says `%PDF-1.4` and this says `/2.0`, and the later of the two wins — so a
/// reader that ignores it reads a 2.0 file as 1.4. Parsed into its two numbers rather
/// than kept as a name, because comparing `"1.10"` with `"1.9"` as text gets the answer
/// wrong.
///
/// A name that is not `M.m` becomes [`DeclaredVersion::Other`] rather than an error, and
/// that choice matters more than it looks: this is a *catalogue* entry, and a reader that
/// refused it would refuse the catalogue, which would refuse the document. `PageMode`
/// settled the same question the same way — keep what the file wrote, in a variant that
/// says it was not one of the defined values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeclaredVersion {
    /// A version the clause defines the form of: major and minor.
    Numbered {
        /// The major version — 1 or 2 in every edition published so far.
        major: u8,
        /// The minor version.
        minor: u8,
    },
    /// A name that is not `M.m`, kept verbatim.
    Other(String),
}

impl DeclaredVersion {
    /// The two numbers, when the entry had that form.
    #[must_use]
    pub fn numbers(&self) -> Option<(u8, u8)> {
        match self {
            Self::Numbered { major, minor } => Some((*major, *minor)),
            Self::Other(_) => None,
        }
    }
}

impl FromPdfObject for DeclaredVersion {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let name = PdfName::from_pdf_object(obj, arena)?;
        let text = name.as_str();
        let parsed = text.split_once('.').and_then(|(major, minor)| {
            Some(Self::Numbered { major: major.parse().ok()?, minor: minor.parse().ok()? })
        });
        Ok(parsed.unwrap_or_else(|| Self::Other(text.to_string())))
    }
}

/// One catalogue entry, read without requiring the rest of the catalogue to be legible.
///
/// Asking a `PdfCatalog` for `/Pages` means reading all 32 entries of Table 29, and any
/// one of them failing fails the lot. That was fatal in two places: enumerating a
/// document's pages depended on its `/Version` parsing, and auditing its structure tree
/// depended on `/MarkInfo` being a dictionary — so `/MarkInfo 42` made the whole file
/// refuse to open, over a question about tagging.
///
/// The entry is read through the same reader the catalogue uses, so this is neither ad
/// hoc nor a second implementation. `Ok(None)` means the catalogue does not carry it.
///
/// # Errors
/// Fails when the catalogue is not a dictionary, or when the entry is present and will
/// not read as `T`.
pub fn entry<T: FromPdfObject>(
    arena: &PdfArena,
    catalog: &Object,
    key: &str,
) -> PdfResult<Option<T>> {
    let dict = dict_of(arena, catalog)
        .ok_or_else(|| PdfError::Other("the catalogue is not a dictionary (7.7.2)".into()))?;
    match dict.get(&arena.name(key)) {
        Some(value) => T::from_pdf_object(value.clone(), arena).map(Some),
        None => Ok(None),
    }
}

/// `/AcroForm` (12.7.2, Table 224): the interactive form.
///
/// The fields themselves are walked by [`crate::interactive::FormFields`], which
/// descends `/Kids` and resolves 12.7.4.2's inheritance; what this adds is the form's
/// own settings, which decide how those fields are *drawn* and whether the file claims
/// to be signed.
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "12.7.2")]
pub struct AcroForm {
    #[pdf_key("Fields")]
    /// `/Fields`: the root fields. Named, not walked here — see
    /// [`crate::interactive::FormFields`], which needs the whole document to resolve
    /// what a field inherits.
    pub fields: Option<Handle<Vec<Object>>>,
    #[pdf_key("NeedAppearances")]
    /// `/NeedAppearances`: the viewer must build appearance streams from the field
    /// values, because the file's are missing or stale. `isartor-6-9-t01-fail-a.pdf`
    /// sets it, which is exactly what PDF/A forbids.
    pub need_appearances: Option<bool>,
    #[pdf_key("SigFlags")]
    /// `/SigFlags`: bit 1 says the document contains a signature field, bit 2 that
    /// appending a change would invalidate it (Table 225).
    pub signature_flags: Option<SignatureFlags>,
    #[pdf_key("CO")]
    /// `/CO`: the order field calculations run in. Named, not modelled.
    pub calculation_order: Option<Handle<Vec<Object>>>,
    #[pdf_key("DR")]
    /// `/DR`: the resources a field's `/DA` is written against — a resource dictionary,
    /// which is a subsystem rather than a value. Reachable, not modelled.
    pub default_resources: Option<Object>,
    #[pdf_key("DA")]
    /// `/DA`: the default appearance string, in the syntax of a content stream.
    pub default_appearance: Option<String>,
    #[pdf_key("Q")]
    /// `/Q`: the quadding — 0 left, 1 centred, 2 right.
    pub quadding: Option<i64>,
    #[pdf_key("XFA")]
    /// `/XFA`: the XML forms architecture, deprecated in 2.0. Reachable, not modelled,
    /// and deliberately: reading it would mean a second form model this edition is
    /// retiring.
    pub xfa: Option<Object>,
}

/// `/SigFlags` (Table 225), as flags rather than as an integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureFlags {
    /// Bit 1: the document has at least one signature field.
    pub signatures_exist: bool,
    /// Bit 2: the file must be saved as an incremental update, because rewriting it
    /// would invalidate a signature.
    pub append_only: bool,
}

impl FromPdfObject for SignatureFlags {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let bits = obj
            .resolve(arena)
            .as_integer()
            .ok_or_else(|| PdfError::Other("/SigFlags is a bit field".into()))?;
        Ok(Self { signatures_exist: bits & 1 != 0, append_only: bits & 2 != 0 })
    }
}

/// `/OutputIntents` (14.11.5, Table 401): what the file was prepared to be printed on.
///
/// 64 files of the external corpus carry one, which makes it the most common catalogue
/// entry this engine could not read anything out of. `/S` is the subtype —
/// `GTS_PDFA1`, `GTS_PDFX`, `ISO_PDFE1` — and it is how a file *claims* a standard;
/// the audit compares that claim with what the file actually does.
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "14.11.5")]
pub struct OutputIntent {
    #[pdf_key("Type")]
    /// `/Type`, `OutputIntent` when written.
    pub kind: Option<PdfName>,
    #[pdf_key("S")]
    /// `/S`: the subtype — which standard this intent is for.
    pub subtype: Option<PdfName>,
    #[pdf_key("OutputConditionIdentifier")]
    /// `/OutputConditionIdentifier`: the registered name of the condition, or `Custom`.
    pub condition_identifier: Option<String>,
    #[pdf_key("OutputCondition")]
    /// `/OutputCondition`: the condition in words, for a human.
    pub condition: Option<String>,
    #[pdf_key("RegistryName")]
    /// `/RegistryName`: where the identifier is registered.
    pub registry: Option<String>,
    #[pdf_key("Info")]
    /// `/Info`: more about the condition. Required when the identifier is not registered.
    pub info: Option<String>,
    #[pdf_key("DestOutputProfile")]
    /// `/DestOutputProfile`: the ICC profile stream itself. Named, not decoded — an ICC
    /// profile is a format of its own and nothing in this engine parses one.
    pub destination_profile: Option<Object>,
}

/// `/OutputIntents`: the array of them, with what would not read counted.
///
/// A wrapper rather than a bare `Vec`, for the reason `DestsDictionary` is one: an array
/// of 64 intents that silently became 63 because one would not parse is the more
/// convenient number and the less true one.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputIntents {
    /// Each intent, in the order the array writes them.
    pub intents: Vec<OutputIntent>,
    /// Elements present in the array but not readable as an intent.
    pub unreadable: usize,
}

impl FromPdfObject for OutputIntents {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        read_array(arena, &obj, "/OutputIntents")
            .map(|(intents, unreadable)| Self { intents, unreadable })
    }
}

/// `/Threads`: the articles, with what would not read counted.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArticleThreads {
    /// Each thread, in the order the array writes them.
    pub threads: Vec<ArticleThread>,
    /// Elements present in the array but not readable as a thread.
    pub unreadable: usize,
}

impl FromPdfObject for ArticleThreads {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        read_array(arena, &obj, "/Threads")
            .map(|(threads, unreadable)| Self { threads, unreadable })
    }
}

/// Reads an array of `T`, counting the elements that will not read rather than failing.
///
/// One malformed element out of 64 is a defect in that element. Refusing the whole entry
/// would lose the other 63 and report the catalogue as unreadable, which is the
/// all-or-nothing that `if let Ok(..)` cost this engine once already: a cross-reference
/// section that would not read took eleven objects with it and said nothing.
fn read_array<T: FromPdfObject>(
    arena: &PdfArena,
    obj: &Object,
    what: &str,
) -> PdfResult<(Vec<T>, usize)> {
    let items = array_of(arena, obj)
        .ok_or_else(|| PdfError::Other(format!("{what} is not an array").into()))?;
    let mut read = Vec::new();
    let mut unreadable = 0;
    for item in items {
        match T::from_pdf_object(item, arena) {
            Ok(value) => read.push(value),
            Err(_) => unreadable += 1,
        }
    }
    Ok((read, unreadable))
}

/// `/PageLabels` (12.4.2, Table 161): what a viewer shows instead of a page index.
///
/// A number tree, so the entries are ranges: "from page 0, lowercase roman; from page 4,
/// decimal starting at 1". Read as the ranges rather than as a label per page, because
/// that is what the file holds — a document of 846 pages carries two entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageLabels {
    /// Each range, by the page index it starts at, in order.
    pub ranges: Vec<PageLabelRange>,
    /// Entries in the tree that would not read as a label range. Counted rather than
    /// dropped, so a total cannot quietly mean "the ones that parsed".
    pub unreadable: usize,
}

/// One entry of the `/PageLabels` number tree (Table 161).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLabelRange {
    /// The zero-based index of the first page this range covers.
    pub from_page: i64,
    /// `/S`: the numbering style. Absent means the pages have no number, only `/P`.
    pub style: Option<PdfName>,
    /// `/P`: a prefix before the number.
    pub prefix: Option<String>,
    /// `/St`: the number the range starts at. 12.4.2 defaults it to 1, and it is left
    /// `None` when unwritten so that a caller can tell a default from a declaration.
    pub start_at: Option<i64>,
}

impl FromPdfObject for PageLabels {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let dict = dict_of(arena, &obj)
            .ok_or_else(|| PdfError::Other("/PageLabels is not a number tree".into()))?;
        let mut out = Self::default();
        let mut pairs = Vec::new();
        walk_number_tree(arena, &dict, &mut pairs, 0);
        for (index, value) in pairs {
            match dict_of(arena, &value) {
                Some(d) => out.ranges.push(PageLabelRange {
                    from_page: index,
                    style: name_at(arena, &d, "S"),
                    prefix: text_at(arena, &d, "P"),
                    start_at: d.get(&arena.name("St")).and_then(|v| v.resolve(arena).as_integer()),
                }),
                None => out.unreadable += 1,
            }
        }
        out.ranges.sort_by_key(|r| r.from_page);
        Ok(out)
    }
}

/// `/Threads` (12.4.3, Table 158): an article, as a sequence of beads across pages.
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "12.4.3")]
pub struct ArticleThread {
    #[pdf_key("Type")]
    /// `/Type`, `Thread` when written.
    pub kind: Option<PdfName>,
    #[pdf_key("F")]
    /// `/F`: the first bead. The beads form a ring, so this is the whole thread.
    /// Named, not walked — a bead points at a page, and following the ring from here is
    /// a document-level walk rather than a reading of this dictionary.
    pub first_bead: Option<Handle<Object>>,
    #[pdf_key("I")]
    /// `/I`: the thread's own information dictionary — title, author, subject.
    pub information: Option<Object>,
}

/// `/OCProperties` (8.11.4.3, Table 98): the optional content the document defines.
///
/// This reader existed through Phase K and **nothing consulted it**, which is what made
/// `BDC` discarding its property list invisible: the document's word on which layers were
/// off was reachable, and no drawing path asked. [`crate::optional_content`] is what asks
/// now, and it enters through here rather than walking the raw dictionary a second time.
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "8.11.4.3")]
pub struct OptionalContent {
    #[pdf_key("OCGs")]
    /// `/OCGs`: every optional content group in the document. Named, not walked — a
    /// group's *state* comes from the configuration below, not from this list, and the
    /// one thing the list decides is what `/BaseState /OFF` means.
    pub groups: Option<Handle<Vec<Object>>>,
    #[pdf_key("D")]
    /// `/D`: the default configuration — which groups are on when the file opens
    /// (Table 100). Modelled, because this entry alone decides what is drawn.
    pub default_configuration: Option<crate::optional_content::OptionalContentConfiguration>,
    #[pdf_key("Configs")]
    /// `/Configs`: alternative configurations a viewer may *offer*. Named, not modelled:
    /// offering one is a user interface, and applying one that the document did not make
    /// default would be this engine choosing which layers a reader sees.
    pub configurations: Option<Handle<Vec<Object>>>,
}

/// `/OpenAction` (12.6, Table 196) and the entries of `/AA`.
///
/// **Two shapes, and the ambiguity is the point.** `/OpenAction` is either a destination
/// array — go here when the file opens — or an action dictionary that does something.
/// 208 files carry one and the corpus writes both forms; a reader that assumed either
/// would report the other as absent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggeredAction {
    /// A destination written in place: open at this view of this page.
    Destination(Object),
    /// An action dictionary.
    Action(Action),
}

/// One action dictionary (12.6.3, Table 196).
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "12.6.3")]
pub struct Action {
    #[pdf_key("Type")]
    /// `/Type`, `Action` when written.
    pub kind: Option<PdfName>,
    #[pdf_key("S")]
    /// `/S`: what the action does — `GoTo`, `URI`, `JavaScript`, `Launch`.
    pub action: PdfName,
    #[pdf_key("Next")]
    /// `/Next`: actions to perform after this one, in order. Named, not followed: the
    /// chain can branch and revisit, and walking it belongs to whatever executes
    /// actions rather than to reading one.
    pub next: Option<Object>,
    #[pdf_key("D")]
    /// `/D`: where a `/GoTo` goes. Present on the two commonest action types and on no
    /// others, which is why it is read here rather than in a per-type reader.
    pub destination: Option<Object>,
    #[pdf_key("URI")]
    /// `/URI`: what a `/URI` action opens.
    pub uri: Option<String>,
}

impl FromPdfObject for TriggeredAction {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        match obj.resolve(arena) {
            Object::Array(_) => Ok(Self::Destination(obj)),
            Object::Dictionary(_) | Object::Stream(..) => {
                Action::from_pdf_object(obj, arena).map(Self::Action)
            }
            other => Err(PdfError::Other(
                format!("/OpenAction is a destination or an action, not {other:?}").into(),
            )),
        }
    }
}

/// `/AA` on the catalogue (12.6.3, Table 197): actions triggered by document events.
///
/// Four events, each an action. Named apart rather than kept as a dictionary because
/// *which* event fires is the information — a `/WC` that runs JavaScript before the
/// document closes is a different fact from a `/WS` that runs one before it prints.
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "12.6.3")]
pub struct AdditionalActions {
    #[pdf_key("WC")]
    /// `/WC`: before the document is closed.
    pub will_close: Option<Action>,
    #[pdf_key("WS")]
    /// `/WS`: before the document is saved.
    pub will_save: Option<Action>,
    #[pdf_key("DS")]
    /// `/DS`: after the document is saved.
    pub did_save: Option<Action>,
    #[pdf_key("WP")]
    /// `/WP`: before the document is printed.
    pub will_print: Option<Action>,
    #[pdf_key("DP")]
    /// `/DP`: after the document is printed.
    pub did_print: Option<Action>,
}

/// `/Outlines` (12.3.3, Table 152): the root of the bookmark tree.
///
/// `/Count` is **not** the number of items: 12.3.3 defines it as the number *visible*,
/// so it differs from the size of the tree on every outline with a collapsed branch —
/// all three that the sample corpus carries.
/// [`crate::interactive::Outline`] walks the tree and compares the two;
/// this reads what the root dictionary declares.
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "12.3.3")]
pub struct OutlineRoot {
    #[pdf_key("Type")]
    /// `/Type`, `Outlines` when written.
    pub kind: Option<PdfName>,
    #[pdf_key("First")]
    /// `/First`: the first top-level item.
    pub first: Option<Handle<Object>>,
    #[pdf_key("Last")]
    /// `/Last`: the last top-level item.
    pub last: Option<Handle<Object>>,
    #[pdf_key("Count")]
    /// `/Count`: how many items are *visible* when the outline is first shown.
    pub visible_count: Option<i64>,
}

/// `/StructTreeRoot` (14.7.4.2, Table 322): the root of the logical structure.
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "14.7.4.2")]
pub struct StructTreeRoot {
    #[pdf_key("Type")]
    /// `/Type`, `StructTreeRoot` when written.
    pub kind: Option<PdfName>,
    #[pdf_key("K")]
    /// `/K`: the children — one element, or an array of them. Named, not walked: the
    /// tree is what `fepdf-doc`'s visitor and the UA-2 audit exist for, and reading it
    /// here would be a second implementation of the same walk.
    pub children: Option<Object>,
    #[pdf_key("IDTree")]
    /// `/IDTree`: structure elements by their `/ID`.
    pub id_tree: Option<Object>,
    #[pdf_key("ParentTree")]
    /// `/ParentTree`: which structure element each marked-content sequence belongs to.
    pub parent_tree: Option<Object>,
    #[pdf_key("ParentTreeNextKey")]
    /// `/ParentTreeNextKey`: the next free key, so a writer can add without colliding.
    pub parent_tree_next_key: Option<i64>,
    #[pdf_key("RoleMap")]
    /// `/RoleMap`: how this document's element types map onto the standard ones.
    pub role_map: Option<Object>,
    #[pdf_key("ClassMap")]
    /// `/ClassMap`: named attribute sets elements can refer to.
    pub class_map: Option<Object>,
    #[pdf_key("Namespaces")]
    /// `/Namespaces`: the 2.0 namespaces the structure uses (14.7.4.3).
    pub namespaces: Option<Handle<Vec<Object>>>,
}

/// `/Names` (7.7.4, Table 31): the document's name trees.
///
/// Which trees a document declares, and how many names each holds. The names themselves
/// are not collected — `/Dests` alone holds 279,501 in one sample — but the *shape* is
/// the information: a `/JavaScript` tree is a document that runs code when it opens, and
/// an `/EmbeddedFiles` tree is a document carrying other files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NameDictionary {
    /// Each tree the dictionary declares, and how many names it holds, in Table 31's
    /// order.
    pub trees: Vec<NameTree>,
}

/// One of Table 31's name trees.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NameTree {
    /// The key — `Dests`, `JavaScript`, `EmbeddedFiles`, and so on.
    pub key: String,
    /// How many names the tree holds, counted by walking it.
    pub names: usize,
}

/// The keys Table 31 defines, in the order it defines them.
const NAME_TREES: &[&str] = &[
    "Dests",
    "AP",
    "JavaScript",
    "Pages",
    "Templates",
    "IDS",
    "URLS",
    "EmbeddedFiles",
    "AlternatePresentations",
    "Renditions",
];

impl FromPdfObject for NameDictionary {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let dict = dict_of(arena, &obj)
            .ok_or_else(|| PdfError::Other("/Names is not a dictionary".into()))?;
        let mut out = Self::default();
        for key in NAME_TREES {
            let Some(root) = dict.get(&arena.name(key)).and_then(|v| dict_of(arena, v)) else {
                continue;
            };
            let mut names = Vec::new();
            walk_names(arena, &root, &mut names, 0);
            out.trees.push(NameTree { key: (*key).to_string(), names: names.len() });
        }
        Ok(out)
    }
}

/// `/Metadata` (14.3.2): the XMP packet, and what it says.
///
/// The one catalogue entry whose contents are not PDF at all — it is a stream of XML,
/// and reading it means decoding the stream and parsing the packet. 210 files of the
/// two corpora carry one.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XmpMetadata {
    /// `/Subtype`, which 14.3.2 requires to be `XML`.
    pub subtype: Option<String>,
    /// How many bytes the decoded packet holds. Zero when the stream would not decode,
    /// which is a fact about the file rather than a reason to report no metadata.
    pub packet_bytes: usize,
    /// What the packet says, as far as this engine reads XMP: the Dublin Core and
    /// XMP Basic properties that `metadata.rs` reconciles with `/Info`.
    pub properties: crate::metadata::MetadataInfo,
}

impl FromPdfObject for XmpMetadata {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let resolved = obj.resolve(arena);
        let Object::Stream(dh, ref data) = resolved else {
            return Err(PdfError::Other("/Metadata is not a stream (14.3.2)".into()));
        };
        let mut out = Self {
            subtype: arena.get_dict(dh).and_then(|d| name_at(arena, &d, "Subtype")).map(|n| n.0),
            ..Self::default()
        };
        // A packet that will not decode leaves the properties empty rather than failing
        // the whole catalogue: 14.3.2 metadata is descriptive, and a file whose XMP is
        // damaged is still a file whose catalogue reads.
        if let Ok(bytes) = arena.get_stream_bytes(data) {
            out.packet_bytes = bytes.len();
            if let Some(info) = crate::metadata::read_xmp_packet(&bytes) {
                out.properties = info;
            }
        }
        Ok(out)
    }
}

/// `/Pages` (7.7.3.2, Table 30): the root of the page tree.
///
/// What the root *declares* — the count it claims and the attributes its pages inherit
/// — rather than the pages themselves, which `Document::pages` holds after ingestion has
/// resolved inheritance into each page ([ADR-0013](../../../docs/adr/0013-a-document-is-one-normalised-state.md)).
/// The claim is worth reading separately: `/Count` is a number the file asserts, and a
/// file that asserts the wrong one is a file this engine can now say so about.
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "7.7.3.2")]
pub struct PageTreeRoot {
    #[pdf_key("Type")]
    /// `/Type`, `Pages` when written.
    pub kind: Option<PdfName>,
    #[pdf_key("Kids")]
    /// `/Kids`: the immediate children, which may be pages or further nodes.
    pub kids: Option<Handle<Vec<Object>>>,
    #[pdf_key("Count")]
    /// `/Count`: how many leaf pages the file claims are under this node.
    pub declared_pages: Option<i64>,
    #[pdf_key("MediaBox")]
    /// `/MediaBox`: inherited by every page beneath that does not state its own (7.7.3.4).
    pub media_box: Option<crate::graphics::Rect>,
    #[pdf_key("Resources")]
    /// `/Resources`: likewise inherited. Reachable, not modelled — a resource dictionary
    /// is a subsystem.
    pub resources: Option<Object>,
    #[pdf_key("Rotate")]
    /// `/Rotate`: the inherited rotation, in degrees clockwise and a multiple of 90.
    pub rotate: Option<i64>,
}

/// A value read from an indirect object, with the handle it was read *from*.
///
/// Some catalogue entries are needed both ways. `/Pages` has contents worth reading —
/// the count the file claims, the attributes its pages inherit — and the page walk needs
/// the *handle*, because it descends from there; `/StructTreeRoot` is the same for the
/// structure-tree visitor and the UA-2 audit. Before this, those accessors read the raw
/// dictionary for one key, which is precisely the ad-hoc handling that typing an entry is
/// supposed to remove.
///
/// `reference` is `None` when the entry is written in place rather than as a reference,
/// which is legal and which the page tree of a small file does.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Located<T> {
    /// Where the value lives, when it lives somewhere.
    pub reference: Option<Handle<Object>>,
    /// What it says.
    pub value: T,
}

impl<T: FromPdfObject> FromPdfObject for Located<T> {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let reference = obj.as_reference();
        // A reference that resolves to nothing is worth saying plainly. Left to the
        // derive it reads "Expected dictionary for PageTreeRoot, got Reference(5)",
        // which describes the type's disappointment rather than the file's defect —
        // and the file's defect is that object 5 is not in it.
        // `UnknownFilter-xrefstm.pdf` is exactly that: its page tree root was indexed
        // only by a cross-reference stream nothing can decode.
        if let Some(handle) = reference
            && matches!(obj.resolve(arena), Object::Null)
        {
            return Err(PdfError::Arena(
                format!("object {} does not resolve", handle.index()).into(),
            ));
        }
        T::from_pdf_object(obj, arena).map(|value| Self { reference, value })
    }
}

// --- shared readers -------------------------------------------------------------

/// How deep a tree is followed before it is assumed to be looping.
const MAX_TREE_DEPTH: usize = 64;

/// Collects `(key, value)` from a number tree (7.9.7), following `/Kids`.
fn walk_number_tree(arena: &PdfArena, node: &Dict, out: &mut Vec<(i64, Object)>, depth: usize) {
    if depth >= MAX_TREE_DEPTH {
        return;
    }
    if let Some(kids) = node.get(&arena.name("Kids")).and_then(|k| array_of(arena, k)) {
        for kid in &kids {
            if let Some(d) = dict_of(arena, kid) {
                walk_number_tree(arena, &d, out, depth + 1);
            }
        }
    }
    if let Some(nums) = node.get(&arena.name("Nums")).and_then(|n| array_of(arena, n)) {
        for pair in nums.chunks(2) {
            if let [key, value] = pair
                && let Some(index) = key.resolve(arena).as_integer()
            {
                out.push((index, value.clone()));
            }
        }
    }
}

/// Collects the names of a name tree (7.9.6), following `/Kids`.
fn walk_names(arena: &PdfArena, node: &Dict, out: &mut Vec<Vec<u8>>, depth: usize) {
    if depth >= MAX_TREE_DEPTH {
        return;
    }
    if let Some(kids) = node.get(&arena.name("Kids")).and_then(|k| array_of(arena, k)) {
        for kid in &kids {
            if let Some(d) = dict_of(arena, kid) {
                walk_names(arena, &d, out, depth + 1);
            }
        }
    }
    if let Some(names) = node.get(&arena.name("Names")).and_then(|n| array_of(arena, n)) {
        for pair in names.chunks(2) {
            if let [key, _] = pair
                && let Some(bytes) = key.resolve(arena).as_string()
            {
                out.push(bytes.to_vec());
            }
        }
    }
}

fn dict_of(arena: &PdfArena, object: &Object) -> Option<Dict> {
    match object.resolve(arena) {
        Object::Dictionary(h) | Object::Stream(h, _) => arena.get_dict(h),
        _ => None,
    }
}

fn array_of(arena: &PdfArena, object: &Object) -> Option<Vec<Object>> {
    arena.get_array(object.resolve(arena).as_array()?)
}

fn name_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<PdfName> {
    arena.get_name(dict.get(&arena.name(key))?.resolve(arena).as_name()?)
}

fn text_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<String> {
    match dict.get(&arena.name(key))?.resolve(arena) {
        Object::Text(s) => Some(s),
        Object::String(b) | Object::Hex(b) => Some(crate::refine::text::recover_string(&b)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::PdfSchema;

    /// One object, written the way a file writes it. Parsed rather than built by hand,
    /// so the test exercises the same path a document takes.
    fn arena_with(source: &str) -> (PdfArena, Object) {
        let arena = PdfArena::new();
        let mut parser =
            crate::parser::Parser::new(bytes::Bytes::from(format!("{source}\n")), &arena);
        let object = parser.parse_object().expect("parses");
        (arena, object)
    }

    /// `/Version` is two numbers, and comparing it as text gets 1.10 and 1.9 backwards.
    #[test]
    fn a_declared_version_is_parsed_into_its_numbers() {
        let (arena, object) = arena_with("/2.0");
        let v = DeclaredVersion::from_pdf_object(object, &arena).expect("reads");
        assert_eq!(v.numbers(), Some((2, 0)));

        let (arena, object) = arena_with("/1.10");
        let ten = DeclaredVersion::from_pdf_object(object, &arena).expect("reads");
        let (arena9, object9) = arena_with("/1.9");
        let nine = DeclaredVersion::from_pdf_object(object9, &arena9).expect("reads");
        assert!(ten.numbers() > nine.numbers(), "1.10 is later than 1.9");
        let _ = arena;
    }

    /// A name that is not a version is kept, not refused.
    ///
    /// Refusing it would refuse the catalogue, and refusing the catalogue would refuse
    /// the document — over an entry that only overrides the header's version.
    #[test]
    fn a_version_that_is_not_a_version_is_kept_verbatim() {
        let (arena, object) = arena_with("/NotAVersion");
        let v = DeclaredVersion::from_pdf_object(object, &arena).expect("reads");
        assert_eq!(v, DeclaredVersion::Other("NotAVersion".into()));
        assert_eq!(v.numbers(), None);
    }

    /// The three booleans of Table 321 stay apart from "not written".
    #[test]
    fn mark_info_keeps_unstated_apart_from_false() {
        let (arena, object) = arena_with("<< /Marked true >>");
        let m = MarkInfo::from_pdf_object(object, &arena).expect("reads");
        assert_eq!(m.marked, Some(true));
        assert_eq!(m.suspects, None, "unwritten is not `false`");
    }

    /// `/SigFlags` is a bit field: 3 means both flags, not "three signatures".
    #[test]
    fn signature_flags_are_bits() {
        let (arena, object) = arena_with("<< /SigFlags 3 /NeedAppearances true >>");
        let form = AcroForm::from_pdf_object(object, &arena).expect("reads");
        let flags = form.signature_flags.expect("present");
        assert!(flags.signatures_exist && flags.append_only);
        assert_eq!(form.need_appearances, Some(true));
        assert!(form.default_appearance.is_none());
    }

    /// `/OpenAction` is a destination array *or* an action dictionary, and 208 files
    /// carry one. A reader that assumed either shape would report the other as absent.
    #[test]
    fn an_open_action_reads_both_of_its_shapes() {
        let (arena, object) = arena_with("[ 3 0 R /Fit ]");
        assert!(matches!(
            TriggeredAction::from_pdf_object(object, &arena).expect("reads"),
            TriggeredAction::Destination(_)
        ));

        let (arena, object) = arena_with("<< /S /URI /URI (https://example.invalid) >>");
        let TriggeredAction::Action(action) =
            TriggeredAction::from_pdf_object(object, &arena).expect("reads")
        else {
            panic!("a dictionary is an action");
        };
        assert_eq!(action.action.as_str(), "URI");
        assert_eq!(action.uri.as_deref(), Some("https://example.invalid"));
    }

    /// The label tree is read as ranges, in page order, whatever order the tree holds.
    #[test]
    fn page_labels_are_read_as_ranges_in_page_order() {
        let (arena, object) =
            arena_with("<< /Nums [ 4 << /S /D /St 1 >> 0 << /S /r /P (front-) >> ] >>");
        let labels = PageLabels::from_pdf_object(object, &arena).expect("reads");
        assert_eq!(labels.ranges.len(), 2);
        assert_eq!(labels.ranges[0].from_page, 0);
        assert_eq!(labels.ranges[0].style.as_ref().map(PdfName::as_str), Some("r"));
        assert_eq!(labels.ranges[0].prefix.as_deref(), Some("front-"));
        assert_eq!(labels.ranges[0].start_at, None, "unwritten, not defaulted to 1");
        assert_eq!(labels.ranges[1].from_page, 4);
        assert_eq!(labels.ranges[1].start_at, Some(1));
        assert_eq!(labels.unreadable, 0);
    }

    /// A tree entry that is not a label dictionary is counted, not dropped.
    #[test]
    fn a_label_that_will_not_read_is_counted() {
        let (arena, object) = arena_with("<< /Nums [ 0 << /S /D >> 2 42 ] >>");
        let labels = PageLabels::from_pdf_object(object, &arena).expect("reads");
        assert_eq!(labels.ranges.len(), 1);
        assert_eq!(labels.unreadable, 1);
    }

    /// `/Names` reports which trees exist and how big each is.
    #[test]
    fn the_name_dictionary_reports_a_tree_and_its_size() {
        let (arena, object) = arena_with(
            "<< /JavaScript << /Names [ (a) 1 0 R (b) 2 0 R ] >> \
              /EmbeddedFiles << /Names [ (f) 3 0 R ] >> >>",
        );
        let names = NameDictionary::from_pdf_object(object, &arena).expect("reads");
        let js = names.trees.iter().find(|t| t.key == "JavaScript").expect("declared");
        assert_eq!(js.names, 2);
        assert!(!names.trees.iter().any(|t| t.key == "Dests"), "not declared");
        assert_eq!(names.trees.len(), 2);
    }

    /// An array element that will not read is counted, and the others still read.
    #[test]
    fn one_bad_output_intent_does_not_lose_the_others() {
        let (arena, object) = arena_with(
            "[ << /S /GTS_PDFA1 /OutputConditionIdentifier (sRGB) >> 42 \
              << /S /GTS_PDFX >> ]",
        );
        let intents = OutputIntents::from_pdf_object(object, &arena).expect("reads");
        assert_eq!(intents.intents.len(), 2);
        assert_eq!(intents.unreadable, 1);
        assert_eq!(intents.intents[0].subtype.as_ref().map(PdfName::as_str), Some("GTS_PDFA1"));
        assert_eq!(intents.intents[0].condition_identifier.as_deref(), Some("sRGB"));
    }

    /// `Located` keeps the handle *and* the contents, which is what the page walk and
    /// the structure-tree visitor need and what a bare reader threw away.
    #[test]
    fn a_located_entry_carries_both_where_and_what() {
        let arena = PdfArena::new();
        let dict = arena.alloc_dict(BTreeMap::from([(arena.name("Count"), Object::Integer(7))]));
        let handle = arena.alloc_object(Object::Dictionary(dict));
        let located = Located::<PageTreeRoot>::from_pdf_object(Object::Reference(handle), &arena)
            .expect("reads");
        assert_eq!(located.reference, Some(handle));
        assert_eq!(located.value.declared_pages, Some(7));

        // Written in place, so there is no reference to keep.
        let direct = Located::<PageTreeRoot>::from_pdf_object(Object::Dictionary(dict), &arena)
            .expect("reads");
        assert_eq!(direct.reference, None);
        assert_eq!(direct.value.declared_pages, Some(7));
    }

    /// One entry reads even when another will not, which is what keeps a document with
    /// one bad key openable.
    #[test]
    fn one_entry_reads_without_the_rest_of_the_catalogue() {
        let (arena, catalog) =
            arena_with("<< /Type /Catalog /Pages << /Count 3 >> /MarkInfo 42 >>");
        let pages: Option<Located<PageTreeRoot>> = entry(&arena, &catalog, "Pages").expect("reads");
        assert_eq!(pages.expect("present").value.declared_pages, Some(3));

        // The whole catalogue does not read, and that is the point of the narrow one.
        assert!(crate::document::PdfCatalog::from_pdf_object(catalog.clone(), &arena).is_err());
        assert!(entry::<MarkInfo>(&arena, &catalog, "MarkInfo").is_err(), "still reported");
        assert!(
            entry::<MarkInfo>(&arena, &catalog, "Outlines").expect("absent").is_none(),
            "an entry the catalogue does not carry is not an error"
        );
    }

    /// Every reader here declares the clause it reads, so a report can cite it.
    #[test]
    fn each_reader_names_its_clause() {
        assert_eq!(MarkInfo::iso_clause(), "14.7.1");
        assert_eq!(AcroForm::iso_clause(), "12.7.2");
        assert_eq!(OutputIntent::iso_clause(), "14.11.5");
        assert_eq!(OutlineRoot::iso_clause(), "12.3.3");
        assert_eq!(StructTreeRoot::iso_clause(), "14.7.4.2");
        assert_eq!(PageTreeRoot::iso_clause(), "7.7.3.2");
    }
}
