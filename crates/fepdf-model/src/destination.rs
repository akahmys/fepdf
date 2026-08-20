//! Destinations (12.3.2) and the name tree that holds them (7.9.6).
//!
//! Measured before it was written, and the measurement changed the shape of the work.
//! `ROADMAP.md` listed `Dests` as "one file" — the next catalogue entry to type by
//! occurrence — and that is true of the *entry*: only `volvo_xc90.pdf` carries a
//! catalogue `/Dests` dictionary, with 651 destinations. But `intel_sdm.pdf` declares
//! **279,501** named destinations through `/Names → /Dests`, which is the same feature
//! reached the other way (12.3.2.3 gives it two forms, one from PDF 1.1 and one from
//! 1.2), and 25,946 link annotations resolve through it. Typing the catalogue entry
//! alone would have covered 651 destinations and left 279,501 unreadable.
//!
//! So both forms are here, and they are genuinely different lookups rather than one
//! with an alias:
//!
//! | Form | Keyed by | Held in | Corpus |
//! | :--- | :--- | :--- | ---: |
//! | PDF 1.1 | name objects | the catalogue's `/Dests` dictionary | `volvo_xc90`, 651 |
//! | PDF 1.2 | byte strings | the `/Dests` name tree under `/Names` | `intel_sdm`, 279,501 |
//!
//! An annotation's `/Dest` is looked up in the one that matches its own type — a name
//! in the dictionary, a string in the tree — and nowhere else. Readers exist that try
//! both, and this does not, because the corpus says the fallback would never fire:
//! `volvo_xc90`'s references are all names and all found in the dictionary,
//! `intel_sdm`'s are all strings and all found in the tree but one. Building the
//! fallback would be building a path no file reaches (`ARCHITECTURE.md` §5.2).
//!
//! That one exception is what the work is for. `intel_sdm.pdf` references
//! `(G3.7717)`, which appears in no leaf of its name tree — a link that goes nowhere,
//! in a 5,000-page manual from a producer that had every reason to check. It is the
//! only one of 11,917 distinct names in that file, and nothing in this engine could
//! have said so before.

use crate::arena::PdfArena;
use crate::handle::Handle;
use crate::object::{Object, PdfName};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// What page a destination names, and how it names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    /// A page of this document, as the object Table 151's first element points at.
    Page(Handle<Object>),
    /// A zero-based page *number*, which Table 151 permits only in a destination
    /// reached through a remote go-to (12.6.4.3): the file being pointed at is not this
    /// one, so there is no object here to name.
    ///
    /// Zero occurrences in the corpus. Kept anyway, because a legal file may carry one
    /// and refusing to parse it would report a broken link where there is none.
    RemotePage(i64),
}

/// How a destination asks the page to be fitted to the window (Table 151).
///
/// The parameters are `Option<f32>` where the table writes "null" as a permitted value
/// meaning *keep whatever the viewer is currently using* — not a default this code may
/// substitute. `intel_sdm.pdf` writes `/XYZ null null null` and means it: place the page
/// without changing the scroll position or the zoom.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum View {
    /// `/XYZ left top zoom` — a corner and a zoom, any of them null.
    Xyz {
        /// Horizontal coordinate of the upper-left corner.
        left: Option<f32>,
        /// Vertical coordinate of the upper-left corner.
        top: Option<f32>,
        /// Zoom factor; null, and also 0, mean the current one.
        zoom: Option<f32>,
    },
    /// `/Fit` — the whole page in the window.
    Fit,
    /// `/FitH top` — page width, at a vertical position.
    FitH {
        /// Vertical coordinate to place at the top edge.
        top: Option<f32>,
    },
    /// `/FitV left` — page height, at a horizontal position.
    FitV {
        /// Horizontal coordinate to place at the left edge.
        left: Option<f32>,
    },
    /// `/FitR left bottom right top` — a rectangle, entirely in the window.
    ///
    /// The four numbers are required, not `Option`: Table 151 permits null in every
    /// other form and not in this one, and a `/FitR` without its rectangle names no
    /// region to fit. Zero occurrences in the corpus, so this is the table's word
    /// rather than a measurement.
    FitR {
        /// Left edge.
        left: f32,
        /// Bottom edge.
        bottom: f32,
        /// Right edge.
        right: f32,
        /// Top edge.
        top: f32,
    },
    /// `/FitB` — the bounding box of the page's contents (1.1).
    FitB,
    /// `/FitBH top` — bounding-box width, at a vertical position (1.1).
    FitBH {
        /// Vertical coordinate to place at the top edge.
        top: Option<f32>,
    },
    /// `/FitBV left` — bounding-box height, at a horizontal position (1.1).
    FitBV {
        /// Horizontal coordinate to place at the left edge.
        left: Option<f32>,
    },
}

