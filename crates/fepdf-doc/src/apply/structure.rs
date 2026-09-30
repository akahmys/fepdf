use crate::operation::{
    ArticleThread, AssociatedFile, AttributeValue, StructAttribute, StructElemMove,
    StructElemUpdate, UserProperty, UserPropertyValue,
};
use crate::struct_tree;
use fepdf_model::arena::PdfArena;
use fepdf_model::{Document, Handle, Object, PdfError, PdfResult};
use std::collections::BTreeMap;

/// Updates properties of a structure element.
pub fn apply_update_struct(doc: &Document, update: StructElemUpdate) -> PdfResult<()> {
    let handle = Handle::<Object>::new(update.handle_index);
    let arena = doc.arena();
    // **An element that is not there is an error, not an update of nothing.** This answered
    // `Ok(())` for any handle, so a caller naming the wrong element was told it had
    // changed it.
    let Some((dh, mut dict)) = arena
        .get_object(handle)
        .and_then(|o| o.as_dict_handle())
        .and_then(|dh| Some((dh, arena.get_dict(dh)?)))
    else {
        return Err(PdfError::Other(
            format!("object {} is not a structure element", update.handle_index).into(),
        ));
    };
    if let Some(tag) = update.new_tag {
        dict.insert(arena.name("S"), Object::Name(arena.name(&tag)));
    }
    // Text strings (14.9.3 to 14.9.5): each is read aloud or in place of the content, so
    // each is one a non-Latin document is certain to need.
    for (key, value) in [
        ("Alt", update.new_alt),
        ("Lang", update.new_lang),
        ("ActualText", update.new_actual_text),
        ("E", update.new_expansion),
    ] {
        if let Some(value) = value {
            dict.insert(arena.name(key), Object::Text(value));
        }
    }
    arena.set_dict(dh, dict);
    Ok(())
}

/// Deletes a structure element, and everything under it, from the tree (14.7).
///
/// **Out of the tree is out of the file.** The element was taken out of its parent's `/K`
/// alone, so the parent tree went on saying its marks were its and the `/IDTree` went on
/// naming it: the writer wrote it, `/Alt` and all, and the tree said content belonged to
/// an element it did not hold (14.7.5.4). Its marks now belong to nothing, which is what
/// deleting a tag and keeping what it tagged means, and the `/IDTree` forgets it.
///
/// # Errors
/// Fails when the document has no structure tree or the tree does not hold the element —
/// deleting nothing is not an answer a caller naming the wrong element should be given.
pub fn apply_delete_struct(doc: &Document, handle_index: u32) -> PdfResult<()> {
    let handle = Handle::<Object>::new(handle_index);
    let arena = doc.arena();
    let Some(root) = doc.get_structure_root()? else {
        return Err(PdfError::Other("the document has no structure tree".into()));
    };
    let gone = struct_tree::subtree(arena, handle);
    if !struct_tree::delete_struct_node(arena, root, handle) {
        return Err(PdfError::Other(
            format!("object {handle_index} is not an element of the structure tree").into(),
        ));
    }
    crate::parent_tree::forget_elements(arena, root, &gone);
    let ids = arena
        .get_object(root)
        .and_then(|o| o.as_dict_handle())
        .and_then(|d| arena.dict_entry(d, arena.name("IDTree")));
    if let Some(ids) = ids {
        crate::page_removal::retain_in_name_tree(arena, ids, &|value| {
            !value.as_reference().is_some_and(|element| gone.contains(&element))
        });
    }
    Ok(())
}

/// Moves a structure element beside or inside another.
///
/// **A move that cannot be made is an error rather than a silent no-op.** The three
/// refusals — a cycle, a target outside the tree, an element the tree does not hold —
/// are all a caller asking for something the file cannot express, and reporting `Ok(())`
/// for them is how the window came to have a drag that rearranged nothing.
pub fn apply_move_struct(doc: &Document, move_: StructElemMove) -> PdfResult<()> {
    let StructElemMove { handle_index, target_index, placement } = move_;
    let arena = doc.arena();
    let Some(root) = doc
        .catalog_handle()
        .and_then(|cah| doc.resolve_to_dict(cah).ok())
        .and_then(|cadh| arena.get_dict(cadh))
        .and_then(|dict| dict.get(&arena.name("StructTreeRoot")).cloned())
        .and_then(|entry| struct_tree::resolve_to_node_handle(arena, &entry))
    else {
        return Err(fepdf_model::PdfError::Other(
            "the document has no /StructTreeRoot to move an element within".into(),
        ));
    };
    let moved = struct_tree::move_struct_node(
        arena,
        root,
        Handle::<Object>::new(handle_index),
        Handle::<Object>::new(target_index),
        placement,
    );
    if moved {
        Ok(())
    } else {
        Err(fepdf_model::PdfError::Other(
            format!(
                "element {handle_index} cannot move to {target_index}: \
             one of them is not in the structure tree, or the move would make a cycle"
            )
            .into(),
        ))
    }
}

