//! What a page taken out of a document leaves behind, taken out with it (ROADMAP Y-F18).
//!
//! **A page out of the page tree is not out of the file.** The writer writes what is
//! reachable, and a page is reachable from much besides `/Kids`: a link's or a bookmark's
//! destination, a structure element's `/Pg`, an annotation's `/P`. Measured on a two-page
//! fixture whose first page links to the second, removing the second and saving left its
//! content stream in the output, readable by anyone who opened the file with a text
//! editor, and page extraction did the same by cloning what the link named. A reader who
//! removes a page to send the rest has sent it.
//!
//! **What named the page goes with it**, which is the user's choice over leaving the
//! references empty: a link to it is removed, a bookmark keeps its title and children and
//! loses where it went, an `/OpenAction` or a named destination naming it is removed, and
//! structure content on it — marks, annotation references, and the elements left holding
//! nothing — is taken out of the tree and the parent tree. Whatever still points at the
//! page after that is made `null`, so that nothing can reach it whatever this did not
//! foresee.

use fepdf_model::destination::{Destination, Lookup, NamedDestinations, Target};
use fepdf_model::{Document, Handle, Object, PdfArena, PdfResult};
use std::collections::BTreeSet;

/// How deep the outline and structure walks go (Rule 6).
const DEPTH: usize = 256;

/// Page objects the document holds and its page tree does not.
type Absent = BTreeSet<Handle<Object>>;

/// Takes out of `doc` every page object its page tree does not list, and what named one.
///
/// # Errors
/// Fails when the catalogue will not read.
pub fn forget_absent_pages(doc: &Document) -> PdfResult<()> {
    let absent = absent_pages(doc);
    if absent.is_empty() {
        return Ok(());
    }
    let arena = doc.arena();
    let catalog = doc.catalog_handle().and_then(|h| doc.resolve_to_dict(h).ok());
    let named = catalog
        .and_then(|c| arena.get_dict(c))
        .map(|dict| NamedDestinations::collect(arena, &dict))
        .unwrap_or_default();
    let gone = Gone { arena, absent: &absent, named: &named };
    let mut dropped = annotations_of(arena, &absent);
    dropped.extend(gone.links(doc));
    if let Some(catalog) = catalog {
        gone.outlines(catalog);
        gone.open_action(catalog);
        gone.named_destinations(catalog);
    }
    if let Some(root) = doc.get_structure_root()? {
        let keys = structure_keys(arena, &absent, &dropped);
        crate::struct_tree_pruning::prune(arena, root, &absent, &dropped, &keys);
    }
    forget_references(arena, &absent);
    Ok(())
}

/// Every page object in the arena that the page tree does not list.
fn absent_pages(doc: &Document) -> Absent {
    let arena = doc.arena();
    let listed: BTreeSet<Handle<Object>> = doc.pages.iter().copied().collect();
    let page = arena.name("Page");
    let type_key = arena.name("Type");
    (0..arena.object_count())
        .map(|i| arena.handle(i))
        .filter(|handle| !listed.contains(handle))
        .filter(|handle| {
            arena.get_object(*handle).and_then(|o| o.as_dict_handle()).is_some_and(|d| {
                arena.dict_entry(d, type_key).and_then(|t| t.as_name()) == Some(page)
            })
        })
        .collect()
}

/// The annotations the absent pages carry, which go with them.
fn annotations_of(arena: &PdfArena, absent: &Absent) -> BTreeSet<Handle<Object>> {
    absent
        .iter()
        .filter_map(|page| arena.get_object(*page)?.as_dict_handle())
        .filter_map(|page| arena.dict_entry(page, arena.name("Annots")))
        .flat_map(|annots| match annots.resolve(arena) {
            Object::Array(array) => arena.get_array(array).unwrap_or_default(),
            _ => Vec::new(),
        })
        .filter_map(|annotation| annotation.as_reference())
        .collect()
}

