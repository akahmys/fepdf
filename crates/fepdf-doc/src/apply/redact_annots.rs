//! The annotations a redaction region meets, removed with what carries their content
//! (12.5.6.23, ROADMAP Y-10).
//!
//! **An annotation is not drawn into the page, so cutting the content leaves it.** A note's
//! `/Contents`, a link's address, a field's value: each is in the annotation or in what
//! hangs from it. One whose `/Rect` meets a region is taken off the page, and with it:
//!
//! - its `/Popup`, and every annotation that replies to it by `/IRT`, which carry
//!   `/Contents` of their own, on whichever page they are;
//! - for a widget, its place in the field tree. A field keeps its value in the field and
//!   not in the widget, so removing the widget leaves `/V` written. A field whose widgets
//!   all go goes from the tree — its parent's `/Kids`, or `/Fields` — and from `/CO`; one
//!   with a widget elsewhere keeps its value, which is shown there;
//! - the form's `/XFA`, which holds every field's value again as XML that a save writes
//!   untouched, with a `Decision`;
//! - the structure tree's references to them, which would keep them reachable.

use super::text::GlyphBox;
use fepdf_model::interpretation::Decision;
use fepdf_model::{DictHandle, Document, Handle, Object, PdfArena, PdfResult};
use std::collections::BTreeSet;

/// How many popups and replies deep the removal follows, and how deep a field tree is
/// climbed (Rule 6).
const DEPTH: usize = 64;

/// The `/Rect` of every annotation on `page` that a region meets, on the page.
///
/// # Errors
/// Fails when the page is not there.
pub fn meeting(doc: &Document, page: usize, regions: &[GlyphBox]) -> PdfResult<Vec<GlyphBox>> {
    let arena = doc.arena();
    Ok(annotations_on(doc, page)?
        .iter()
        .filter_map(|a| rect_of(arena, a))
        .filter(|r| regions.iter().any(|g| super::text::meets(*r, *g)))
        .collect())
}

/// Removes every annotation on `page` that a region meets, and what goes with it.
///
/// # Errors
/// Fails when the page is not there, or the structure tree will not read.
pub fn remove(doc: &Document, page: usize, regions: &[GlyphBox]) -> PdfResult<()> {
    let arena = doc.arena();
    let gone: BTreeSet<Handle<Object>> = annotations_on(doc, page)?
        .iter()
        .filter(|a| {
            rect_of(arena, a).is_some_and(|r| regions.iter().any(|g| super::text::meets(r, *g)))
        })
        .filter_map(Object::as_reference)
        .collect();
    remove_handles(doc, gone)
}

/// Removes the annotations `gone` names, with their pop-ups and replies, their places in
/// the field tree, and the structure tree's references to them.
///
/// # Errors
/// Fails when a page is not there, or the structure tree will not read.
pub fn remove_handles(doc: &Document, mut gone: BTreeSet<Handle<Object>>) -> PdfResult<()> {
    let arena = doc.arena();
    if gone.is_empty() {
        return Ok(());
    }
    with_popups_and_replies(doc, &mut gone);
    for page in 0..doc.page_count()? {
        take_off(doc, page, &gone)?;
    }
    let widgets: Vec<Handle<Object>> =
        gone.iter().copied().filter(|a| is_widget(arena, *a)).collect();
    if !widgets.is_empty() {
        forget_fields(doc, &widgets);
    }
    if let Some(root) = doc.get_structure_root()? {
        let keys = gone.iter().filter_map(|a| integer(arena, *a, "StructParent")).collect();
        crate::struct_tree_pruning::prune(arena, root, &BTreeSet::new(), &gone, &keys);
    }
    Ok(())
}

/// The entries of `page`'s `/Annots`.
pub fn annotations_on(doc: &Document, page: usize) -> PdfResult<Vec<Object>> {
    let arena = doc.arena();
    let page_dict = doc.resolve_to_dict(doc.page_handle(page)?)?;
    Ok(match arena.dict_entry(page_dict, arena.name("Annots")).map(|a| a.resolve(arena)) {
        Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
        _ => Vec::new(),
    })
}

/// An annotation's `/Rect`, put in order.
fn rect_of(arena: &PdfArena, annotation: &Object) -> Option<GlyphBox> {
    let dict = annotation.resolve(arena).as_dict_handle()?;
    let rect = arena.dict_entry(dict, arena.name("Rect"))?.resolve(arena).as_array()?;
    let n: Vec<f64> =
        arena.get_array(rect)?.iter().filter_map(|v| v.resolve(arena).as_f64()).collect();
    match n[..] {
        [x0, y0, x1, y1] => Some((x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1))),
        _ => None,
    }
}

/// The dictionary behind an annotation's handle.
fn dict_of(arena: &PdfArena, handle: Handle<Object>) -> Option<DictHandle> {
    arena.get_object(handle)?.as_dict_handle()
}