impl View {
    /// The name Table 151 gives this form.
    #[must_use]
    pub fn as_name(&self) -> &'static str {
        match self {
            Self::Xyz { .. } => "XYZ",
            Self::Fit => "Fit",
            Self::FitH { .. } => "FitH",
            Self::FitV { .. } => "FitV",
            Self::FitR { .. } => "FitR",
            Self::FitB => "FitB",
            Self::FitBH { .. } => "FitBH",
            Self::FitBV { .. } => "FitBV",
        }
    }
}

/// A destination: a page, and how to fit it (12.3.2.2).
///
/// There is no `Other` arm on [`View`], unlike `PageMode` and `Direction`. Those are
/// enums over a *label*, so keeping an unrecognised name loses nothing. Table 151's
/// names instead determine how many numbers follow, so a form this code does not know
/// is a form whose arguments it cannot read — keeping the name would preserve a label
/// and throw the destination away. The set has also not changed since PDF 1.1 added the
/// three `FitB` forms, which is the opposite history from `/PageMode`'s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Destination {
    /// The page.
    pub target: Target,
    /// How to fit it.
    pub view: View,
}

impl Destination {
    /// Reads a destination from `object`, which may be the array itself, a reference to
    /// one, or the `<< /D array >>` dictionary 12.3.2.3 also allows — `intel_sdm.pdf`
    /// uses the dictionary form for all 279,501 of its named destinations.
    ///
    /// Returns `None` when `object` is not a destination this code can read: a form
    /// Table 151 does not define, a missing page, or too few arguments for the form.
    /// The caller decides what to say about that; here it is simply not a destination.
    #[must_use]
    pub fn read(object: &Object, arena: &PdfArena) -> Option<Self> {
        let resolved = object.resolve(arena);
        let array = match &resolved {
            Object::Array(handle) => arena.get_array(*handle)?,
            Object::Dictionary(handle) => {
                let inner = arena.get_dict(*handle)?.get(&arena.name("D"))?.clone();
                arena.get_array(inner.resolve(arena).as_array()?)?
            }
            _ => return None,
        };
        let target = target_of(array.first()?)?;
        let view = view_of(&array[1..], arena)?;
        Some(Self { target, view })
    }
}

/// Table 151's first element: a page object, or a number for a remote destination.
fn target_of(object: &Object) -> Option<Target> {
    match object {
        // Checked before resolving: an indirect reference *is* the page here, and
        // resolving first would turn it into the dictionary and lose which page it was.
        Object::Reference(handle) => Some(Target::Page(*handle)),
        Object::Integer(n) => Some(Target::RemotePage(*n)),
        _ => None,
    }
}

/// Table 151's remaining elements: the form name, then its arguments.
fn view_of(rest: &[Object], arena: &PdfArena) -> Option<View> {
    let name = match rest.first()?.resolve(arena) {
        Object::Name(handle) => arena.get_name_str(handle)?,
        _ => return None,
    };
    let args = &rest[1..];
    let n = |i: usize| args.get(i).and_then(|o| number(o, arena));
    Some(match name.as_str() {
        "XYZ" => View::Xyz { left: n(0), top: n(1), zoom: n(2) },
        "Fit" => View::Fit,
        "FitH" => View::FitH { top: n(0) },
        "FitV" => View::FitV { left: n(0) },
        "FitR" => View::FitR { left: n(0)?, bottom: n(1)?, right: n(2)?, top: n(3)? },
        "FitB" => View::FitB,
        "FitBH" => View::FitBH { top: n(0) },
        "FitBV" => View::FitBV { left: n(0) },
        _ => return None,
    })
}