/// The parent tree keys the absent pages and the dropped annotations held (14.7.5.4).
fn structure_keys(
    arena: &PdfArena,
    absent: &Absent,
    dropped: &BTreeSet<Handle<Object>>,
) -> BTreeSet<i64> {
    let key = |handle: &Handle<Object>, name: &str| {
        let dict = arena.get_object(*handle)?.as_dict_handle()?;
        arena.dict_entry(dict, arena.name(name))?.as_integer()
    };
    absent
        .iter()
        .filter_map(|page| key(page, "StructParents"))
        .chain(dropped.iter().filter_map(|annotation| key(annotation, "StructParent")))
        .collect()
}

/// Makes `null` every reference to an absent page that is left, in every dictionary and
/// array the arena holds — the guarantee that nothing reaches one.
fn forget_references(arena: &PdfArena, absent: &Absent) {
    let forget = |value: &mut Object| {
        if value.as_reference().is_some_and(|h| absent.contains(&h)) {
            *value = Object::Null;
            return true;
        }
        false
    };
    for handle in arena.all_dict_handles() {
        let Some(mut dict) = arena.get_dict(handle) else { continue };
        if dict.values_mut().fold(false, |changed, value| forget(value) | changed) {
            arena.set_dict(handle, dict);
        }
    }
    for handle in arena.all_array_handles() {
        let Some(mut array) = arena.get_array(handle) else { continue };
        if array.iter_mut().fold(false, |changed, value| forget(value) | changed) {
            arena.set_array(handle, array);
        }
    }
}

/// What decides whether a destination named an absent page.
struct Gone<'a> {
    arena: &'a PdfArena,
    absent: &'a Absent,
    named: &'a NamedDestinations,
}

type Dict = Handle<std::collections::BTreeMap<Handle<fepdf_model::PdfName>, Object>>;

