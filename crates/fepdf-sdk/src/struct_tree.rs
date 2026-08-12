//! PDF Logical Structure Tree Traversal and Presentation Data (ISO 32000-2 Clause 14.7).

use crate::PdfDocument;
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
    /// Extracts the full presentation-ready structure tree from a PdfDocument.
    pub fn extract(doc: &PdfDocument) -> Option<StructureTreeNode> {
        let arena = doc.inner().arena();
        let cah = doc.inner().catalog_handle()?;
        let cadh = doc.inner().resolve_to_dict(cah).ok()?;
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

pub(crate) fn delete_struct_node(
    arena: &PdfArena,
    parent_handle: Handle<Object>,
    target_handle: Handle<Object>,
) -> bool {
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
            removed = delete_struct_node(arena, kid_ref, target_handle);
        }
    } else if let Object::Array(ah) = kids_obj.resolve(arena)
        && let Some(array) = arena.get_array(ah)
    {
        let mut new_kids = Vec::new();
        for kid in array {
            if let Some(kid_ref) = resolve_to_node_handle(arena, &kid) {
                if kid_ref == target_handle {
                    removed = true;
                } else {
                    if delete_struct_node(arena, kid_ref, target_handle) {
                        removed = true;
                    }
                    new_kids.push(kid);
                }
            } else {
                new_kids.push(kid);
            }
        }
        if removed {
            dict.insert(k_key, Object::Array(arena.alloc_array(new_kids)));
        }
    }

    if removed {
        arena.set_dict(dh, dict);
    }
    removed
}

fn build_page_handle_map(doc: &PdfDocument) -> BTreeMap<Handle<Object>, usize> {
    let mut map = BTreeMap::new();
    if let Ok(count) = doc.page_count() {
        for index in 0..count {
            if let Ok(page) = doc.get_page(index) {
                map.insert(page.obj_handle(), index);
            }
        }
    }
    map
}

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

fn parse_alt_text_helper(
    arena: &PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
) -> Option<String> {
    let alt_key = arena.name("Alt");
    let alt_obj = dict.get(&alt_key)?;
    let resolved = alt_obj.resolve(arena);
    let bytes = resolved.as_string()?;
    String::from_utf8(bytes.to_vec()).ok()
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
