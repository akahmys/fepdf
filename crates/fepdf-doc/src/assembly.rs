//! New documents built from existing ones: merged, extracted, or copied for a writer.
//!
//! **Each of these creates an arena**, and this is where that happens: the facade holds a
//! document and does not make the store one lives in (CODING.md Rule A, which
//! `scripts/audit/layering.py` checks). The three sat in `fepdf/src/lib.rs` until
//! ROADMAP Y-5, and merging and extracting each carried its own copy of the page tree
//! they build.

use crate::cloning::ObjectCloner;
use fepdf_model::{DictHandle, Document, Handle, Object, PdfArena, PdfError, PdfResult};
use std::collections::BTreeMap;

/// A page tree being filled: its root, allocated first so each page can name it as
/// `/Parent`, and the pages it will hold.
struct PageTree {
    root: Handle<Object>,
    root_dict: DictHandle,
    kids: Vec<Object>,
}

impl PageTree {
    fn new(target: &PdfArena) -> Self {
        let root_dict = target.alloc_dict(BTreeMap::new());
        let root = target.alloc_object(Object::Dictionary(root_dict));
        Self { root, root_dict, kids: Vec::new() }
    }

    /// Clones `source`'s pages at `indices` into the tree, in that order.
    ///
    /// **By handle**, so a page is one object however it is reached. A page was cloned as
    /// a dictionary and given a new object, while a link or a bookmark naming it was
    /// cloned through the cloner's map into a second copy no tree held — and once
    /// [`crate::page_removal::forget_absent_pages`] ran, a link between two pages both
    /// kept was deleted as naming a page left out (ROADMAP Y-F21).
    fn clone_pages(
        &mut self,
        source: &Document,
        indices: impl IntoIterator<Item = usize>,
        cloner: &mut ObjectCloner,
    ) -> PdfResult<()> {
        let target = cloner.target();
        let parent_key = target.name("Parent");
        for index in indices {
            let page = cloner.clone_handle(source.get_page(index)?.obj_handle())?;
            let dh = target
                .get_object(page)
                .and_then(|o| o.as_dict_handle())
                .ok_or_else(|| PdfError::internal(format!("page {index} is not a dictionary")))?;
            let mut cloned = target.get_dict(dh).unwrap_or_default();
            cloned.insert(parent_key, Object::Reference(self.root));
            target.set_dict(dh, cloned);
            self.kids.push(Object::Reference(page));
        }
        Ok(())
    }

    /// Writes the tree's root and a catalogue naming it, with `entries` besides, and
    /// returns the document they make.
    fn finish(self, target: PdfArena, entries: Vec<(&str, Object)>) -> Document {
        let type_key = target.name("Type");
        let pages_key = target.name("Pages");
        let mut pages = BTreeMap::new();
        pages.insert(type_key, Object::Name(pages_key));
        #[allow(clippy::cast_possible_wrap)] // a page count is nowhere near i64::MAX
        pages.insert(target.name("Count"), Object::Integer(self.kids.len() as i64));
        pages.insert(target.name("Kids"), Object::Array(target.alloc_array(self.kids)));
        target.set_dict(self.root_dict, pages);

        let mut catalog = BTreeMap::new();
        catalog.insert(type_key, Object::Name(target.name("Catalog")));
        catalog.insert(pages_key, Object::Reference(self.root));
        for (key, value) in entries {
            catalog.insert(target.name(key), value);
        }
        let catalog = target.alloc_object(Object::Dictionary(target.alloc_dict(catalog)));
        let mut document = Document::new(target, catalog, None);
        // Without this the document reports no pages at all: the page index is
        // `Document::new`'s empty vector until something walks the tree.
        document.index_pages();
        document
    }
}

/// `sources`, one after another, in a document of their own: every page, every field of
/// each `/AcroForm`, and each source's outline under an item of its own.
///
/// # Errors
///
/// Refuses an empty list, and fails where a page cannot be read or cloned.
pub fn merge(sources: &[&Document]) -> PdfResult<Document> {
    if sources.is_empty() {
        return Err(PdfError::refused("merge", "No sources to merge"));
    }
    let target = PdfArena::new();
    let mut tree = PageTree::new(&target);
    let (mut fields, mut outlines) = (Vec::new(), Vec::new());
    for (index, source) in sources.iter().enumerate() {
        let mut cloner = ObjectCloner::new(source.arena(), &target);
        tree.clone_pages(source, 0..source.page_count()?, &mut cloner)?;
        fields.extend(cloned_fields(source, &mut cloner));
        outlines.extend(cloned_outline(source, index + 1, &mut cloner));
    }
    let mut entries = Vec::new();
    if !fields.is_empty() {
        let mut form = BTreeMap::new();
        form.insert(target.name("Fields"), Object::Array(target.alloc_array(fields)));
        entries.push(("AcroForm", Object::Dictionary(target.alloc_dict(form))));
    }
    if !outlines.is_empty() {
        entries.push(("Outlines", linked_outlines(&target, &outlines)));
    }
    Ok(tree.finish(target, entries))
}