/// A number, or `None` for the `null` Table 151 permits in place of one.
fn number(object: &Object, arena: &PdfArena) -> Option<f32> {
    match object.resolve(arena) {
        Object::Integer(i) => Some(i as f32),
        Object::Real(r) => Some(r as f32),
        _ => None,
    }
}

/// The catalogue's `/Dests` dictionary (12.3.2.3): destinations keyed by name (PDF 1.1).
///
/// A named type rather than the `BTreeMap` alone, so that `unreadable` travels with the
/// entries. A count of destinations that silently omitted the ones that would not parse
/// would be the more convenient number and the less true one.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct DestsDictionary {
    /// The destinations, by the name that declares each.
    pub entries: BTreeMap<String, Destination>,
    /// Entries present in the dictionary but not readable as a destination.
    pub unreadable: usize,
}

impl crate::object::FromPdfObject for DestsDictionary {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> crate::error::PdfResult<Self> {
        let dict = dict_of(arena, &obj).ok_or_else(|| {
            crate::error::PdfError::Arena("/Dests is not a dictionary (12.3.2.3)".into())
        })?;
        let mut out = Self::default();
        for (key, value) in &dict {
            let Some(name) = arena.get_name_str(*key) else { continue };
            match Destination::read(value, arena) {
                Some(d) => {
                    out.entries.insert(name, d);
                }
                None => out.unreadable += 1,
            }
        }
        Ok(out)
    }
}

impl crate::object::PdfSchema for DestsDictionary {
    fn iso_clause() -> &'static str {
        "12.3.2.3"
    }

    fn pdf_keys() -> &'static [&'static str] {
        // The keys *are* the destination names, so there is no fixed set to list.
        &[]
    }

    fn pdf_key_types() -> &'static [(&'static str, &'static str)] {
        // Likewise: every value is a `Destination`, and none of them is a fixed key.
        &[]
    }
}

/// Every named destination the document declares, by the form that declared it.
///
/// The two maps are not merged. A `/Dest` that is a name is answered from `by_name` and
/// a `/Dest` that is a byte string from `by_string`, because 12.3.2.3 puts them in
/// different places and the corpus supplies exactly one file of each kind. Merging them
/// would make a document that declares `/Foo` in the 1.1 dictionary answer a `(Foo)`
/// reference, which no conforming reader promises and this engine cannot check against
/// anything.
#[derive(Debug, Clone, Default)]
pub struct NamedDestinations {
    /// The catalogue's `/Dests` dictionary, keyed by name (PDF 1.1).
    pub by_name: BTreeMap<String, Destination>,
    /// The `/Dests` name tree under `/Names`, keyed by byte string (PDF 1.2).
    pub by_string: BTreeMap<Vec<u8>, Destination>,
    /// Entries present in one of the two but unreadable as a destination — a form
    /// Table 151 does not define, or a first element that is not a page. Counted rather
    /// than dropped, so "this document declares 651 destinations" cannot quietly mean
    /// "651 minus the ones that did not parse".
    pub unreadable: usize,
}

impl NamedDestinations {
    /// Collects both forms from `catalog`.
    #[must_use]
    pub fn collect(arena: &PdfArena, catalog: &Dict) -> Self {
        use crate::object::FromPdfObject as _;
        let mut out = Self::default();
        if let Some(entry) = catalog.get(&arena.name("Dests"))
            && let Ok(dests) = DestsDictionary::from_pdf_object(entry.clone(), arena)
        {
            out.unreadable += dests.unreadable;
            out.by_name = dests.entries;
        }
        if let Some(names) = catalog.get(&arena.name("Names")).and_then(|o| dict_of(arena, o))
            && let Some(root) = names.get(&arena.name("Dests"))
        {
            let mut leaves = Vec::new();
            walk_name_tree(arena, root, &mut leaves, 0);
            for (key, value) in leaves {
                match Destination::read(&value, arena) {
                    Some(d) => {
                        out.by_string.insert(key, d);
                    }
                    None => out.unreadable += 1,
                }
            }
        }
        out
    }

