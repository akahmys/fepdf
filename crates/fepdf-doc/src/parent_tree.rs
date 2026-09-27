//! The structure tree root's `/ParentTree` (14.7.5.4), read as the elements its single
//! entries name.
//!
//! **Only the entries that name one element.** A page's entry is an array — one element
//! per `/MCID` on it — and an annotation's or an XObject's `/StructParent` names the one
//! element that holds it through an `/OBJR`. The second kind is what lets an annotation be
//! asked which element it belongs to.

use fepdf_model::{Handle, Object, PdfArena};
use std::collections::{BTreeMap, BTreeSet};

/// How many nodes of a number tree are read before the rest is not (Rule 6).
const NODES: usize = 100_000;

/// The `/StructParent` keys whose entry is a single structure element, and that element.
#[must_use]
pub fn single_entries(arena: &PdfArena, root: Handle<Object>) -> BTreeMap<i64, Handle<Object>> {
    let mut entries = BTreeMap::new();
    let Some(root) = arena.get_object(root).and_then(|o| o.as_dict_handle()) else {
        return entries;
    };
    let Some(tree) = arena.dict_entry(root, arena.name("ParentTree")) else { return entries };
    let mut waiting = vec![tree];
    let mut seen = BTreeSet::new();
    let mut read = 0;
    while let Some(node) = waiting.pop() {
        read += 1;
        if read > NODES {
            break;
        }
        if let Object::Reference(handle) = node
            && !seen.insert(handle)
        {
            continue;
        }
        let Some(dict) = node.resolve(arena).as_dict_handle() else { continue };
        let listed =
            |key: &str| match arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena)) {
                Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
                _ => Vec::new(),
            };
        waiting.extend(listed("Kids"));
        for pair in listed("Nums").chunks_exact(2) {
            let (Object::Integer(key), Some(element)) = (&pair[0], pair[1].as_reference()) else {
                continue;
            };
            // An array is a page's entry; a single element is what an annotation names.
            if arena.get_object(element).and_then(|o| o.as_dict_handle()).is_some() {
                entries.insert(*key, element);
            }
        }
    }
    entries
}