/// The catalogue entry `key` of `source`, resolved to a dictionary.
fn catalog_entry(
    source: &Document,
    key: &str,
) -> Option<BTreeMap<Handle<fepdf_model::PdfName>, Object>> {
    let arena = source.arena();
    let catalog = Object::Reference(source.catalog_handle()?);
    fepdf_model::access::dict_of(arena, &fepdf_model::access::entry(arena, &catalog, key)?)
}

/// `source`'s form fields, cloned; a field that will not clone is left out.
fn cloned_fields(source: &Document, cloner: &mut ObjectCloner) -> Vec<Object> {
    let arena = source.arena();
    let Some(form) = catalog_entry(source, "AcroForm") else { return Vec::new() };
    let Some(fields) = form.get(&arena.name("Fields")).and_then(|f| f.resolve(arena).as_array())
    else {
        return Vec::new();
    };
    arena
        .get_array(fields)
        .unwrap_or_default()
        .iter()
        .filter_map(|field| cloner.clone_complete(field).ok())
        .collect()
}

/// An outline item titled "Source `index`" holding `source`'s outline, when it has one.
fn cloned_outline(
    source: &Document,
    index: usize,
    cloner: &mut ObjectCloner,
) -> Option<Handle<Object>> {
    let outlines = catalog_entry(source, "Outlines")?;
    let first = cloner.clone_complete(outlines.get(&source.arena().name("First"))?).ok()?;
    let target = cloner.target();
    let mut item = BTreeMap::new();
    item.insert(target.name("Title"), Object::String(format!("Source {index}").into()));
    item.insert(target.name("First"), first);
    Some(target.alloc_object(Object::Dictionary(target.alloc_dict(item))))
}

/// The outline root over `items`, each linked to its neighbours by `/Prev` and `/Next`.
///
/// **An object of its own**, because Table 29 says the catalogue's `/Outlines` shall be an
/// indirect reference. It was written direct, and the outline reader reads a direct one
/// as no outline at all, so every merged document had none (ROADMAP Y-F21).
fn linked_outlines(target: &PdfArena, items: &[Handle<Object>]) -> Object {
    for (i, &item) in items.iter().enumerate() {
        let Some(dh) = target.get_object(item).and_then(|o| o.as_dict_handle()) else { continue };
        let mut dict = target.get_dict(dh).unwrap_or_default();
        if let Some(&prev) = i.checked_sub(1).and_then(|p| items.get(p)) {
            dict.insert(target.name("Prev"), Object::Reference(prev));
        }
        if let Some(&next) = items.get(i + 1) {
            dict.insert(target.name("Next"), Object::Reference(next));
        }
        target.set_dict(dh, dict);
    }
    let mut root = BTreeMap::new();
    root.insert(target.name("Type"), Object::Name(target.name("Outlines")));
    if let (Some(&first), Some(&last)) = (items.first(), items.last()) {
        root.insert(target.name("First"), Object::Reference(first));
        root.insert(target.name("Last"), Object::Reference(last));
    }
    #[allow(clippy::cast_possible_wrap)] // one item a source
    root.insert(target.name("Count"), Object::Integer(items.len() as i64));
    Object::Reference(target.alloc_object(Object::Dictionary(target.alloc_dict(root))))
}

/// The pages of `source` at `indices`, in that order, in a document of their own.
///
/// # Errors
///
/// Refuses an empty list, and fails where a page cannot be read or cloned.
pub fn extract_pages(source: &Document, indices: &[usize]) -> PdfResult<Document> {
    if indices.is_empty() {
        return Err(PdfError::refused("extract_pages", "No indices to extract"));
    }
    let target = PdfArena::new();
    let mut tree = PageTree::new(&target);
    let mut cloner = ObjectCloner::new(source.arena(), &target);
    tree.clone_pages(source, indices.iter().copied(), &mut cloner)?;
    let document = tree.finish(target, Vec::new());
    // Cloning a page clones what it names, and a link names the page it goes to: a
    // page left out came across that way, and would be written (ROADMAP Y-F18).
    crate::page_removal::forget_absent_pages(&document)?;
    Ok(document)
}

/// `source` copied into an arena of its own, for a writer to consume.
///
/// **The one place two arenas are live for a save**, and the reason it is a function
/// rather than four lines written twice. The copy's handles index its own arena;
/// `source`'s index a different one, and a `Handle<Object>` does not say which — the
/// pools are separated by handle *type*, not by arena (ROADMAP W-A3). Two copies of this
/// sat four lines above a `writer.finish(root, info)` where the source's root handle
/// would have compiled and written a different object. Returning a `Document` keeps the
/// two from sharing a scope at all.
///
/// # Errors
///
/// Fails where the root or the information dictionary cannot be cloned.
pub fn copied(source: &Document) -> PdfResult<Document> {
    let target = PdfArena::new();
    let mut cloner = ObjectCloner::new(source.arena(), &target);
    let root = cloner.clone_handle(*source.root_handle())?;
    let info = source.info_handle().map(|h| cloner.clone_handle(h)).transpose()?;
    drop(cloner);
    Ok(Document::new(target, root, info))
}