fn create_article_thread_dict(
    arena: &PdfArena,
    thread: &ArticleThread,
    get_page_handle: impl Fn(usize) -> Option<Handle<Object>>,
) -> Handle<Object> {
    let mut info_dict = BTreeMap::new();
    // `/I` is a thread information dictionary (12.4.3), which takes the document
    // information dictionary's entries — and `/Title` there is a text string (Table 349).
    info_dict.insert(arena.name("Title"), Object::Text(thread.title.clone()));
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
///
/// # Errors
/// Fails, before anything is written, when a thread has no beads — Table 160 requires the
/// `/F` a thread starts from — or a bead is on a page the document does not have, which
/// was written with no `/P` though Table 162 requires one.
pub fn apply_update_article_threads(doc: &Document, threads: Vec<ArticleThread>) -> PdfResult<()> {
    let arena = doc.arena();
    let count = doc.page_count()?;
    for thread in &threads {
        if thread.beads.is_empty() {
            return Err(PdfError::Other(
                format!("the thread {:?} has no beads to start from", thread.title).into(),
            ));
        }
        if let Some(bead) = thread.beads.iter().find(|bead| bead.page >= count) {
            return Err(PdfError::Other(
                format!("this document has {count} pages and no page {}", bead.page).into(),
            ));
        }
    }
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
        // `/N` and `/F` are text strings (Table 380), and a `/V` that is a string is the
        // value those two describe — all three are shown to a reader.
        pdict.insert(arena.name("N"), Object::Text(prop.name));
        let val_obj = match prop.value {
            UserPropertyValue::Text(s) => Object::Text(s),
            UserPropertyValue::Number(n) => Object::Real(n),
            UserPropertyValue::Boolean(b) => Object::Boolean(b),
        };
        pdict.insert(arena.name("V"), val_obj);
        if let Some(f) = prop.formatted {
            pdict.insert(arena.name("F"), Object::Text(f));
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

    append_attribute(arena, target_handle, Object::Reference(attr_h))
}

/// The attribute objects an element's `/A` holds (14.7.6): one, direct or by reference, or
/// an array of them among revision numbers — which are kept, and left where they were.
fn attributes(arena: &PdfArena, a: Option<&Object>) -> Vec<Object> {
    match a {
        Some(Object::Array(array)) => arena.get_array(*array).unwrap_or_default(),
        Some(single @ (Object::Reference(_) | Object::Dictionary(_))) => vec![single.clone()],
        Some(_) | None => Vec::new(),
    }
}

/// Adds `attribute` to the element `element`'s `/A`, keeping what is there.
///
/// **A direct `/A` dictionary is kept.** The match that preceded this read an array or a
/// reference and made anything else an empty list, so an element whose `/A` was written
/// in place — `/A << /O /Layout /Placement /Block >>` — lost it to the first user property
/// added.
fn append_attribute(arena: &PdfArena, element: u32, attribute: Object) -> PdfResult<()> {
    let Some((dh, mut dict)) = element_dict(arena, element) else {
        return Err(not_an_element(element));
    };
    let a_key = arena.name("A");
    let mut items = attributes(arena, dict.get(&a_key));
    items.push(attribute);
    dict.insert(a_key, Object::Array(arena.alloc_array(items)));
    arena.set_dict(dh, dict);
    Ok(())
}

/// An element's dictionary, by object handle index.
pub(super) fn element_dict(
    arena: &PdfArena,
    element: u32,
) -> Option<(
    Handle<BTreeMap<Handle<fepdf_model::PdfName>, Object>>,
    BTreeMap<Handle<fepdf_model::PdfName>, Object>,
)> {
    let dh = arena.get_object(Handle::<Object>::new(element))?.as_dict_handle()?;
    Some((dh, arena.get_dict(dh)?))
}

/// The refusal for an index naming no element.
pub(super) fn not_an_element(element: u32) -> PdfError {
    PdfError::Other(format!("object {element} is not a structure element").into())
}

/// Sets one attribute of a structure element (14.7.6): the key in the attribute object its
/// `/A` holds for the owner, or in one added for it.
///
/// **One object per owner, and the first one found.** An element may carry several
/// attribute objects, one per owner; a second object for an owner that has one would say
/// the same thing twice, and a reader takes the first.
pub fn apply_set_struct_attribute(doc: &Document, attribute: StructAttribute) -> PdfResult<()> {
    let arena = doc.arena();
    let Some((_, dict)) = element_dict(arena, attribute.handle_index) else {
        return Err(not_an_element(attribute.handle_index));
    };
    let value = attribute_value(arena, attribute.value);
    let owned = attributes(arena, dict.get(&arena.name("A"))).into_iter().find_map(|a| {
        let handle = a.resolve(arena).as_dict_handle()?;
        let owner = arena.dict_entry(handle, arena.name("O"))?.as_name()?;
        (arena.get_name(owner)?.as_str() == attribute.owner).then_some(handle)
    });
    if let Some(handle) = owned {
        let mut entries = arena.get_dict(handle).unwrap_or_default();
        entries.insert(arena.name(&attribute.key), value);
        arena.set_dict(handle, entries);
        return Ok(());
    }
    let mut entries = BTreeMap::new();
    entries.insert(arena.name("O"), Object::Name(arena.name(&attribute.owner)));
    entries.insert(arena.name(&attribute.key), value);
    let object = arena.alloc_object(Object::Dictionary(arena.alloc_dict(entries)));
    append_attribute(arena, attribute.handle_index, Object::Reference(object))
}

/// Sets the elements an element refers to (`/Ref`, Table 355).
///
/// Each is an indirect reference to a structure element; an empty list removes the entry. A
/// target that is not an element is refused, before anything is written.
pub fn apply_set_struct_refs(doc: &Document, element: u32, targets: &[u32]) -> PdfResult<()> {
    let arena = doc.arena();
    let Some((dh, mut dict)) = element_dict(arena, element) else {
        return Err(not_an_element(element));
    };
    if let Some(missing) = targets.iter().find(|t| element_dict(arena, **t).is_none()) {
        return Err(not_an_element(*missing));
    }
    let key = arena.name("Ref");
    if targets.is_empty() {
        dict.remove(&key);
    } else {
        let refs = targets.iter().map(|t| Object::Reference(Handle::new(*t))).collect();
        dict.insert(key, Object::Array(arena.alloc_array(refs)));
    }
    arena.set_dict(dh, dict);
    Ok(())
}

/// The namespace dictionary the structure tree root's `/Namespaces` holds for `name`, added
/// there if it holds none (Table 356).
fn namespace(doc: &Document, name: &str) -> PdfResult<Handle<Object>> {
    let arena = doc.arena();
    let root = doc
        .get_structure_root()?
        .ok_or_else(|| PdfError::Other("the document has no structure tree".into()))?;
    let Some((root_dh, mut root_dict)) = element_dict(arena, root.index()) else {
        return Err(PdfError::Other("the structure tree root is not a dictionary".into()));
    };
    let key = arena.name("Namespaces");
    let mut listed = attributes_array(arena, root_dict.get(&key));
    let text = |o: Option<Object>| match o.map(|o| o.resolve(arena)) {
        Some(Object::Text(t)) => Some(t),
        Some(Object::String(b) | Object::Hex(b)) => {
            Some(fepdf_model::refine::text::recover_string(&b))
        }
        _ => None,
    };
    let found = listed.iter().find_map(|entry| {
        let handle = entry.as_reference()?;
        let dict = arena.get_object(handle)?.as_dict_handle()?;
        (text(arena.dict_entry(dict, arena.name("NS"))).as_deref() == Some(name)).then_some(handle)
    });
    if let Some(handle) = found {
        return Ok(handle);
    }
    let mut entries = BTreeMap::new();
    entries.insert(arena.name("Type"), Object::Name(arena.name("Namespace")));
    entries.insert(arena.name("NS"), Object::Text(name.to_string()));
    let handle = arena.alloc_object(Object::Dictionary(arena.alloc_dict(entries)));
    listed.push(Object::Reference(handle));
    root_dict.insert(key, Object::Array(arena.alloc_array(listed)));
    arena.set_dict(root_dh, root_dict);
    Ok(handle)
}

/// An array entry's items, or nothing.
fn attributes_array(arena: &PdfArena, entry: Option<&Object>) -> Vec<Object> {
    match entry.map(|e| e.resolve(arena)) {
        Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Puts an element in the namespace `name` (`/NS`, 14.7.4), or, for an empty name, back in
/// the default standard namespace by removing the entry.
pub fn apply_set_struct_namespace(doc: &Document, element: u32, name: &str) -> PdfResult<()> {
    let arena = doc.arena();
    if element_dict(arena, element).is_none() {
        return Err(not_an_element(element));
    }
    let reference = if name.is_empty() { None } else { Some(namespace(doc, name)?) };
    let Some((dh, mut dict)) = element_dict(arena, element) else {
        return Err(not_an_element(element));
    };
    let key = arena.name("NS");
    match reference {
        Some(handle) => dict.insert(key, Object::Reference(handle)),
        None => dict.remove(&key),
    };
    arena.set_dict(dh, dict);
    Ok(())
}

/// Maps `from` in the namespace `name` to `to` (`RoleMapNS`, Table 356): a name for a type of
/// the default standard namespace, or `[to, namespace]` for one of `to_namespace`.
pub fn apply_map_struct_type(
    doc: &Document,
    name: &str,
    (from, to): (&str, &str),
    to_namespace: Option<&str>,
) -> PdfResult<()> {
    let arena = doc.arena();
    let handle = namespace(doc, name)?;
    let target = match to_namespace {
        Some(other) => Object::Array(arena.alloc_array(vec![
            Object::Name(arena.name(to)),
            Object::Reference(namespace(doc, other)?),
        ])),
        None => Object::Name(arena.name(to)),
    };
    let Some((dh, mut dict)) = element_dict(arena, handle.index()) else {
        return Err(PdfError::Other("the namespace is not a dictionary".into()));
    };
    let key = arena.name("RoleMapNS");
    let (map_dh, mut map) = match dict.get(&key).and_then(|m| m.resolve(arena).as_dict_handle()) {
        Some(existing) => (existing, arena.get_dict(existing).unwrap_or_default()),
        None => {
            let fresh = arena.alloc_dict(BTreeMap::new());
            dict.insert(key, Object::Dictionary(fresh));
            arena.set_dict(dh, dict);
            (fresh, BTreeMap::new())
        }
    };
    map.insert(arena.name(from), target);
    arena.set_dict(map_dh, map);
    Ok(())
}

/// Associates an embedded file with a structure element (`/AF`, 14.13), after any it has.
///
/// The file specification is the one the catalogue's association makes — `/F` and `/UF`,
/// `/AFRelationship`, the stream typed `EmbeddedFile` — so an element's file meets 21-001 as
/// the catalogue's does.
pub fn apply_attach_struct_file(
    doc: &Document,
    element: u32,
    file: AssociatedFile,
) -> PdfResult<()> {
    let arena = doc.arena();
    let Some((dh, mut dict)) = element_dict(arena, element) else {
        return Err(not_an_element(element));
    };
    let size = u64::try_from(file.data.len()).unwrap_or(u64::MAX);
    let spec = crate::apply::metadata::create_embedded_filespec(
        arena,
        file.filename,
        Some(file.mime_type),
        None,
        size,
        file.data,
        Some(file.relationship),
    );
    let key = arena.name("AF");
    let mut files = attributes_array(arena, dict.get(&key));
    files.push(Object::Reference(spec));
    dict.insert(key, Object::Array(arena.alloc_array(files)));
    arena.set_dict(dh, dict);
    Ok(())
}

/// An attribute value as the object it is written as.
fn attribute_value(arena: &PdfArena, value: AttributeValue) -> Object {
    let array = |items: Vec<Object>| Object::Array(arena.alloc_array(items));
    match value {
        AttributeValue::Name(n) => Object::Name(arena.name(&n)),
        AttributeValue::Number(n) => Object::Real(n),
        AttributeValue::Text(t) => Object::Text(t),
        AttributeValue::Boolean(b) => Object::Boolean(b),
        AttributeValue::Names(ns) => {
            array(ns.iter().map(|n| Object::Name(arena.name(n))).collect())
        }
        AttributeValue::Numbers(ns) => array(ns.into_iter().map(Object::Real).collect()),
        // An ID is a byte string (14.7.2, Table 355), and so is each of a cell's `Headers`.
        AttributeValue::Strings(ss) => array(
            ss.into_iter().map(|s| Object::String(bytes::Bytes::from(s.into_bytes()))).collect(),
        ),
    }
}
