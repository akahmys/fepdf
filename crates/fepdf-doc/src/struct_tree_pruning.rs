//! Structure content on pages taken out, taken out of the tree (ROADMAP Y-F18).
//!
//! **A kid goes when what it names went**: a mark whose page is absent — an integer `/K`
//! under an element whose `/Pg` is one, or an `/MCR` naming one — and an `/OBJR` whose
//! annotation went with its page or its link. **An element goes when that left it holding
//! nothing**, and so on up; one that held nothing before is left as the file wrote it.
//! The parent tree's entries for the absent pages and annotations go too, and the
//! `/IDTree`'s for the elements that went, since either would keep a removed element —
//! and its `/Alt` and `/ActualText` — reachable.

use fepdf_model::{Handle, Object, PdfArena};
use std::collections::BTreeSet;

/// How deep the walk goes (Rule 6).
const DEPTH: usize = 256;

/// Takes out of the tree under `root` what named `absent` pages or `dropped` annotations,
/// and the parent tree's entries under `keys`.
pub fn prune(
    arena: &PdfArena,
    root: Handle<Object>,
    absent: &BTreeSet<Handle<Object>>,
    dropped: &BTreeSet<Handle<Object>>,
    keys: &BTreeSet<i64>,
) {
    let marks = BTreeSet::new();
    let gone = walk(arena, root, (absent, dropped, &marks));
    crate::parent_tree::remove_keys(arena, root, keys);
    forget_in_ids(arena, root, &gone);
}

/// Takes out of the tree under `root` the marks `marks` names — each a page and an MCID —
/// and the elements that leaves holding nothing; and those elements from the parent tree
/// and the `/IDTree`, either of which would keep them, and their `/Alt` and `/ActualText`,
/// in the file (ROADMAP Y-10).
pub fn prune_marks(
    arena: &PdfArena,
    root: Handle<Object>,
    marks: &BTreeSet<(Handle<Object>, i64)>,
) {
    let none = BTreeSet::new();
    let gone = walk(arena, root, (&none, &none, marks));
    crate::parent_tree::forget_elements(arena, root, &gone);
    forget_in_ids(arena, root, &gone);
}

/// Prunes under `root` what names an absent page, a dropped annotation or a gone mark;
/// answers the elements taken out.
fn walk(
    arena: &PdfArena,
    root: Handle<Object>,
    (absent, dropped, marks): (
        &BTreeSet<Handle<Object>>,
        &BTreeSet<Handle<Object>>,
        &BTreeSet<(Handle<Object>, i64)>,
    ),
) -> BTreeSet<Handle<Object>> {
    let mut pruning =
        Pruning { arena, absent, dropped, marks, seen: BTreeSet::new(), gone: Vec::new() };
    pruning.seen.insert(root);
    pruning.node(root, None, 0);
    pruning.gone.into_iter().collect()
}

/// Takes `gone` out of the `/IDTree` under `root`.
fn forget_in_ids(arena: &PdfArena, root: Handle<Object>, gone: &BTreeSet<Handle<Object>>) {
    if gone.is_empty() {
        return;
    }
    let ids = arena
        .get_object(root)
        .and_then(|o| o.as_dict_handle())
        .and_then(|d| arena.dict_entry(d, arena.name("IDTree")));
    if let Some(ids) = ids {
        crate::page_removal::retain_in_name_tree(arena, ids, &|value| {
            !value.as_reference().is_some_and(|element| gone.contains(&element))
        });
    }
}

struct Pruning<'a> {
    arena: &'a PdfArena,
    absent: &'a BTreeSet<Handle<Object>>,
    dropped: &'a BTreeSet<Handle<Object>>,
    /// Marks gone from the content, each a page and an MCID.
    marks: &'a BTreeSet<(Handle<Object>, i64)>,
    seen: BTreeSet<Handle<Object>>,
    /// The elements taken out.
    gone: Vec<Handle<Object>>,
}

impl Pruning<'_> {
    /// Prunes `node`'s `/K`, on `page` unless it names its own; answers whether it held
    /// something and now holds nothing.
    fn node(&mut self, node: Handle<Object>, page: Option<Handle<Object>>, depth: usize) -> bool {
        let arena = self.arena;
        let Some(dict) = arena.get_object(node).and_then(|o| o.as_dict_handle()) else {
            return false;
        };
        let page = arena.dict_entry(dict, arena.name("Pg")).and_then(|p| p.as_reference()).or(page);
        let Some(k) = arena.dict_entry(dict, arena.name("K")) else { return false };
        let (array, kids) = match k.resolve(arena) {
            Object::Array(array) => (Some(array), arena.get_array(array).unwrap_or_default()),
            _ => (None, vec![k]),
        };
        let before = kids.len();
        let kept: Vec<Object> =
            kids.into_iter().filter(|kid| self.keeps(kid, page, depth)).collect();
        if kept.len() == before {
            return false;
        }
        let emptied = kept.is_empty();
        match array {
            Some(array) => arena.set_array(array, kept),
            None => {
                let mut entries = arena.get_dict(dict).unwrap_or_default();
                entries.remove(&arena.name("K"));
                arena.set_dict(dict, entries);
            }
        }
        emptied && before > 0
    }

    /// Whether `kid`, under an element on `page`, stays.
    fn keeps(&mut self, kid: &Object, page: Option<Handle<Object>>, depth: usize) -> bool {
        let arena = self.arena;
        let on_absent =
            |page: Option<Handle<Object>>| page.is_some_and(|p| self.absent.contains(&p));
        let gone = |page: Option<Handle<Object>>, mcid: Option<i64>| {
            page.zip(mcid).is_some_and(|mark| self.marks.contains(&mark))
        };
        let dict = match kid.resolve(arena) {
            Object::Integer(mcid) => return !on_absent(page) && !gone(page, Some(mcid)),
            Object::Dictionary(dict) => dict,
            _ => return true,
        };
        let entry = |key: &str| arena.dict_entry(dict, arena.name(key));
        let own_page = entry("Pg").and_then(|p| p.as_reference()).or(page);
        match entry("Type").and_then(|t| t.as_name()).and_then(|n| arena.get_name_str(n)).as_deref()
        {
            // A mark in a form's stream (`/Stm`) is not the page's mark of that number.
            Some("MCR") => {
                let mcid =
                    entry("MCID").and_then(|m| m.as_integer()).filter(|_| entry("Stm").is_none());
                !on_absent(own_page) && !gone(own_page, mcid)
            }
            Some("OBJR") => {
                let object = entry("Obj").and_then(|o| o.as_reference());
                !(on_absent(entry("Pg").and_then(|p| p.as_reference()))
                    || object.is_some_and(|o| self.dropped.contains(&o)))
            }
            _ => self.keeps_element(kid, page, depth),
        }
    }

    /// Whether the element `kid` stays: it does unless pruning it left it holding nothing.
    fn keeps_element(&mut self, kid: &Object, page: Option<Handle<Object>>, depth: usize) -> bool {
        let Some(element) = kid.as_reference() else { return true };
        if depth >= DEPTH || !self.seen.insert(element) {
            return true;
        }
        if self.node(element, page, depth + 1) {
            self.gone.push(element);
            return false;
        }
        true
    }
}
