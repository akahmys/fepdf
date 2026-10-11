//! Each paragraph a `/P` element (14.8.4.3.2), its content found from its pages through
//! the parent tree (14.7.5.4).
//!
//! **Into the tree the document has, or one made for it.** A tree that holds one
//! `/Document` element takes the paragraphs into it; one that does not takes them at its
//! root. A document with no tree is given one, a `/Document` element holding them, and
//! `/MarkInfo` saying it is tagged — and, where the text's language is given and the
//! document states none, `/Lang`.
//!
//! A paragraph that runs onto the next page is one element whose content is on both,
//! each part by a marked-content reference naming its page.

use fepdf_model::{Document, Handle, Object, PdfArena, PdfName, PdfResult};
use std::collections::BTreeMap;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// A paragraph's parts: the page each is on and its `/MCID` there.
pub(super) struct Tagged {
    pub(super) parts: Vec<(Handle<Object>, usize)>,
}

/// Tags `paragraphs`, whose parts are on `pages`, in `doc`'s structure tree.
///
/// # Errors
/// When the catalogue does not resolve.
pub(super) fn tag(
    doc: &Document,
    paragraphs: &[Tagged],
    pages: &[Handle<Object>],
    lang: Option<&str>,
) -> PdfResult<()> {
    let arena = doc.arena();
    let root = root_of(doc, lang)?;
    let parent = document_element(arena, root).unwrap_or(root);
    let mut elements = Vec::with_capacity(paragraphs.len());
    let mut by_page: BTreeMap<Handle<Object>, BTreeMap<usize, Handle<Object>>> = BTreeMap::new();
    for paragraph in paragraphs {
        let element = paragraph_element(arena, parent, paragraph, lang);
        for (page, mcid) in &paragraph.parts {
            by_page.entry(*page).or_default().insert(*mcid, element);
        }
        elements.push(Object::Reference(element));
    }
    append_kids(arena, parent, elements);
    let mut key = next_key(arena, root);
    let mut entries = Vec::new();
    for page in pages {
        let Some(marks) = by_page.get(page) else { continue };
        let array = marks.values().map(|e| Object::Reference(*e)).collect();
        entries.push((key, Object::Array(arena.alloc_array(array))));
        set(arena, *page, |dict| {
            dict.insert(arena.name("StructParents"), Object::Integer(key));
            dict.insert(arena.name("Tabs"), Object::Name(arena.name("S")));
        });
        key += 1;
    }
    add_entries(arena, root, entries);
    set(arena, root, |dict| {
        dict.insert(arena.name("ParentTreeNextKey"), Object::Integer(key));
    });
    Ok(())
}

/// The structure tree root, made where there is none.
fn root_of(doc: &Document, lang: Option<&str>) -> PdfResult<Handle<Object>> {
    let arena = doc.arena();
    let catalog_handle = doc.catalog_handle().ok_or_else(|| {
        fepdf_model::PdfError::refused("InsertText", "the document has no catalogue".to_owned())
    })?;
    let catalog = doc.resolve_to_dict(catalog_handle)?;
    match arena.dict_entry(catalog, arena.name("StructTreeRoot")) {
        Some(Object::Reference(root)) => return Ok(root),
        // Held in the catalogue rather than as an object: made one, so elements can name it.
        Some(direct @ Object::Dictionary(_)) => {
            let root = arena.alloc_object(direct);
            let mut entries = arena.get_dict(catalog).unwrap_or_default();
            entries.insert(arena.name("StructTreeRoot"), Object::Reference(root));
            arena.set_dict(catalog, entries);
            return Ok(root);
        }
        _ => {}
    }
    let mut root = Dict::new();
    root.insert(arena.name("Type"), Object::Name(arena.name("StructTreeRoot")));
    let root = arena.alloc_object(Object::Dictionary(arena.alloc_dict(root)));
    let mut element = Dict::new();
    element.insert(arena.name("Type"), Object::Name(arena.name("StructElem")));
    element.insert(arena.name("S"), Object::Name(arena.name("Document")));
    element.insert(arena.name("P"), Object::Reference(root));
    element.insert(arena.name("K"), Object::Array(arena.alloc_array(Vec::new())));
    let element = arena.alloc_object(Object::Dictionary(arena.alloc_dict(element)));
    set(arena, root, |dict| {
        dict.insert(arena.name("K"), Object::Reference(element));
    });
    let mut entries = arena.get_dict(catalog).unwrap_or_default();
    entries.insert(arena.name("StructTreeRoot"), Object::Reference(root));
    let mut mark_info = Dict::new();
    mark_info.insert(arena.name("Marked"), Object::Boolean(true));
    entries.insert(arena.name("MarkInfo"), Object::Dictionary(arena.alloc_dict(mark_info)));
    if let Some(lang) = lang {
        entries.entry(arena.name("Lang")).or_insert_with(|| Object::Text(lang.to_owned()));
    }
    // PDF/UA-2's 07-001: a tagged document shows its title, not its file's name, where
    // a title is shown. Asked for where there were no preferences, and changed nowhere.
    entries.entry(arena.name("ViewerPreferences")).or_insert_with(|| {
        let mut preferences = Dict::new();
        preferences.insert(arena.name("DisplayDocTitle"), Object::Boolean(true));
        Object::Dictionary(arena.alloc_dict(preferences))
    });
    arena.set_dict(catalog, entries);
    Ok(root)
}

