//! The structure tree root's `/ParentTree` (14.7.5.4), read as the elements its entries
//! name.
//!
//! **Two kinds of entry.** A page's or a form's is an array — one element per `/MCID` on
//! it — and an annotation's or an XObject's `/StructParent` names the one element that
//! holds it through an `/OBJR`. The first lets content be asked which element it is in,
//! the second an annotation.

use fepdf_model::{Handle, Object, PdfArena};
use std::collections::{BTreeMap, BTreeSet};

/// How many nodes of a number tree are read before the rest is not (Rule 6).
const NODES: usize = 100_000;

/// The `/StructParent` keys whose entry is a single structure element, and that element.
#[must_use]
pub fn single_entries(arena: &PdfArena, root: Handle<Object>) -> BTreeMap<i64, Handle<Object>> {
    let mut entries = BTreeMap::new();
    walk(arena, root, |key, value, _| {
        // An array is a page's entry; a single element is what an annotation names.
        if let Some(element) = value.as_reference()
            && arena.get_object(element).and_then(|o| o.as_dict_handle()).is_some()
        {
            entries.insert(key, element);
        }
    });
    entries
}

/// The `/StructParents` keys whose entry is an array — a page's or a form's, one element
/// per `/MCID` — and that array's items, `null` where an MCID belongs to nothing.
#[must_use]
pub fn array_entries(arena: &PdfArena, root: Handle<Object>) -> BTreeMap<i64, Vec<Object>> {
    let mut entries = BTreeMap::new();
    walk(arena, root, |key, value, _| {
        if let Object::Array(array) = value.resolve(arena) {
            entries.insert(key, arena.get_array(array).unwrap_or_default());
        }
    });
    entries
}

/// The array the entry for `key` is — a page's or a form's, one element per `/MCID` — for
/// a caller to change in place.
#[must_use]
pub fn array_for(arena: &PdfArena, root: Handle<Object>, key: i64) -> Option<Handle<Vec<Object>>> {
    let mut found = None;
    walk(arena, root, |at, value, _| {
        if at == key
            && let Object::Array(array) = value.resolve(arena)
        {
            found = Some(array);
        }
    });
    found
}

/// Makes the entry for `key` name `element`: item `mcid` of the array it is — a page's or a
/// form's — or, with no MCID, the entry itself, an annotation's or an XObject's. Whether
/// there was such an entry.
pub fn rename(
    arena: &PdfArena,
    root: Handle<Object>,
    key: i64,
    mcid: Option<usize>,
    element: Handle<Object>,
) -> bool {
    let mut entry = None;
    walk(arena, root, |at, value, place| {
        if at == key {
            entry = Some((value.resolve(arena), place));
        }
    });
    let Some((value, (nums, at))) = entry else { return false };
    let (array, index) = match (mcid, value) {
        (Some(mcid), Object::Array(array)) => (array, mcid),
        (None, Object::Array(_)) | (Some(_), _) => return false,
        (None, _) => (nums, at),
    };
    let mut items = arena.get_array(array).unwrap_or_default();
    let Some(slot) = items.get_mut(index) else { return false };
    *slot = Object::Reference(element);
    arena.set_array(array, items);
    true
}

/// Takes the entries for `keys` out of the parent tree: a removed page's, and a removed
/// annotation's (ROADMAP Y-F18).
pub fn remove_keys(arena: &PdfArena, root: Handle<Object>, keys: &BTreeSet<i64>) {
    if keys.is_empty() {
        return;
    }
    let mut doomed: BTreeMap<Handle<Vec<Object>>, BTreeSet<usize>> = BTreeMap::new();
    walk(arena, root, |key, _, (nums, at)| {
        if keys.contains(&key) {
            doomed.entry(nums).or_default().insert(at / 2);
        }
    });
    for (nums, pairs) in doomed {
        let items = arena.get_array(nums).unwrap_or_default();
        let kept: Vec<Object> = items
            .chunks(2)
            .enumerate()
            .filter(|(pair, _)| !pairs.contains(pair))
            .flat_map(|(_, pair)| pair.iter().cloned())
            .collect();
        arena.set_array(nums, kept);
    }
}

/// Every key and value of the number tree, to [`NODES`] nodes, with the `/Nums` array the
/// value is in and its place there.
fn walk(
    arena: &PdfArena,
    root: Handle<Object>,
    mut found: impl FnMut(i64, &Object, (Handle<Vec<Object>>, usize)),
) {
    let Some(root) = arena.get_object(root).and_then(|o| o.as_dict_handle()) else { return };
    let Some(tree) = arena.dict_entry(root, arena.name("ParentTree")) else { return };
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
        let array =
            |key: &str| match arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena)) {
                Some(Object::Array(array)) => Some(array),
                _ => None,
            };
        let listed = |array: Option<Handle<Vec<Object>>>| {
            array.and_then(|a| arena.get_array(a)).unwrap_or_default()
        };
        waiting.extend(listed(array("Kids")));
        let nums = array("Nums");
        for (i, [key, value]) in listed(nums).as_chunks::<2>().0.iter().enumerate() {
            if let (Object::Integer(key), Some(nums)) = (key, nums) {
                found(*key, value, (nums, 2 * i + 1));
            }
        }
    }
}
