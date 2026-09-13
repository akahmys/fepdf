use fepdf_model::Document;
use fepdf_model::arena::PdfArena;
use fepdf_model::handle::Handle;
use fepdf_model::object::{Object, PdfName};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Presentation-ready structure tree node for frontends (GUI/MCP/CLI).
/// Contains zero raw arena pointers.
pub struct StructureTreeNode {
    /// Unique identifier within the presentation hierarchy.
    pub id: usize,
    /// Structure element tag name (e.g., "P", "H1", "Document").
    pub tag: String,
    /// Display title or tag name.
    pub title: String,
    /// Alternative text description (/Alt entry).
    pub alt_text: Option<String>,
    /// Bounding box rectangle ([llx, lly, urx, ury]) if specified (/BBox).
    pub rect: Option<[f32; 4]>,
    /// Resolved zero-based target page index (from /Pg entry or inherited).
    pub page_index: Option<usize>,
    /// Handle index of the underlying PdfArena object.
    pub handle_index: Option<u32>,
    /// Child nodes in the structure hierarchy.
    pub children: Vec<StructureTreeNode>,
}

/// Visitor that extracts structure tree information from a document.
pub struct StructureTreeVisitor;

impl StructureTreeVisitor {
    /// Extracts the full presentation-ready structure tree from a Document.
    pub fn extract(doc: &Document) -> Option<StructureTreeNode> {
        let arena = doc.arena();
        let cah = doc.catalog_handle()?;
        let cadh = doc.resolve_to_dict(cah).ok()?;
        let dict = arena.get_dict(cadh)?;
        let str_root_key = arena.name("StructTreeRoot");
        let str_root_obj = dict.get(&str_root_key)?;
        let str_root_ref = resolve_to_node_handle(arena, str_root_obj)?;
        let page_map = build_page_handle_map(doc);
        let mut visited = BTreeSet::new();
        let mut next_id = 0;
        parse_struct_node(arena, str_root_ref, &mut next_id, &mut visited, &page_map, None)
    }
}

pub(crate) fn resolve_to_node_handle(arena: &PdfArena, obj: &Object) -> Option<Handle<Object>> {
    match obj {
        Object::Reference(h) => Some(*h),
        Object::Dictionary(dh) => Some(Handle::new(dh.index())),
        _ => {
            let resolved = obj.resolve(arena);
            match resolved {
                Object::Reference(h) => Some(h),
                Object::Dictionary(dh) => Some(Handle::new(dh.index())),
                _ => None,
            }
        }
    }
}

/// How deep a `/K` chain is followed before it is taken to be looping.
///
/// The same 64 as the field-tree and `/Next` walks in `fepdf-model`, and a depth rather
/// than a visited set for the reason [ADR-0060] gives: this is a tree, where meeting the
/// same node twice can be legitimate, and a set would prune what it should keep.
///
/// [ADR-0060]: ../../../docs/adr/0060-a-reference-chain-is-bounded-by-what-it-has-seen.md
const MAX_STRUCTURE_DEPTH: usize = 64;

/// Removes `target_handle` from wherever it hangs under `parent_handle`.
///
/// **Bounded since 2026-09-05.** A structure tree whose `/K` named an ancestor made this
/// follow it forever: `Operation::DeleteStructElem` with a handle no element carries has
/// to exhaust the tree, and `fatal runtime error: stack overflow` is what it did instead.
/// RR-15 Rule 6, whose enforcement column says "Code review".
pub(crate) fn delete_struct_node(
    arena: &PdfArena,
    parent_handle: Handle<Object>,
    target_handle: Handle<Object>,
) -> bool {
    delete_struct_node_at(arena, parent_handle, target_handle, 0)
}

/// The kids of one node with `target_handle` gone, or `None` if it was not under them.
///
/// Split out of [`delete_struct_node_at`] to keep that under RR-15 Rule 1's fifty lines
/// once it took a depth, not because the two do different jobs.
fn delete_from_kids(
    arena: &PdfArena,
    kids: &[Object],
    target_handle: Handle<Object>,
    depth: usize,
) -> Option<Vec<Object>> {
    let mut removed = false;
    let mut kept = Vec::new();
    for kid in kids {
        let Some(kid_ref) = resolve_to_node_handle(arena, kid) else {
            kept.push(kid.clone());
            continue;
        };
        if kid_ref == target_handle {
            removed = true;
            continue;
        }
        if delete_struct_node_at(arena, kid_ref, target_handle, depth + 1) {
            removed = true;
        }
        kept.push(kid.clone());
    }
    removed.then_some(kept)
}

fn delete_struct_node_at(
    arena: &PdfArena,
    parent_handle: Handle<Object>,
    target_handle: Handle<Object>,
    depth: usize,
) -> bool {
    if depth >= MAX_STRUCTURE_DEPTH {
        return false;
    }
    let Some(obj) = arena.get_object(parent_handle) else {
        return false;
    };
    let Some(dh) = obj.as_dict_handle() else {
        return false;
    };
    let Some(mut dict) = arena.get_dict(dh) else {
        return false;
    };

    let k_key = arena.name("K");
    let Some(kids_obj) = dict.get(&k_key).cloned() else {
        return false;
    };

    let mut removed = false;
    if let Some(kid_ref) = resolve_to_node_handle(arena, &kids_obj) {
        if kid_ref == target_handle {
            dict.remove(&k_key);
            removed = true;
        } else {
            removed = delete_struct_node_at(arena, kid_ref, target_handle, depth + 1);
        }
    } else if let Object::Array(ah) = kids_obj.resolve(arena)
        && let Some(array) = arena.get_array(ah)
        && let Some(kept) = delete_from_kids(arena, &array, target_handle, depth)
    {
        dict.insert(k_key, Object::Array(arena.alloc_array(kept)));
        removed = true;
    }

    if removed {
        arena.set_dict(dh, dict);
    }
    removed
}

