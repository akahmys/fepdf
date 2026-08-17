#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
use crate::operation::{PageLabelSpec, PageLabelStyle, PageSelection, RotateMode};
use bytes::Bytes;
use fepdf_model::{Document, Object, PdfResult};
use std::collections::BTreeMap;

/// Applies page rotation to selected pages.
pub fn apply_rotate(doc: &mut Document, pages: &PageSelection, mode: &RotateMode) -> PdfResult<()> {
    let count = doc.page_count()?;
    let indices = match pages {
        PageSelection::All => (0..count).collect(),
        PageSelection::Single(i) => vec![*i],
        PageSelection::Indices(idx) => idx.clone(),
    };
    for idx in indices {
        if idx < count {
            let page = doc.get_page(idx)?;
            let current = if let Some(Object::Integer(angle)) = page.resolve_attribute("Rotate") {
                let normalized = (angle % 360) as i32;
                normalized.rem_euclid(360)
            } else {
                0
            };
            let target = match mode {
                RotateMode::Absolute(q) => q.to_degrees(),
                RotateMode::Relative(q) => (current + q.to_degrees()).rem_euclid(360),
            };
            let page_dh = doc.resolve_to_dict(page.obj_handle())?;
            let arena = doc.arena();
            let mut dict = arena.get_dict(page_dh).unwrap_or_default();
            dict.insert(arena.name("Rotate"), Object::Integer(i64::from(target)));
            arena.set_dict(page_dh, dict);
        }
    }
    Ok(())
}

/// Moves a page from one index to another.
pub fn apply_reorder(doc: &mut Document, from: usize, to: usize) -> PdfResult<()> {
    let count = doc.page_count()?;
    if from >= count || to >= count {
        return Err(fepdf_model::PdfError::Arena("Page index out of bounds".into()));
    }
    doc.reorder_page(from, to)
}

fn prune_struct_tree_pages(doc: &Document, removed: &[fepdf_model::Handle<Object>]) {
    if removed.is_empty() {
        return;
    }
    let Ok(Some(root_h)) = doc.get_structure_root() else {
        return;
    };
    let removed_set: std::collections::BTreeSet<_> = removed.iter().copied().collect();
    let mut visitor = crate::structure::StructureVisitor::new(doc.arena(), root_h);
    let pg_key = doc.arena().name("Pg");

    while let Some(elem_h) = visitor.next_element() {
        if let Some(dh) = doc.arena().get_object(elem_h).and_then(|o| o.as_dict_handle())
            && let Some(mut dict) = doc.arena().get_dict(dh)
            && let Some(Object::Reference(h)) = dict.get(&pg_key)
            && removed_set.contains(h)
        {
            dict.remove(&pg_key);
            doc.arena().set_dict(dh, dict);
        }
    }
}

/// Removes selected pages from the document.
pub fn apply_remove_pages(doc: &mut Document, pages: &PageSelection) -> PdfResult<()> {
    let count = doc.page_count()?;
    let mut indices = match pages {
        PageSelection::All => (0..count).collect(),
        PageSelection::Single(i) => vec![*i],
        PageSelection::Indices(idx) => idx.clone(),
    };
    indices.sort_unstable();
    indices.dedup();
    let mut removed_handles = Vec::new();
    for &idx in &indices {
        if let Some(h) = doc.get_page_handle(idx) {
            removed_handles.push(h);
        }
    }
    for idx in indices.into_iter().rev() {
        if idx < count {
            doc.remove_page(idx)?;
        }
    }
    prune_struct_tree_pages(doc, &removed_handles);
    Ok(())
}

/// Applies page label ranges (Table 159).
pub fn apply_set_page_labels(doc: &Document, labels: Vec<PageLabelSpec>) -> PdfResult<()> {
    let arena = doc.arena();
    let mut nums_items = Vec::new();

    for spec in labels {
        let mut label_dict = BTreeMap::new();
        label_dict.insert(arena.name("Type"), Object::Name(arena.name("PageLabel")));
        let style_name = match spec.style {
            PageLabelStyle::Decimal => "D",
            PageLabelStyle::UpperRoman => "R",
            PageLabelStyle::LowerRoman => "r",
            PageLabelStyle::UpperAlpha => "A",
            PageLabelStyle::LowerAlpha => "a",
        };
        label_dict.insert(arena.name("S"), Object::Name(arena.name(style_name)));
        if let Some(prefix) = spec.prefix
            && !prefix.is_empty()
        {
            label_dict.insert(arena.name("P"), Object::String(Bytes::from(prefix)));
        }
        if spec.start_number != 1 {
            label_dict.insert(arena.name("St"), Object::Integer(i64::from(spec.start_number)));
        }
        let label_dh = arena.alloc_dict(label_dict);
        let label_h = arena.alloc_object(Object::Dictionary(label_dh));

        nums_items.push(Object::Integer(spec.start_page as i64));
        nums_items.push(Object::Reference(label_h));
    }

    let nums_arr_h = arena.alloc_array(nums_items);
    let mut num_tree_dict = BTreeMap::new();
    num_tree_dict.insert(arena.name("Nums"), Object::Array(nums_arr_h));
    let num_tree_dh = arena.alloc_dict(num_tree_dict);
    let num_tree_h = arena.alloc_object(Object::Dictionary(num_tree_dh));

    if let Some(cah) = doc.catalog_handle() {
        let cadh = doc.resolve_to_dict(cah)?;
        let mut cdict = arena.get_dict(cadh).unwrap_or_default();
        cdict.insert(arena.name("PageLabels"), Object::Reference(num_tree_h));
        arena.set_dict(cadh, cdict);
    }
    Ok(())
}
