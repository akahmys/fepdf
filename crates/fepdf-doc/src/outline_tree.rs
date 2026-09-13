//! Reading the bookmark tree back out of a document (12.3.3).
//!
//! [`crate::Operation::UpdateOutlines`] has been able to *write* an outline since the
//! operation vocabulary was first laid out. Nothing could read one, so the only outline
//! an editor could offer was one built from nothing: opening a document with bookmarks
//! and saving it back through that operation replaced whatever it had.
//!
//! This is the other half. It answers the same [`OutlineTree`] the operation takes, so
//! read-edit-write is a round trip rather than a replacement.
//!
//! **What the round trip drops.** [`OutlineNode`] carries a title, a page and children,
//! and Table 153 gives an item five more entries: `/C` (colour), `/F` (bold and italic),
//! `/SE` (the structure element it belongs to), `/A` (an action instead of a
//! destination), and the sign of `/Count` (whether the item is open). A document read
//! here and written back loses all five. That is the operation's shape, not this
//! reader's, and widening it is a separate decision — recorded so that the loss is a
//! known one rather than a surprise.

use fepdf_model::Document;
use fepdf_model::arena::PdfArena;
use fepdf_model::destination::{Lookup, NamedDestinations, Target};
use fepdf_model::document::extensions::{OutlineNode, OutlineTree};
use fepdf_model::handle::Handle;
use fepdf_model::object::{Object, PdfName};
use std::collections::{BTreeMap, BTreeSet};

/// How deep the `/First` chain is followed before the tree is taken to be looping.
///
/// The same 64 as the `/K` walk in [`crate::struct_tree`] and the field-tree walk in
/// `fepdf-model`, and the same bound [`crate::apply`] refuses to *write* past.
const MAX_OUTLINE_DEPTH: usize = 64;

/// A dictionary as the arena answers one.
type Dict = BTreeMap<Handle<PdfName>, Object>;

/// A bookmark that named a page this document does not have.
///
/// Such an item still has a title and still holds its children, so dropping it would
/// silently shorten the tree. It is kept, pointed at page 0, and counted here — the
/// count is what a caller reports rather than discovering the lie later.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OutlineReport {
    /// Items read, at every level.
    pub items: usize,
    /// Of those, the ones whose destination named no page of this document: a `/Dest`
    /// that is absent, a name nothing declares, or an `/A` that is not a `/GoTo`.
    pub placeless: usize,
    /// Whether a `/Next` or `/First` link was met twice and the walk stopped there.
    pub looped: bool,
}

/// What the walk carries down and back up.
struct Reader<'a> {
    arena: &'a PdfArena,
    pages: BTreeMap<Handle<Object>, usize>,
    names: NamedDestinations,
    seen: BTreeSet<Handle<Object>>,
    report: OutlineReport,
}

/// Reads the catalogue's `/Outlines` into the tree [`crate::Operation::UpdateOutlines`]
/// takes.
///
/// Answers an empty tree — not an error — for a document that declares no `/Outlines`,
/// which is most of them. The [`OutlineReport`] says how much of what was there survived
/// the read.
#[must_use]
pub fn read_outlines(doc: &Document) -> (OutlineTree, OutlineReport) {
    let arena = doc.arena();
    let Some(catalog) = doc.catalog_handle().and_then(|h| doc.resolve_to_dict(h).ok()) else {
        return (OutlineTree::default(), OutlineReport::default());
    };
    let Some(dict) = arena.get_dict(catalog) else {
        return (OutlineTree::default(), OutlineReport::default());
    };
    let Some(root) = dict.get(&arena.name("Outlines")).and_then(|o| node_handle(arena, o)) else {
        return (OutlineTree::default(), OutlineReport::default());
    };

    let mut reader = Reader {
        arena,
        pages: page_handles(doc),
        names: NamedDestinations::collect(arena, &dict),
        seen: BTreeSet::new(),
        report: OutlineReport::default(),
    };
    reader.seen.insert(root);
    let items = reader.level(root, 0);
    (OutlineTree { items }, reader.report)
}