/// The root's one `/Document` element, where it has exactly that.
fn document_element(arena: &PdfArena, root: Handle<Object>) -> Option<Handle<Object>> {
    let root_dict = Object::Reference(root).resolve(arena).as_dict_handle()?;
    let kid = match arena.dict_entry(root_dict, arena.name("K"))? {
        Object::Reference(kid) => kid,
        Object::Array(kids) => match arena.get_array(kids)?.as_slice() {
            [Object::Reference(kid)] => *kid,
            _ => return None,
        },
        _ => return None,
    };
    let dict = Object::Reference(kid).resolve(arena).as_dict_handle()?;
    let tag = arena.dict_entry(dict, arena.name("S"))?.as_name()?;
    (arena.get_name_str(tag).as_deref() == Some("Document")).then_some(kid)
}

/// A `/P` element under `parent`, its content the marked-content references of its parts.
fn paragraph_element(
    arena: &PdfArena,
    parent: Handle<Object>,
    paragraph: &Tagged,
    lang: Option<&str>,
) -> Handle<Object> {
    let kids = paragraph
        .parts
        .iter()
        .map(|(page, mcid)| {
            let mut mcr = Dict::new();
            mcr.insert(arena.name("Type"), Object::Name(arena.name("MCR")));
            mcr.insert(arena.name("Pg"), Object::Reference(*page));
            mcr.insert(arena.name("MCID"), Object::Integer(i64::try_from(*mcid).unwrap_or(0)));
            Object::Dictionary(arena.alloc_dict(mcr))
        })
        .collect();
    let mut element = Dict::new();
    element.insert(arena.name("Type"), Object::Name(arena.name("StructElem")));
    element.insert(arena.name("S"), Object::Name(arena.name("P")));
    element.insert(arena.name("P"), Object::Reference(parent));
    if let Some((first, _)) = paragraph.parts.first() {
        element.insert(arena.name("Pg"), Object::Reference(*first));
    }
    element.insert(arena.name("K"), Object::Array(arena.alloc_array(kids)));
    if let Some(lang) = lang {
        element.insert(arena.name("Lang"), Object::Text(lang.to_owned()));
    }
    arena.alloc_object(Object::Dictionary(arena.alloc_dict(element)))
}

/// `kids` after whatever `parent`'s `/K` holds, which becomes an array if it was one kid.
fn append_kids(arena: &PdfArena, parent: Handle<Object>, kids: Vec<Object>) {
    set(arena, parent, |dict| {
        let key = arena.name("K");
        let mut all = match dict.get(&key) {
            Some(Object::Array(held)) => arena.get_array(*held).unwrap_or_default(),
            Some(one) => vec![one.clone()],
            None => Vec::new(),
        };
        all.extend(kids);
        dict.insert(key, Object::Array(arena.alloc_array(all)));
    });
}

/// The first key the parent tree has not used: `/ParentTreeNextKey`, or one past the
/// largest key in it.
fn next_key(arena: &PdfArena, root: Handle<Object>) -> i64 {
    let stated = Object::Reference(root)
        .resolve(arena)
        .as_dict_handle()
        .and_then(|d| arena.dict_entry(d, arena.name("ParentTreeNextKey")))
        .and_then(|k| k.as_integer());
    let used = crate::parent_tree::array_entries(arena, root)
        .keys()
        .chain(crate::parent_tree::single_entries(arena, root).keys())
        .max()
        .map_or(0, |k| k + 1);
    stated.unwrap_or(0).max(used)
}

/// `entries` into the parent tree: onto its `/Nums`, or as a leaf after its `/Kids`, whose
/// keys they all follow.
fn add_entries(arena: &PdfArena, root: Handle<Object>, entries: Vec<(i64, Object)>) {
    let (Some((first, _)), Some((last, _))) = (entries.first(), entries.last()) else { return };
    let limits = vec![Object::Integer(*first), Object::Integer(*last)];
    let nums: Vec<Object> =
        entries.into_iter().flat_map(|(k, v)| [Object::Integer(k), v]).collect();
    let tree = Object::Reference(root)
        .resolve(arena)
        .as_dict_handle()
        .and_then(|d| arena.dict_entry(d, arena.name("ParentTree")))
        .and_then(|t| t.resolve(arena).as_dict_handle());
    let Some(tree) = tree else {
        let mut fresh = Dict::new();
        fresh.insert(arena.name("Nums"), Object::Array(arena.alloc_array(nums)));
        set(arena, root, |dict| {
            dict.insert(arena.name("ParentTree"), Object::Dictionary(arena.alloc_dict(fresh)));
        });
        return;
    };
    let mut dict = arena.get_dict(tree).unwrap_or_default();
    let kids_key = arena.name("Kids");
    if let Some(Object::Array(kids)) = dict.get(&kids_key).map(|k| k.resolve(arena)) {
        let mut leaf = Dict::new();
        leaf.insert(arena.name("Limits"), Object::Array(arena.alloc_array(limits)));
        leaf.insert(arena.name("Nums"), Object::Array(arena.alloc_array(nums)));
        let mut all = arena.get_array(kids).unwrap_or_default();
        all.push(Object::Reference(arena.alloc_object(Object::Dictionary(arena.alloc_dict(leaf)))));
        dict.insert(kids_key, Object::Array(arena.alloc_array(all)));
    } else {
        let nums_key = arena.name("Nums");
        let mut all = match dict.get(&nums_key).map(|n| n.resolve(arena)) {
            Some(Object::Array(held)) => arena.get_array(held).unwrap_or_default(),
            _ => Vec::new(),
        };
        all.extend(nums);
        dict.insert(nums_key, Object::Array(arena.alloc_array(all)));
    }
    arena.set_dict(tree, dict);
}

/// Changes the dictionary `object` is.
fn set(arena: &PdfArena, object: Handle<Object>, change: impl FnOnce(&mut Dict)) {
    let Some(handle) = Object::Reference(object).resolve(arena).as_dict_handle() else { return };
    let mut dict = arena.get_dict(handle).unwrap_or_default();
    change(&mut dict);
    arena.set_dict(handle, dict);
}