impl Gone<'_> {
    /// Whether `dest` — written in place, or a name — reaches an absent page.
    fn destination(&self, dest: &Object) -> bool {
        match self.named.resolve(dest, self.arena) {
            Lookup::Inline(d) | Lookup::Named(d) => self.names_absent(&d),
            Lookup::Dangling(_) | Lookup::Unreadable => false,
        }
    }

    fn names_absent(&self, destination: &Destination) -> bool {
        matches!(destination.target, Target::Page(page) if self.absent.contains(&page))
    }

    /// Whether `action` is a go-to (12.6.4.2) whose destination reaches an absent page.
    fn action(&self, action: &Object) -> bool {
        let Some(dict) = action.resolve(self.arena).as_dict_handle() else { return false };
        let kind = self.arena.dict_entry(dict, self.arena.name("S")).and_then(|s| s.as_name());
        kind == Some(self.arena.name("GoTo"))
            && self
                .arena
                .dict_entry(dict, self.arena.name("D"))
                .is_some_and(|d| self.destination(&d))
    }

    /// Whether the dictionary `dict` goes to an absent page, by `/Dest` or by `/A`.
    fn goes(&self, dict: Dict) -> bool {
        let entry = |key: &str| self.arena.dict_entry(dict, self.arena.name(key));
        entry("Dest").is_some_and(|d| self.destination(&d))
            || entry("A").is_some_and(|a| self.action(&a))
    }

    /// Removes from each listed page the links that went to an absent one; answers them.
    fn links(&self, doc: &Document) -> BTreeSet<Handle<Object>> {
        let mut dropped = BTreeSet::new();
        for page in &doc.pages {
            let Some(page) = self.arena.get_object(*page).and_then(|o| o.as_dict_handle()) else {
                continue;
            };
            let Some(Object::Array(annots)) = self
                .arena
                .dict_entry(page, self.arena.name("Annots"))
                .map(|a| a.resolve(self.arena))
            else {
                continue;
            };
            let mut kept = self.arena.get_array(annots).unwrap_or_default();
            kept.retain(|annotation| {
                let goes = annotation
                    .resolve(self.arena)
                    .as_dict_handle()
                    .is_some_and(|d| self.goes(d) && self.subtype(d).as_deref() == Some("Link"));
                if goes {
                    dropped.extend(annotation.as_reference());
                }
                !goes
            });
            self.arena.set_array(annots, kept);
        }
        dropped
    }

    fn subtype(&self, dict: Dict) -> Option<String> {
        let name = self.arena.dict_entry(dict, self.arena.name("Subtype"))?.as_name()?;
        self.arena.get_name_str(name)
    }

    /// A bookmark to an absent page keeps its title and its children and loses where it
    /// went: its `/Dest`, or its `/A` when that is the go-to.
    fn outlines(&self, catalog: Dict) {
        let arena = self.arena;
        let Some(root) = arena.dict_entry(catalog, arena.name("Outlines")) else { return };
        let mut waiting = vec![(root, 0)];
        let mut seen = BTreeSet::new();
        while let Some((node, depth)) = waiting.pop() {
            let Some(dict) = node.resolve(arena).as_dict_handle() else { continue };
            if depth > DEPTH || !seen.insert(dict) {
                continue;
            }
            self.forget_destination(dict);
            for key in ["First", "Next"] {
                waiting.extend(arena.dict_entry(dict, arena.name(key)).map(|n| (n, depth + 1)));
            }
        }
    }

    /// Takes `/Dest`, and `/A` when it is a go-to, off `dict` where they reach an absent
    /// page.
    fn forget_destination(&self, dict: Dict) {
        let Some(mut entries) = self.arena.get_dict(dict) else { return };
        let (dest, action) = (self.arena.name("Dest"), self.arena.name("A"));
        let before = entries.len();
        entries.retain(|key, value| {
            !((*key == dest && self.destination(value)) || (*key == action && self.action(value)))
        });
        if entries.len() != before {
            self.arena.set_dict(dict, entries);
        }
    }

    /// An `/OpenAction` that opens on an absent page is removed (Table 29).
    fn open_action(&self, catalog: Dict) {
        let Some(mut entries) = self.arena.get_dict(catalog) else { return };
        let key = self.arena.name("OpenAction");
        let goes = entries.get(&key).is_some_and(|open| match open.resolve(self.arena) {
            Object::Array(_) => self.destination(open),
            _ => self.action(open),
        });
        if goes {
            entries.remove(&key);
            self.arena.set_dict(catalog, entries);
        }
    }

    /// Named destinations to an absent page are removed from the catalogue's `/Dests` and
    /// from the `/Names` `/Dests` tree (12.3.2.3).
    fn named_destinations(&self, catalog: Dict) {
        let arena = self.arena;
        let reads_absent =
            |value: &Object| Destination::read(value, arena).is_some_and(|d| self.names_absent(&d));
        if let Some(dests) = arena.dict_entry(catalog, arena.name("Dests"))
            && let Some(dests) = dests.resolve(arena).as_dict_handle()
            && let Some(mut entries) = arena.get_dict(dests)
        {
            entries.retain(|_, value| !reads_absent(value));
            arena.set_dict(dests, entries);
        }
        let tree = arena
            .dict_entry(catalog, arena.name("Names"))
            .and_then(|n| n.resolve(arena).as_dict_handle())
            .and_then(|n| arena.dict_entry(n, arena.name("Dests")));
        if let Some(tree) = tree {
            retain_in_name_tree(arena, tree, &|value| !reads_absent(value));
        }
    }
}

/// Keeps, in every leaf of the name tree `root` (7.9.6), the entries whose value `keep`
/// accepts. A leaf losing entries keeps its `/Limits`, which still bound what is left.
pub(crate) fn retain_in_name_tree(arena: &PdfArena, root: Object, keep: &dyn Fn(&Object) -> bool) {
    let mut waiting = vec![(root, 0)];
    let mut seen = BTreeSet::new();
    while let Some((node, depth)) = waiting.pop() {
        let Some(dict) = node.resolve(arena).as_dict_handle() else { continue };
        if depth > DEPTH || !seen.insert(dict) {
            continue;
        }
        let entry = |key: &str| match arena.dict_entry(dict, arena.name(key))?.resolve(arena) {
            Object::Array(array) => Some(array),
            _ => None,
        };
        if let Some(kids) = entry("Kids") {
            let kids = arena.get_array(kids).unwrap_or_default();
            waiting.extend(kids.into_iter().map(|kid| (kid, depth + 1)));
        } else if let Some(names) = entry("Names") {
            let pairs = arena.get_array(names).unwrap_or_default();
            let kept: Vec<Object> = pairs
                .chunks(2)
                .filter(|pair| pair.get(1).is_none_or(keep))
                .flatten()
                .cloned()
                .collect();
            arena.set_array(names, kept);
        }
    }
}
