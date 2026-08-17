use crate::operation::{ArticleThread, StructElemUpdate, UserProperty, UserPropertyValue};
use crate::struct_tree;
use bytes::Bytes;
use fepdf_model::arena::PdfArena;
use fepdf_model::{Document, Handle, Object, PdfResult};
use std::collections::BTreeMap;

/// Updates properties of a structure element.
pub fn apply_update_struct(doc: &Document, update: StructElemUpdate) -> PdfResult<()> {
    let handle = Handle::<Object>::new(update.handle_index);
    let arena = doc.arena();
    if let Some(Object::Dictionary(dh)) = arena.get_object(handle)
        && let Some(mut dict) = arena.get_dict(dh)
    {
        if let Some(tag) = update.new_tag {
            let s_key = arena.name("S");
            dict.insert(s_key, Object::Name(arena.name(&tag)));
        }
        if let Some(alt) = update.new_alt {
            let alt_key = arena.name("Alt");
            dict.insert(alt_key, Object::String(Bytes::from(alt)));
        }
        arena.set_dict(dh, dict);
    }
    Ok(())
}

/// Deletes a structure element by handle index from the StructTreeRoot.
pub fn apply_delete_struct(doc: &Document, handle_index: u32) -> PdfResult<()> {
    let handle = Handle::<Object>::new(handle_index);
    let arena = doc.arena();
    if let Some(cah) = doc.catalog_handle()
        && let Ok(cadh) = doc.resolve_to_dict(cah)
        && let Some(dict) = arena.get_dict(cadh)
        && let Some(str_root_obj) = dict.get(&arena.name("StructTreeRoot"))
        && let Some(str_root_ref) = struct_tree::resolve_to_node_handle(arena, str_root_obj)
    {
        struct_tree::delete_struct_node(arena, str_root_ref, handle);
    }
    Ok(())
}

fn create_article_thread_dict(
    arena: &PdfArena,
    thread: &ArticleThread,
    get_page_handle: impl Fn(usize) -> Option<Handle<Object>>,
) -> Handle<Object> {
    let mut info_dict = BTreeMap::new();
    info_dict.insert(arena.name("Title"), Object::String(Bytes::from(thread.title.clone())));
    let info_dh = arena.alloc_dict(info_dict);

    let mut thread_dict = BTreeMap::new();
    thread_dict.insert(arena.name("Type"), Object::Name(arena.name("Thread")));
    thread_dict.insert(arena.name("I"), Object::Dictionary(info_dh));
    let thread_dh = arena.alloc_dict(thread_dict);
    let thread_h = arena.alloc_object(Object::Dictionary(thread_dh));

    if !thread.beads.is_empty() {
        let mut bead_handles = Vec::new();
        for _ in &thread.beads {
            let bdh = arena.alloc_dict(BTreeMap::new());
            let bh = arena.alloc_object(Object::Dictionary(bdh));
            bead_handles.push((bdh, bh));
        }

        let n = bead_handles.len();
        for (i, (bead, &(bdh, _bh))) in thread.beads.iter().zip(bead_handles.iter()).enumerate() {
            let mut bdict = BTreeMap::new();
            bdict.insert(arena.name("Type"), Object::Name(arena.name("Bead")));
            bdict.insert(arena.name("T"), Object::Reference(thread_h));
            if let Some(page_h) = get_page_handle(bead.page) {
                bdict.insert(arena.name("P"), Object::Reference(page_h));
            }
            let rect_items = vec![
                Object::Real(f64::from(bead.rect[0])),
                Object::Real(f64::from(bead.rect[1])),
                Object::Real(f64::from(bead.rect[2])),
                Object::Real(f64::from(bead.rect[3])),
            ];
            let rect_ah = arena.alloc_array(rect_items);
            bdict.insert(arena.name("R"), Object::Array(rect_ah));
            bdict.insert(arena.name("N"), Object::Reference(bead_handles[(i + 1) % n].1));
            bdict.insert(arena.name("V"), Object::Reference(bead_handles[(i + n - 1) % n].1));
            arena.set_dict(bdh, bdict);
        }

        if let Some(mut td) = arena.get_dict(thread_dh) {
            td.insert(arena.name("F"), Object::Reference(bead_handles[0].1));
            arena.set_dict(thread_dh, td);
        }
    }

    thread_h
}

/// Updates article threads in the catalogue (Clause 12.4.3).
pub fn apply_update_article_threads(doc: &Document, threads: Vec<ArticleThread>) -> PdfResult<()> {
    let arena = doc.arena();
    let mut thread_refs = Vec::new();
    for thread in &threads {
        let th = create_article_thread_dict(arena, thread, |idx| doc.get_page_handle(idx));
        thread_refs.push(Object::Reference(th));
    }

    if let Some(cah) = doc.catalog_handle() {
        let cadh = doc.resolve_to_dict(cah)?;
        let mut cdict = arena.get_dict(cadh).unwrap_or_default();
        let threads_ah = arena.alloc_array(thread_refs);
        cdict.insert(arena.name("Threads"), Object::Array(threads_ah));
        arena.set_dict(cadh, cdict);
    }
    Ok(())
}

/// Adds user properties to a structure element attribute dictionary (Clause 14.7.5.3).
pub fn apply_add_user_properties(
    doc: &Document,
    target_handle: u32,
    properties: Vec<UserProperty>,
) -> PdfResult<()> {
    let arena = doc.arena();
    let mut prop_items = Vec::new();

    for prop in properties {
        let mut pdict = BTreeMap::new();
        pdict.insert(arena.name("N"), Object::String(Bytes::from(prop.name)));
        let val_obj = match prop.value {
            UserPropertyValue::Text(s) => Object::String(Bytes::from(s)),
            UserPropertyValue::Number(n) => Object::Real(n),
            UserPropertyValue::Boolean(b) => Object::Boolean(b),
        };
        pdict.insert(arena.name("V"), val_obj);
        if let Some(f) = prop.formatted {
            pdict.insert(arena.name("F"), Object::String(Bytes::from(f)));
        }
        let pdh = arena.alloc_dict(pdict);
        let ph = arena.alloc_object(Object::Dictionary(pdh));
        prop_items.push(Object::Reference(ph));
    }

    let p_arr_h = arena.alloc_array(prop_items);
    let mut attr_dict = BTreeMap::new();
    attr_dict.insert(arena.name("O"), Object::Name(arena.name("UserProperties")));
    attr_dict.insert(arena.name("P"), Object::Array(p_arr_h));
    let attr_dh = arena.alloc_dict(attr_dict);
    let attr_h = arena.alloc_object(Object::Dictionary(attr_dh));

    let target_h = Handle::new(target_handle);
    if let Some(Object::Dictionary(dh)) = arena.get_object(target_h)
        && let Some(mut dict) = arena.get_dict(dh)
    {
        let a_key = arena.name("A");
        let mut a_items = if let Some(existing_a) = dict.get(&a_key) {
            match existing_a {
                Object::Array(ah) => arena.get_array(*ah).unwrap_or_default(),
                Object::Reference(h) => vec![Object::Reference(*h)],
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        a_items.push(Object::Reference(attr_h));
        let new_a_ah = arena.alloc_array(a_items);
        dict.insert(a_key, Object::Array(new_a_ah));
        arena.set_dict(dh, dict);
    }
    Ok(())
}