impl Reader<'_> {
    /// The `/First` … `/Next` chain hanging off `parent`, in file order.
    fn level(&mut self, parent: Handle<Object>, depth: usize) -> Vec<OutlineNode> {
        let mut out = Vec::new();
        if depth >= MAX_OUTLINE_DEPTH {
            self.report.looped = true;
            return out;
        }
        let Some(first) = self.entry(parent, "First") else { return out };
        let mut current = Some(first);
        while let Some(handle) = current {
            if !self.seen.insert(handle) {
                self.report.looped = true;
                break;
            }
            out.push(self.item(handle, depth));
            current = self.entry(handle, "Next");
        }
        out
    }

    /// One item, with everything under it.
    fn item(&mut self, handle: Handle<Object>, depth: usize) -> OutlineNode {
        let title = self
            .dict_of(handle)
            .and_then(|d| text_entry(self.arena, &d, "Title"))
            .unwrap_or_default();
        let page = self.destination_page(handle);
        self.report.items += 1;
        if page.is_none() {
            self.report.placeless += 1;
        }
        OutlineNode {
            title,
            destination_page: page.unwrap_or(0),
            children: self.level(handle, depth + 1),
        }
    }

    /// The page an item names, through `/Dest` or through an `/A` `/GoTo`.
    fn destination_page(&self, handle: Handle<Object>) -> Option<usize> {
        let dict = self.dict_of(handle)?;
        let dest = match dict.get(&self.arena.name("Dest")) {
            Some(entry) => entry.clone(),
            // 12.3.3: an item carries a destination *or* an action, and a `/GoTo`
            // action's `/D` is a destination written one dictionary further down.
            None => go_to_destination(self.arena, dict.get(&self.arena.name("A"))?)?,
        };
        let found = match self.names.resolve(&dest, self.arena) {
            Lookup::Inline(d) | Lookup::Named(d) => d,
            Lookup::Dangling(_) | Lookup::Unreadable => return None,
        };
        match found.target {
            Target::Page(page) => self.pages.get(&page).copied(),
            // A page number in another file. It names no page of this document.
            Target::RemotePage(_) => None,
        }
    }

    fn dict_of(&self, handle: Handle<Object>) -> Option<Dict> {
        self.arena.get_object(handle)?.as_dict_handle().and_then(|dh| self.arena.get_dict(dh))
    }

    fn entry(&self, handle: Handle<Object>, key: &str) -> Option<Handle<Object>> {
        node_handle(self.arena, self.dict_of(handle)?.get(&self.arena.name(key))?)
    }
}

/// An `/A` entry's destination, when the action is a go-to within this file (12.6.4.2).
fn go_to_destination(arena: &PdfArena, action: &Object) -> Option<Object> {
    let dict = arena.get_dict(action.resolve(arena).as_dict_handle()?)?;
    let subtype = dict.get(&arena.name("S"))?.resolve(arena).as_name()?;
    if arena.get_name_str(subtype).as_deref() != Some("GoTo") {
        return None;
    }
    dict.get(&arena.name("D")).cloned()
}

/// A text-string entry, in the shapes a `/Title` is written in.
fn text_entry(arena: &PdfArena, dict: &Dict, key: &str) -> Option<String> {
    match dict.get(&arena.name(key))?.resolve(arena) {
        Object::String(bytes) | Object::Hex(bytes) => {
            Some(fepdf_model::refine::text::recover_string(&bytes))
        }
        Object::Text(text) => Some(text),
        _ => None,
    }
}

/// The object an entry points at, whether by reference or written in place.
///
/// **The reference is taken before it is followed.** Resolving first and taking the
/// dictionary handle of the answer gives `Handle::new(dh.index())` — an index into the
/// dictionary table read as an index into the object table, which is a different object
/// or none. The same mistake cost the structure-tree reader every one of its elements.
fn node_handle(arena: &PdfArena, obj: &Object) -> Option<Handle<Object>> {
    crate::struct_tree::resolve_to_node_handle(arena, obj)
}

fn page_handles(doc: &Document) -> BTreeMap<Handle<Object>, usize> {
    let mut map = BTreeMap::new();
    if let Ok(count) = doc.page_count() {
        for index in 0..count {
            if let Some(page) = doc.get_page_handle(index) {
                map.insert(page, index);
            }
        }
    }
    map
}