fn integer(arena: &PdfArena, handle: Handle<Object>, key: &str) -> Option<i64> {
    arena.dict_entry(dict_of(arena, handle)?, arena.name(key))?.as_integer()
}

fn is_widget(arena: &PdfArena, handle: Handle<Object>) -> bool {
    dict_of(arena, handle)
        .and_then(|d| arena.dict_entry(d, arena.name("Subtype")))
        .and_then(|s| s.as_name())
        == Some(arena.name("Widget"))
}

/// Adds to `gone` each one's `/Popup`, and every annotation on any page whose `/IRT`
/// names one of them, until nothing more is added.
fn with_popups_and_replies(doc: &Document, gone: &mut BTreeSet<Handle<Object>>) {
    let arena = doc.arena();
    let every: Vec<Handle<Object>> = (0..doc.pages.len())
        .filter_map(|page| annotations_on(doc, page).ok())
        .flatten()
        .filter_map(|a| a.as_reference())
        .collect();
    let reference = |handle: Handle<Object>, key: &str| {
        arena.dict_entry(dict_of(arena, handle)?, arena.name(key))?.as_reference()
    };
    for _ in 0..DEPTH {
        let popups = gone.iter().filter_map(|a| reference(*a, "Popup"));
        let replies = every
            .iter()
            .copied()
            .filter(|a| reference(*a, "IRT").is_some_and(|to| gone.contains(&to)));
        let more: Vec<Handle<Object>> =
            popups.chain(replies).filter(|a| !gone.contains(a)).collect();
        if more.is_empty() {
            return;
        }
        gone.extend(more);
    }
}

/// Takes `gone` out of `page`'s `/Annots`.
fn take_off(doc: &Document, page: usize, gone: &BTreeSet<Handle<Object>>) -> PdfResult<()> {
    let arena = doc.arena();
    let page_dict = doc.resolve_to_dict(doc.page_handle(page)?)?;
    let key = arena.name("Annots");
    let Some(Object::Array(array)) = arena.dict_entry(page_dict, key).map(|a| a.resolve(arena))
    else {
        return Ok(());
    };
    let mut kept = arena.get_array(array).unwrap_or_default();
    let before = kept.len();
    kept.retain(|a| a.as_reference().is_none_or(|h| !gone.contains(&h)));
    if kept.len() != before {
        let mut dict = arena.get_dict(page_dict).unwrap_or_default();
        dict.insert(key, Object::Array(arena.alloc_array(kept)));
        arena.set_dict(page_dict, dict);
    }
    Ok(())
}

/// Takes each of `widgets` out of the field tree, and each field left with no widget;
/// then the form's `/XFA`, with a `Decision`.
fn forget_fields(doc: &Document, widgets: &[Handle<Object>]) {
    let arena = doc.arena();
    let Some(form) = doc
        .catalog_handle()
        .and_then(|c| doc.resolve_to_dict(c).ok())
        .and_then(|c| arena.dict_entry(c, arena.name("AcroForm")))
        .and_then(|f| f.resolve(arena).as_dict_handle())
    else {
        return;
    };
    for widget in widgets {
        forget_node(arena, form, *widget, 0);
    }
    let mut dict = arena.get_dict(form).unwrap_or_default();
    if dict.remove(&arena.name("XFA")).is_some() {
        arena.set_dict(form, dict);
        doc.decisions.push(Decision::repaired(
            "12.7.3",
            "the form carries /XFA, which holds its fields' values again, redacted ones with them",
            "removed it; the form's fields are what is left",
        ));
    }
}

/// Takes `node` out of its parent's `/Kids`, or out of `/Fields` where it has no parent,
/// and out of `/CO`; then its parent too where that leaves it no kids.
fn forget_node(arena: &PdfArena, form: DictHandle, node: Handle<Object>, depth: usize) {
    let parent = dict_of(arena, node)
        .and_then(|d| arena.dict_entry(d, arena.name("Parent")))
        .and_then(|p| p.as_reference());
    let (holder, key) = match parent {
        Some(parent) => (dict_of(arena, parent), "Kids"),
        None => (Some(form), "Fields"),
    };
    let left = holder.map_or(1, |h| without(arena, h, key, node));
    without(arena, form, "CO", node);
    if left == 0
        && depth < DEPTH
        && let Some(parent) = parent
    {
        forget_node(arena, form, parent, depth + 1);
    }
}

/// Takes `node` out of the array at `key` in `holder`; how many entries are left.
fn without(arena: &PdfArena, holder: DictHandle, key: &str, node: Handle<Object>) -> usize {
    let name = arena.name(key);
    let Some(Object::Array(array)) = arena.dict_entry(holder, name).map(|a| a.resolve(arena))
    else {
        return 0;
    };
    let mut items = arena.get_array(array).unwrap_or_default();
    let before = items.len();
    items.retain(|item| item.as_reference() != Some(node));
    if items.len() != before {
        arena.set_array(array, items.clone());
    }
    items.len()
}