fn build_page_handle_map(doc: &Document) -> BTreeMap<Handle<Object>, usize> {
    let mut map = BTreeMap::new();
    if let Ok(count) = doc.page_count() {
        for index in 0..count {
            if let Some(page_h) = doc.get_page_handle(index) {
                map.insert(page_h, index);
            }
        }
    }
    map
}

#[allow(clippy::cast_possible_truncation)]
fn parse_bbox_helper(arena: &PdfArena, bbox_obj: &Object) -> Option<[f32; 4]> {
    let array_h = bbox_obj.resolve(arena).as_array()?;
    let arr = arena.get_array(array_h)?;
    if arr.len() != 4 {
        return None;
    }
    let x1 = arr[0].resolve(arena).as_f64().unwrap_or(0.0) as f32;
    let y1 = arr[1].resolve(arena).as_f64().unwrap_or(0.0) as f32;
    let x2 = arr[2].resolve(arena).as_f64().unwrap_or(0.0) as f32;
    let y2 = arr[3].resolve(arena).as_f64().unwrap_or(0.0) as f32;
    Some([x1, y1, x2, y2])
}

fn parse_kids_helper(
    arena: &PdfArena,
    kids_obj: &Object,
    next_id: &mut usize,
    visited: &mut BTreeSet<Handle<Object>>,
    children: &mut Vec<StructureTreeNode>,
    page_map: &BTreeMap<Handle<Object>, usize>,
    inherited_page: Option<usize>,
) {
    if let Some(kid_ref) = resolve_to_node_handle(arena, kids_obj) {
        if let Some(child_node) =
            parse_struct_node(arena, kid_ref, next_id, visited, page_map, inherited_page)
        {
            children.push(child_node);
        }
    } else if let Object::Array(ah) = kids_obj.resolve(arena)
        && let Some(array) = arena.get_array(ah)
    {
        for kid in array {
            if let Some(kid_ref) = resolve_to_node_handle(arena, &kid)
                && let Some(child_node) =
                    parse_struct_node(arena, kid_ref, next_id, visited, page_map, inherited_page)
            {
                children.push(child_node);
            }
        }
    }
}

fn parse_tag_helper(arena: &PdfArena, dict: &BTreeMap<Handle<PdfName>, Object>) -> String {
    let type_key = arena.name("Type");
    let s_key = arena.name("S");

    if let Some(s_obj) = dict.get(&s_key) {
        let resolved = s_obj.resolve(arena);
        if let Some(name_h) = resolved.as_name() {
            arena.get_name(name_h).map_or_else(|| "P".to_string(), |n| n.as_str().to_string())
        } else {
            "P".to_string()
        }
    } else {
        let type_val = dict.get(&type_key).and_then(|t: &Object| t.resolve(arena).as_name());
        if let Some(tv) = type_val {
            if arena.get_name(tv).is_some_and(|n| n.as_str() == "StructTreeRoot") {
                "Document".to_string()
            } else {
                "P".to_string()
            }
        } else {
            "P".to_string()
        }
    }
}

/// A structure element's `/Alt`, decoded as 7.9.2.2 defines rather than as UTF-8.
///
/// **This read both halves of a text string wrong.** `as_string` answers `None` for an
/// `Object::Text`, which is what `apply::structure` now writes, and `String::from_utf8`
/// answers `None` for the UTF-16BE a file is most likely to carry — so a `/Alt` of `代替`
/// was dropped whether it came from this engine or from another one. Only a description
/// that was already ASCII survived, which is the same class of defect as writing one as
/// raw UTF-8 bytes.
fn parse_alt_text_helper(
    arena: &PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
) -> Option<String> {
    let alt_key = arena.name("Alt");
    match dict.get(&alt_key)?.resolve(arena) {
        Object::Text(text) => Some(text),
        Object::String(bytes) | Object::Hex(bytes) => {
            Some(fepdf_model::refine::text::recover_string(&bytes))
        }
        _ => None,
    }
}

fn parse_page_index_helper(
    arena: &PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
    page_map: &BTreeMap<Handle<Object>, usize>,
) -> Option<usize> {
    let pg_obj = dict.get(&arena.name("Pg"))?;
    let pg_ref = resolve_to_node_handle(arena, pg_obj)?;
    page_map.get(&pg_ref).copied()
}

fn parse_struct_node(
    arena: &PdfArena,
    handle: Handle<Object>,
    next_id: &mut usize,
    visited: &mut BTreeSet<Handle<Object>>,
    page_map: &BTreeMap<Handle<Object>, usize>,
    inherited_page: Option<usize>,
) -> Option<StructureTreeNode> {
    if !visited.insert(handle) {
        return None;
    }
    let obj = arena.get_object(handle)?;
    let dh = obj.as_dict_handle()?;
    let dict = arena.get_dict(dh)?;

    let tag = parse_tag_helper(arena, &dict);
    let title = tag.clone();
    let alt_text = parse_alt_text_helper(arena, &dict);

    let rect = dict.get(&arena.name("BBox")).and_then(|b| parse_bbox_helper(arena, b));
    let page_index = parse_page_index_helper(arena, &dict, page_map).or(inherited_page);

    let id = *next_id;
    *next_id += 1;

    let mut children = Vec::new();
    if let Some(kids) = dict.get(&arena.name("K")) {
        parse_kids_helper(arena, kids, next_id, visited, &mut children, page_map, page_index);
    }

    visited.remove(&handle);

    Some(StructureTreeNode {
        id,
        tag,
        title,
        alt_text,
        rect,
        page_index,
        handle_index: Some(handle.index()),
        children,
    })
}