    /// How many destinations were read, across both forms.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_name.len() + self.by_string.len()
    }

    /// Whether the document declares no named destination at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Resolves an annotation's or action's `/Dest`.
    ///
    /// An array is a destination written in place and needs no lookup; a name and a
    /// byte string are each looked up in their own form's map.
    #[must_use]
    pub fn resolve(&self, dest: &Object, arena: &PdfArena) -> Lookup {
        match dest.resolve(arena) {
            Object::Array(_) | Object::Dictionary(_) => match Destination::read(dest, arena) {
                Some(d) => Lookup::Inline(d),
                None => Lookup::Unreadable,
            },
            Object::Name(handle) => {
                let Some(name) = arena.get_name_str(handle) else { return Lookup::Unreadable };
                match self.by_name.get(&name) {
                    Some(d) => Lookup::Named(d.clone()),
                    None => Lookup::Dangling(name),
                }
            }
            Object::String(bytes) | Object::Hex(bytes) => self.by_bytes(&bytes),
            // A destination name is a byte string, not a text string (7.9.2.2), so
            // nothing should have decoded it — but `Object::Text` exists and the
            // lookup key is bytes either way, so this reads what it is given rather
            // than declining on a technicality.
            Object::Text(text) => self.by_bytes(text.as_bytes()),
            _ => Lookup::Unreadable,
        }
    }

    fn by_bytes(&self, key: &[u8]) -> Lookup {
        match self.by_string.get(key) {
            Some(d) => Lookup::Named(d.clone()),
            None => Lookup::Dangling(String::from_utf8_lossy(key).into_owned()),
        }
    }
}

/// What a `/Dest` turned out to name.
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    /// Written in place, so nothing was looked up.
    Inline(Destination),
    /// A name or string that a declared destination answered.
    Named(Destination),
    /// A name or string that nothing declares — a link that goes nowhere.
    Dangling(String),
    /// Present but not a destination: not an array, name or string, or an array this
    /// code cannot read as one.
    Unreadable,
}

/// The depth bound for a name tree.
///
/// `intel_sdm.pdf`'s `/Dests` tree is three levels for 279,501 entries, so a tree deep
/// enough to need more than this would have to be pathological or circular. The bound is
/// what stops a `/Kids` cycle, which nothing in the format forbids.
const MAX_TREE_DEPTH: usize = 32;

/// Appends every `(key, value)` of a name tree to `out` (7.9.6).
///
/// A node has `/Kids` or `/Names`, never both; `/Limits` states the key range a subtree
/// covers and is **not** consulted here. It exists so a reader can find one key without
/// reading the whole tree, and this walks the whole tree — so trusting `/Limits` would
/// mean a file with wrong limits silently lost entries, which is the failure mode of
/// believing an index over the data.
fn walk_name_tree(arena: &PdfArena, node: &Object, out: &mut Vec<(Vec<u8>, Object)>, depth: usize) {
    if depth >= MAX_TREE_DEPTH {
        return;
    }
    let Some(dict) = dict_of(arena, node) else { return };
    if let Some(kids) = dict.get(&arena.name("Kids")).and_then(|k| array_of(arena, k)) {
        for kid in &kids {
            walk_name_tree(arena, kid, out, depth + 1);
        }
        return;
    }
    let Some(pairs) = dict.get(&arena.name("Names")).and_then(|n| array_of(arena, n)) else {
        return;
    };
    for pair in pairs.chunks(2) {
        if let [key, value] = pair
            && let Some(key) = string_bytes(key, arena)
        {
            out.push((key, value.clone()));
        }
    }
}

/// A name tree's key, which 7.9.6 defines as a byte string.
fn string_bytes(object: &Object, arena: &PdfArena) -> Option<Vec<u8>> {
    match object.resolve(arena) {
        Object::String(bytes) | Object::Hex(bytes) => Some(bytes.to_vec()),
        Object::Text(text) => Some(text.into_bytes()),
        _ => None,
    }
}

fn dict_of(arena: &PdfArena, object: &Object) -> Option<Dict> {
    arena.get_dict(object.resolve(arena).as_dict_handle()?)
}

fn array_of(arena: &PdfArena, object: &Object) -> Option<Vec<Object>> {
    arena.get_array(object.resolve(arena).as_array()?)
}
