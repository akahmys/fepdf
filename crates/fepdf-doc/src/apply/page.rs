#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
use crate::operation::{PageLabelSpec, PageLabelStyle, PageSelection, PdfStandard, RotateMode};
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

/// Resolves a selection against the current page count.
fn indices_of(pages: &PageSelection, count: usize) -> Vec<usize> {
    match pages {
        PageSelection::All => (0..count).collect(),
        PageSelection::Single(i) => vec![*i],
        PageSelection::Indices(idx) => idx.clone(),
    }
}

/// Moves several pages to one position, as a single movement.
pub fn apply_reorder_batch(
    doc: &mut Document,
    sources: &[usize],
    target: usize,
) -> PdfResult<std::ops::Range<usize>> {
    doc.reorder_pages_batch(sources, target)
}

/// Duplicates pages, each clone placed immediately after its original.
///
/// Moved out of the facade, where it had been reachable as `PdfDocument::duplicate_page`
/// beside the vocabulary rather than through it (ARCHITECTURE §4.1, Rule D). Nothing about
/// it belonged above `fepdf-doc`: the cloner it needs lives here.
pub fn apply_duplicate_pages(doc: &mut Document, pages: &PageSelection) -> PdfResult<()> {
    let count = doc.page_count()?;
    let mut indices = indices_of(pages, count);
    indices.sort_unstable();
    indices.dedup();
    if let Some(&worst) = indices.last()
        && worst >= count
    {
        return Err(fepdf_model::PdfError::Arena(
            format!("Page index {worst} out of bounds").into(),
        ));
    }

    // Descending, so each insertion leaves the indices still to be handled where they
    // were. Ascending does not merely mis-order: measured on three pages selected
    // together, it clones page 0 three times, because after the first insertion indices 1
    // and 2 name the clones rather than the originals the caller chose. That arithmetic
    // is the reason page selections are resolved here and not in a frontend loop —
    // `fepdf-gui` was running one of its own until Rule D was enforced.
    for idx in indices.into_iter().rev() {
        let source_page = doc.get_page(idx)?;
        let source_dh = doc.resolve_to_dict(source_page.obj_handle())?;
        let arena = doc.arena();
        let cloned = {
            let mut cloner = crate::cloning::ObjectCloner::new(arena, arena);
            cloner.clone_complete(&Object::Dictionary(source_dh))?
        };
        if let Object::Dictionary(dh) = cloned {
            let handle = doc.arena().alloc_object(Object::Dictionary(dh));
            doc.pages.insert(idx + 1, handle);
        }
    }
    doc.rebuild_page_tree_in_arena()
}

/// Inserts every page of another document, given as that document's bytes.
///
/// Returns the number of pages inserted. Opening the source here rather than taking an
/// already-open document is what lets this be an `Operation` at all — a value that
/// serialises, which `fepdf-mcp` needs and the GUI already had in hand.
pub fn apply_insert_from(doc: &mut Document, source: &[u8], at: usize) -> PdfResult<usize> {
    let options = fepdf_model::ingest::IngestionOptions::default();
    let source_doc = Document::open(Bytes::copy_from_slice(source), &options)?;
    let source_count = source_doc.page_count()?;
    if source_count == 0 {
        return Ok(0);
    }

    let clamped = at.min(doc.pages.len());
    let mut cloner = crate::cloning::ObjectCloner::new(source_doc.arena(), doc.arena());
    let mut handles = Vec::with_capacity(source_count);
    for i in 0..source_count {
        let page = source_doc.get_page(i)?;
        let dh = source_doc.resolve_to_dict(page.obj_handle())?;
        if let Object::Dictionary(cloned) = cloner.clone_complete(&Object::Dictionary(dh))? {
            handles.push(doc.arena().alloc_object(Object::Dictionary(cloned)));
        }
    }

    let inserted = handles.len();
    for (i, h) in handles.into_iter().enumerate() {
        doc.pages.insert(clamped + i, h);
    }
    doc.rebuild_page_tree_in_arena()?;
    Ok(inserted)
}

/// Declares a target standard in the catalogue, and sets the version to 2.0.
///
/// Every branch writes 2.0 because output is always 2.0 (ROADMAP, "the subsets this
/// processor has chosen"); what differs is the key each standard reads.
pub fn apply_upgrade(doc: &mut Document, standard: PdfStandard) -> PdfResult<()> {
    use fepdf_model::PdfName;
    let arena = doc.arena();
    arena.set_version(2.0);

    let (key, value) = match standard {
        PdfStandard::ISO32000_2 => return Ok(()),
        PdfStandard::A4 => ("GTS_PDFA14", Object::Name(arena.intern_name(PdfName::new("Yes")))),
        PdfStandard::UA2 => ("PdfUA", Object::Integer(2)),
        PdfStandard::X6 => ("GTS_PDFX", Object::Name(arena.intern_name(PdfName::new("PDFX6")))),
    };

    if let Some(catalog_handle) = doc.catalog_handle()
        && let Ok(dh) = doc.resolve_to_dict(catalog_handle)
    {
        let mut catalog = arena.get_dict(dh).unwrap_or_default();
        catalog.insert(arena.intern_name(PdfName::new(key)), value);
        arena.set_dict(dh, catalog);
    }
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
