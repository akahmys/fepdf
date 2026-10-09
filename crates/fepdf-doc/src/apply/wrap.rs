//! A new structure element round existing content (14.7.2).
//!
//! **Nothing could make one.** WTPDF requires elements a tagged file often lacks — the
//! `Caption` of a `Figure`, the `Lbl` and `LBody` of an `LI`, the `RB`, `RT` and `RP` of a
//! `Ruby` (8.2.5) — and the vocabulary could retag, move and delete elements but not add
//! one between an element and what it holds.

use super::structure::{element_dict, not_an_element};
use crate::operation::StructElemWrap;
use fepdf_model::{Document, Handle, Object, PdfArena, PdfError, PdfResult};
use std::collections::BTreeMap;

/// What a kid is, for the new element to take it over.
enum Kid {
    /// An element, whose `/P` becomes the new one.
    Element(Handle<Object>),
    /// Content the parent tree names: under `key`, item `mcid` of a page's or a form's
    /// array, or, with no MCID, the entry an annotation's or an `XObject`'s is.
    Content { key: i64, mcid: Option<usize> },
    /// Content the parent tree has no entry for, so there is nothing to rename.
    Unlisted,
}

/// Wraps `count` of an element's kids, from `first`, in a new element tagged `tag`, which
/// takes their place.
///
/// **What the kids were in, they are now in through the new element**: an element's `/P`
/// becomes it, and the parent tree's entry for content — an MCID, a marked-content
/// reference, an object reference — names it. It takes the parent's `/Pg`, so an MCID
/// stays on the page it was on.
///
/// # Errors
///
/// When the document has no structure tree, `handle_index` names neither an element nor
/// its root, the kids are not there, or an MCID has no page to be found on — each before
/// anything is written.
pub fn apply_wrap_struct(doc: &Document, wrap: StructElemWrap) -> PdfResult<()> {
    let StructElemWrap { handle_index, first, count, tag } = wrap;
    let arena = doc.arena();
    let root = doc
        .get_structure_root()?
        .ok_or_else(|| PdfError::refused("WrapStructElem", "the document has no structure tree"))?;
    let Some((dh, mut dict)) = element_dict(arena, handle_index) else {
        return Err(not_an_element(handle_index));
    };
    if root.index() != handle_index && !dict.contains_key(&arena.name("S")) {
        return Err(not_an_element(handle_index));
    }
    let key = arena.name("K");
    let mut kids = dict.get(&key).map(|k| kids(arena, k)).unwrap_or_default();
    let Some(end) = first.checked_add(count).filter(|&end| count > 0 && end <= kids.len()) else {
        return Err(PdfError::refused(
            "WrapStructElem",
            format!(
                "object {handle_index} has {} kids, and {count} from {first} is not a run of them",
                kids.len()
            ),
        ));
    };
    let page = dict.get(&arena.name("Pg")).cloned();
    let wrapped = kids.get(first..end).unwrap_or_default().to_vec();
    let found =
        wrapped.iter().map(|kid| what(arena, kid, page.as_ref())).collect::<PdfResult<Vec<_>>>()?;

    let mut entries = BTreeMap::new();
    entries.insert(arena.name("Type"), Object::Name(arena.name("StructElem")));
    entries.insert(arena.name("S"), Object::Name(arena.name(&tag)));
    entries.insert(arena.name("P"), Object::Reference(Handle::new(handle_index)));
    if let Some(page) = page {
        entries.insert(arena.name("Pg"), page);
    }
    entries.insert(key, Object::Array(arena.alloc_array(wrapped)));
    let element = arena.alloc_object(Object::Dictionary(arena.alloc_dict(entries)));
    kids.splice(first..end, [Object::Reference(element)]);
    dict.insert(key, Object::Array(arena.alloc_array(kids)));
    arena.set_dict(dh, dict);

    adopt(arena, root, element, found);
    Ok(())
}

/// Makes `element` what each of `found` is in: an element's `/P`, and the parent tree's
/// entry for content.
fn adopt(arena: &PdfArena, root: Handle<Object>, element: Handle<Object>, found: Vec<Kid>) {
    for kid in found {
        match kid {
            Kid::Element(kid) => {
                if let Some(kid) = arena.get_object(kid).and_then(|k| k.as_dict_handle()) {
                    let mut kid_dict = arena.get_dict(kid).unwrap_or_default();
                    kid_dict.insert(arena.name("P"), Object::Reference(element));
                    arena.set_dict(kid, kid_dict);
                }
            }
            Kid::Content { key, mcid } => {
                crate::parent_tree::rename(arena, root, key, mcid, element);
            }
            Kid::Unlisted => {}
        }
    }
}

/// An element's `/K` as a list of kids, each as written — **not resolved**, so a kid that is
/// a reference to an element stays one.
pub(super) fn kids(arena: &PdfArena, k: &Object) -> Vec<Object> {
    match k.resolve(arena) {
        Object::Array(array) => arena.get_array(array).unwrap_or_default(),
        Object::Null => Vec::new(),
        _ => vec![k.clone()],
    }
}

/// What `kid` is, and where the parent tree names it; `page` is its element's `/Pg`.
fn what(arena: &PdfArena, kid: &Object, page: Option<&Object>) -> PdfResult<Kid> {
    let key_of = |holder: Option<Object>, key: &str| {
        let holder = holder?.as_reference()?;
        let dict = arena.get_object(holder)?.as_dict_handle()?;
        arena.dict_entry(dict, arena.name(key))?.as_integer()
    };
    let content = |key: Option<i64>, mcid: Option<usize>| {
        key.map_or(Kid::Unlisted, |key| Kid::Content { key, mcid })
    };
    let no_page =
        || PdfError::violation("14.7.2", "a marked-content kid has no /Pg to be found on");
    let resolved = kid.resolve(arena);
    if let Object::Integer(mcid) = resolved {
        let page = page.cloned().ok_or_else(no_page)?;
        return Ok(content(key_of(Some(page), "StructParents"), usize::try_from(mcid).ok()));
    }
    let Some(dict) = resolved.as_dict_handle() else { return Ok(Kid::Unlisted) };
    let entry = |key: &str| arena.dict_entry(dict, arena.name(key));
    if entry("S").is_some() {
        return Ok(kid.as_reference().map_or(Kid::Unlisted, Kid::Element));
    }
    if let Some(mcid) = entry("MCID").and_then(|m| m.as_integer()) {
        let mcid = usize::try_from(mcid).ok();
        if let Some(stream) = entry("Stm") {
            return Ok(content(key_of(Some(stream), "StructParents"), mcid));
        }
        let page = entry("Pg").or_else(|| page.cloned()).ok_or_else(no_page)?;
        return Ok(content(key_of(Some(page), "StructParents"), mcid));
    }
    Ok(content(key_of(entry("Obj"), "StructParent"), None))
}
