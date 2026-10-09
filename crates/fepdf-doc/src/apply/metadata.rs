#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]

use crate::operation::{
    AFRelationship, AssociatedFile, CollectionViewMode, OptionalContentProperties, OutlineNode,
    OutlineTree, OutputIntent, PortfolioCollection, VisibilityState,
};
use bytes::Bytes;
use fepdf_model::DictHandle;
use fepdf_model::arena::PdfArena;
use fepdf_model::object::SublimatedData;
use fepdf_model::{Document, Handle, Object, PdfError, PdfName, PdfResult};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Creates an embedded filespec dictionary (Clause 7.11.3).
pub fn create_embedded_filespec(
    arena: &PdfArena,
    filename: String,
    mime_type: Option<String>,
    description: Option<String>,
    size_bytes: u64,
    data: Vec<u8>,
    relationship: Option<AFRelationship>,
) -> Handle<Object> {
    let mut stream_dict = BTreeMap::new();
    stream_dict.insert(arena.name("Type"), Object::Name(arena.name("EmbeddedFile")));
    if let Some(mime) = mime_type {
        stream_dict.insert(arena.name("Subtype"), Object::Name(arena.name(&mime)));
    }
    let mut params = BTreeMap::new();
    params.insert(arena.name("Size"), Object::Integer(size_bytes as i64));
    let params_dh = arena.alloc_dict(params);
    stream_dict.insert(arena.name("Params"), Object::Dictionary(params_dh));

    let stream_dh = arena.alloc_dict(stream_dict);
    let stream_obj = Object::Stream(stream_dh, Arc::new(SublimatedData::Raw(Bytes::from(data))));
    let stream_h = arena.alloc_object(stream_obj);

    let mut ef_dict = BTreeMap::new();
    ef_dict.insert(arena.name("F"), Object::Reference(stream_h));
    ef_dict.insert(arena.name("UF"), Object::Reference(stream_h));
    let ef_dh = arena.alloc_dict(ef_dict);

    let mut filespec = BTreeMap::new();
    filespec.insert(arena.name("Type"), Object::Name(arena.name("Filespec")));
    // **`/F` stays a byte string and `/UF` becomes text.** Table 43 types the two
    // differently on purpose: `/F` is a *file specification string* (7.11.2), whose bytes
    // are a path in the slash-separated form that clause defines, and `/UF` is the text
    // string that exists precisely because `/F` could not carry Unicode. Writing `/F` as
    // text would put a BOM in front of a path.
    filespec.insert(arena.name("F"), Object::String(Bytes::from(filename.clone())));
    filespec.insert(arena.name("UF"), Object::Text(filename));
    if let Some(rel) = relationship {
        let af_rel = match rel {
            AFRelationship::Source => "Source",
            AFRelationship::Data => "Data",
            AFRelationship::Supplement => "Supplement",
            AFRelationship::Alternative => "Alternative",
            AFRelationship::EncryptedPayload => "EncryptedPayload",
            AFRelationship::Unspecified => "Unspecified",
        };
        filespec.insert(arena.name("AFRelationship"), Object::Name(arena.name(af_rel)));
    }
    filespec.insert(arena.name("EF"), Object::Dictionary(ef_dh));
    if let Some(desc) = description {
        // A text string (Table 43): it is shown to a reader, not matched against anything.
        filespec.insert(arena.name("Desc"), Object::Text(desc));
    }
    let filespec_dh = arena.alloc_dict(filespec);
    arena.alloc_object(Object::Dictionary(filespec_dh))
}

/// Attaches an embedded file to the catalogue, in both the places 2.0 names it.
///
/// `/AF` (14.13) is the *association* — what this file is to the document — and
/// `/Names/EmbeddedFiles` (7.11.4) is how a reader lists attachments. A file in one and
/// not the other is either an association nothing can open or an attachment stating no
/// relationship, so the two are written together.
///
/// **This stood written out twice**, in `apply_attach_associated_file` and in
/// `apply_set_unencrypted_wrapper`, identical down to the `Object::Reference` arm that
/// follows an indirect `/AF`. A document with no catalogue is left alone, as both copies
/// left it.
pub fn attach_to_catalog(
    doc: &Document,
    filename: String,
    filespec_h: Handle<Object>,
) -> PdfResult<()> {
    let arena = doc.arena();
    let Some(cah) = doc.catalog_handle() else {
        return Ok(());
    };
    let cadh = doc.resolve_to_dict(cah)?;
    let mut cdict = arena.get_dict(cadh).unwrap_or_default();

    let af_key = arena.name("AF");
    let mut af_items = match cdict.get(&af_key) {
        Some(Object::Array(ah)) => arena.get_array(*ah).unwrap_or_default(),
        Some(Object::Reference(h)) => match arena.get_object(*h) {
            Some(Object::Array(ah)) => arena.get_array(ah).unwrap_or_default(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    af_items.push(Object::Reference(filespec_h));
    let new_af_ah = arena.alloc_array(af_items);
    cdict.insert(af_key, Object::Array(new_af_ah));
    arena.set_dict(cadh, cdict);

    add_embedded_files_to_catalog(doc, vec![(filename, filespec_h)])
}

/// Adds embedded filespec entries to the catalogue's `/EmbeddedFiles` name tree (7.11.4).
///
/// **What the trees held is kept.** A `/Names` written in place was replaced by a new one
/// holding the attachment alone, so the named destinations and scripts beside it went; an
/// `/EmbeddedFiles` tree with `/Kids` was read as having no entries, so the files already
/// attached went. The tree is read whole, the new entries join it — one of an existing
/// name replacing it, since a tree's keys are unique — and it is written back as one leaf
/// in key order, which 7.9.6 asks of a tree's keys.
pub fn add_embedded_files_to_catalog(
    doc: &Document,
    new_entries: Vec<(String, Handle<Object>)>,
) -> PdfResult<()> {
    let arena = doc.arena();
    let Some(cah) = doc.catalog_handle() else { return Ok(()) };
    let cadh = doc.resolve_to_dict(cah)?;
    let names_key = arena.name("Names");
    let names_dh = match arena.dict_entry(cadh, names_key).map(|n| n.resolve(arena)) {
        Some(Object::Dictionary(existing)) => existing,
        _ => {
            let fresh = arena.alloc_dict(BTreeMap::new());
            let mut cdict = arena.get_dict(cadh).unwrap_or_default();
            cdict.insert(names_key, Object::Dictionary(fresh));
            arena.set_dict(cadh, cdict);
            fresh
        }
    };
    let ef_key = arena.name("EmbeddedFiles");
    let mut entries: BTreeMap<Vec<u8>, Object> = BTreeMap::new();
    if let Some(tree) = arena.dict_entry(names_dh, ef_key) {
        name_tree_leaves(arena, &tree, &mut entries);
    }
    // **A name-tree key is a byte string, not a text string** (7.9.6). It is what a lookup
    // compares bytes against — `/EmbeddedFiles` is keyed on it, the collection `/D` below
    // names one, and so does a `GoToE` target's `/N` in `apply::annotations`. Encoding it
    // as text would put a BOM on one side of every one of those comparisons.
    for (filename, filespec_h) in new_entries {
        entries.insert(filename.into_bytes(), Object::Reference(filespec_h));
    }
    let pairs: Vec<Object> = entries
        .into_iter()
        .flat_map(|(key, value)| [Object::String(Bytes::from(key)), value])
        .collect();
    let mut leaf = BTreeMap::new();
    leaf.insert(arena.name("Names"), Object::Array(arena.alloc_array(pairs)));
    let leaf_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(leaf)));
    let mut names_dict = arena.get_dict(names_dh).unwrap_or_default();
    names_dict.insert(ef_key, Object::Reference(leaf_h));
    arena.set_dict(names_dh, names_dict);
    Ok(())
}

/// How deep a name tree's `/Kids` are followed (Rule 6): `intel_sdm.pdf`'s 279,501
/// destinations are three levels deep.
const NAME_TREE_DEPTH: usize = 32;

/// Every key and value of the name tree `node`, through `/Kids`, to a bounded depth.
fn name_tree_leaves(arena: &PdfArena, node: &Object, into: &mut BTreeMap<Vec<u8>, Object>) {
    let mut waiting = vec![(node.clone(), 0)];
    while let Some((node, depth)) = waiting.pop() {
        let Some(dict) = node.resolve(arena).as_dict_handle() else { continue };
        let array = |key: &str| match arena.dict_entry(dict, arena.name(key))?.resolve(arena) {
            Object::Array(array) => arena.get_array(array),
            _ => None,
        };
        if let Some(kids) = array("Kids") {
            if depth < NAME_TREE_DEPTH {
                waiting.extend(kids.into_iter().map(|kid| (kid, depth + 1)));
            }
        } else if let Some(pairs) = array("Names") {
            for pair in pairs.chunks(2) {
                let key = match pair.first().map(|k| k.resolve(arena)) {
                    Some(Object::String(key) | Object::Hex(key)) => key.to_vec(),
                    Some(Object::Text(key)) => key.into_bytes(),
                    _ => continue,
                };
                if let Some(value) = pair.get(1) {
                    into.insert(key, value.clone());
                }
            }
        }
    }
}

/// Creates a portfolio collection (Clause 12.3.5).
pub fn apply_create_portfolio(doc: &Document, portfolio: PortfolioCollection) -> PdfResult<()> {
    let arena = doc.arena();
    let mut collection_dict = BTreeMap::new();
    let view_name = match portfolio.view_mode {
        CollectionViewMode::Details => "D",
        CollectionViewMode::Tile => "T",
        CollectionViewMode::Hidden => "H",
    };
    collection_dict.insert(arena.name("View"), Object::Name(arena.name(view_name)));
    if let Some(init_doc) = portfolio.initial_document {
        // A byte string, because it must equal a key of the `/EmbeddedFiles` name tree
        // written above — see the note there.
        collection_dict.insert(arena.name("D"), Object::String(Bytes::from(init_doc)));
    }
    let col_dh = arena.alloc_dict(collection_dict);
    let col_h = arena.alloc_object(Object::Dictionary(col_dh));

    let mut new_entries = Vec::new();
    for item in portfolio.items {
        let filespec_h = create_embedded_filespec(
            arena,
            item.filename.clone(),
            item.mime_type,
            item.description,
            item.size_bytes,
            item.data,
            None,
        );
        new_entries.push((item.filename, filespec_h));
    }

    if let Some(cah) = doc.catalog_handle() {
        let cadh = doc.resolve_to_dict(cah)?;
        let mut cdict = arena.get_dict(cadh).unwrap_or_default();
        cdict.insert(arena.name("Collection"), Object::Reference(col_h));
        arena.set_dict(cadh, cdict);

        add_embedded_files_to_catalog(doc, new_entries)?;
    }
    Ok(())
}

/// How deep an outline may nest before it is refused.
///
/// **The depth here does not come from a file**, which is what makes this tree different
/// from every other one the engine walks. `Operation` is a caller's value: the parser's
/// 512-level limit bounds everything that starts from bytes and is not in this path.
///
/// **This bound is on Rule 6's terms and not on a demonstrated crash, and the difference
/// is worth stating** — the sweep that found it recorded a stack overflow at ten thousand
/// levels, and measuring where that overflow actually was moved it somewhere else. What
/// is measured, 2026-09-05:
///
/// | | |
/// | :--- | :--- |
/// | `serde_json` refuses an `OutlineNode` past **62** levels | so `fepdf-mcp`, the only caller that deserialises one, cannot reach this at all |
/// | `OutlineNode`'s derived `Drop` overflows the stack between **5,000** and **10,000** | so a Rust caller that builds one that deep aborts when the value is released, whatever this function does |
///
/// The recursion sat between those two numbers, which is why nothing had crashed here.
/// It is bounded anyway, because "no caller can currently reach it" is a fact about
/// today's callers and Rule 6 is not conditional on one. Sixty-four is what the
/// field-tree and `/Next` walks use.
///
/// The `Drop` cliff is **not** fixed by this and belongs to the type: a manual `Drop` for
/// `OutlineNode` in `fepdf-model` is what would move it, and that is a change to a public
/// type rather than to this walk.
const MAX_OUTLINE_DEPTH: usize = 64;

/// Builds one level of the outline and every level below it.
///
/// Refuses past [`MAX_OUTLINE_DEPTH`] rather than truncating: an outline silently missing
/// the levels a caller asked for is worse than one the caller is told was not built.
/// Fills in one outline item's dictionary, and every level below it.
///
/// Split out of [`build_outline_level`] to keep that under RR-15 Rule 1's fifty lines
/// once it took a depth. `where` carries the three handles the siblings decide:
/// this item's index, its parent, and its own object handle.
/// The outline item `node` was read from, where it names one and that is still an
/// outline item — a dictionary with a `/Title`.
fn source_item(doc: &Document, node: &OutlineNode) -> Option<BTreeMap<Handle<PdfName>, Object>> {
    let arena = doc.arena();
    let item = arena.get_object(arena.handle(node.source?))?.as_dict_handle()?;
    let dict = arena.get_dict(item)?;
    dict.contains_key(&arena.name("Title")).then_some(dict)
}

/// Copies onto `dict` what an item read carried that [`OutlineNode`] does not model: its
/// colour, style and structure element, and an action other than a go-to, which then
/// stands instead of the destination (ROADMAP Y-F11). A go-to is the page the node names.
fn carry_unmodelled(
    arena: &fepdf_model::PdfArena,
    source: &BTreeMap<Handle<PdfName>, Object>,
    dict: &mut BTreeMap<Handle<PdfName>, Object>,
) {
    for key in ["C", "F", "SE"] {
        if let Some(value) = source.get(&arena.name(key)) {
            dict.insert(arena.name(key), value.clone());
        }
    }
    let Some(action) = source.get(&arena.name("A")) else { return };
    let kind = action
        .resolve(arena)
        .as_dict_handle()
        .and_then(|a| arena.dict_entry(a, arena.name("S")))
        .and_then(|s| s.as_name());
    if kind != Some(arena.name("GoTo")) {
        dict.insert(arena.name("A"), action.clone());
        dict.remove(&arena.name("Dest"));
    }
}

fn build_outline_item(
    doc: &Document,
    node: &OutlineNode,
    siblings: &[(DictHandle, Handle<Object>)],
    where_: (usize, Handle<Object>, Handle<Object>),
    depth: usize,
) -> PdfResult<usize> {
    let arena = doc.arena();
    let (index, parent_h, self_h) = where_;
    let mut dict = BTreeMap::new();
    // **`Object::Text`, not `Object::String`.** A `/Title` is a text string (7.9.2.2),
    // which is PDFDocEncoding or a marked UTF-16BE/UTF-8 — never raw UTF-8 bytes. Handing
    // the writer the bytes wrote exactly those, so a bookmark titled `第一章` reached the
    // file as `ç¬¬ä¸\u{80}ç«\u{a0}` and read back that way. `Object::Text` leaves the
    // encoding to the writer, which is where the document's choice of encoding lives.
    dict.insert(arena.name("Title"), Object::Text(node.title.clone()));
    dict.insert(arena.name("Parent"), Object::Reference(parent_h));

    if let Some(&(_, prev)) = index.checked_sub(1).and_then(|i| siblings.get(i)) {
        dict.insert(arena.name("Prev"), Object::Reference(prev));
    }
    if let Some(&(_, next)) = siblings.get(index + 1) {
        dict.insert(arena.name("Next"), Object::Reference(next));
    }

    // A bookmark to a page the document does not have is refused, as a link to one is: it
    // was written with no destination and answered `Ok`.
    let page_h = doc.page_handle(node.destination_page)?;
    let dest_items = vec![Object::Reference(page_h), Object::Name(arena.name("Fit"))];
    dict.insert(arena.name("Dest"), Object::Array(arena.alloc_array(dest_items)));
    let source = source_item(doc, node);
    if let Some(source) = &source {
        carry_unmodelled(arena, source, &mut dict);
    }

    let below = if node.children.is_empty() {
        0
    } else {
        let (first_child_h, last_child_h, child_count) =
            build_outline_level(doc, &node.children, self_h, depth + 1)?;
        dict.insert(arena.name("First"), Object::Reference(first_child_h));
        dict.insert(arena.name("Last"), Object::Reference(last_child_h));
        // Negative where the item read was closed (12.3.3, Table 151).
        let closed = source
            .as_ref()
            .and_then(|s| s.get(&arena.name("Count")))
            .and_then(Object::as_integer)
            .is_some_and(|count| count < 0);
        let count = child_count as i64;
        dict.insert(arena.name("Count"), Object::Integer(if closed { -count } else { count }));
        child_count
    };

    if let Some(&(own, _)) = siblings.get(index) {
        arena.set_dict(own, dict);
    }
    Ok(below)
}

fn build_outline_level(
    doc: &Document,
    nodes: &[OutlineNode],
    parent_h: Handle<Object>,
    depth: usize,
) -> PdfResult<(Handle<Object>, Handle<Object>, usize)> {
    let arena = doc.arena();
    if depth >= MAX_OUTLINE_DEPTH {
        return Err(PdfError::refused(
            "UpdateOutlines",
            format!("the outline nests deeper than {MAX_OUTLINE_DEPTH} levels"),
        ));
    }
    if nodes.is_empty() {
        return Err(PdfError::refused("UpdateOutlines", "Empty outline level"));
    }

    let mut handles = Vec::new();
    let mut total_count = nodes.len();

    for _ in nodes {
        let dh = arena.alloc_dict(BTreeMap::new());
        let h = arena.alloc_object(Object::Dictionary(dh));
        handles.push((dh, h));
    }

    for (i, (node, &(_, h))) in nodes.iter().zip(handles.iter()).enumerate() {
        total_count += build_outline_item(doc, node, &handles, (i, parent_h, h), depth)?;
    }

    // `nodes` is not empty, so neither is `handles`; the refusal is the same either way.
    let (Some(&(_, first_h)), Some(&(_, last_h))) = (handles.first(), handles.last()) else {
        return Err(PdfError::refused("UpdateOutlines", "Empty outline level"));
    };
    Ok((first_h, last_h, total_count))
}

/// Updates document outlines / bookmarks tree (Clause 12.3.3).
pub fn apply_update_outlines(doc: &Document, outlines: OutlineTree) -> PdfResult<()> {
    let arena = doc.arena();
    if outlines.items.is_empty() {
        if let Some(cah) = doc.catalog_handle() {
            let cadh = doc.resolve_to_dict(cah)?;
            let mut cdict = arena.get_dict(cadh).unwrap_or_default();
            cdict.remove(&arena.name("Outlines"));
            arena.set_dict(cadh, cdict);
        }
        return Ok(());
    }

    let mut outlines_root_dict = BTreeMap::new();
    outlines_root_dict.insert(arena.name("Type"), Object::Name(arena.name("Outlines")));
    let outlines_root_dh = arena.alloc_dict(outlines_root_dict);
    let outlines_root_h = arena.alloc_object(Object::Dictionary(outlines_root_dh));

    let (first_h, last_h, count) = build_outline_level(doc, &outlines.items, outlines_root_h, 0)?;

    let mut root_d = arena.get_dict(outlines_root_dh).unwrap_or_default();
    root_d.insert(arena.name("First"), Object::Reference(first_h));
    root_d.insert(arena.name("Last"), Object::Reference(last_h));
    root_d.insert(arena.name("Count"), Object::Integer(count as i64));
    arena.set_dict(outlines_root_dh, root_d);

    if let Some(cah) = doc.catalog_handle() {
        let cadh = doc.resolve_to_dict(cah)?;
        let mut cdict = arena.get_dict(cadh).unwrap_or_default();
        cdict.insert(arena.name("Outlines"), Object::Reference(outlines_root_h));
        arena.set_dict(cadh, cdict);
    }
    Ok(())
}

/// Updates Optional Content Groups (OCG layers, Clause 8.11).
///
/// Writes the groups, the default configuration, and — since Phase N — the `/Usage` that
/// carries [`fepdf_model::LayerGroup::printable`] into the file with the `/AS` entry that
/// applies it. Without the second, a `/Usage` is a description no viewer acts on
/// (8.11.4.5), which is how "printable" set by a caller reached the file as nothing at
/// all.
///
/// Putting *content* in one of these groups is `Operation::AddPageDecoration`'s job. The
/// two used to have no connection: this wrote layers, and nothing anywhere was ever
/// marked `/OC`, so every group the engine created was empty whatever its state.
///
/// **The groups content is in are kept** (8.11.2). A layer named as one the document has
/// is that group, with its usage brought up to date, not a new one of the same name; and
/// a group not named stays in `/OCGs` as it was, on or off, since `/OCGs` lists every
/// group in the document (8.11.4.2). Each layer was made anew, so what the pages had
/// marked `/OC` belonged to groups the document no longer listed, and turning a layer off
/// turned off nothing on the page.
pub fn apply_update_layers(doc: &Document, layers: OptionalContentProperties) -> PdfResult<()> {
    let arena = doc.arena();
    let existing = existing_groups(doc);
    let (mut ocg_refs, mut on_refs, mut off_refs) = (Vec::new(), Vec::new(), Vec::new());
    let mut claimed = std::collections::BTreeSet::new();
    for layer in layers.layers {
        let reused = existing
            .iter()
            .find(|(h, name, _)| {
                !claimed.contains(h) && name.as_deref() == Some(layer.name.as_str())
            })
            .map(|(h, _, _)| *h);
        let ocg_h = write_group(doc, reused, layer.name, layer.printable);
        claimed.insert(ocg_h);
        ocg_refs.push(Object::Reference(ocg_h));
        match layer.default_state {
            VisibilityState::On => on_refs.push(Object::Reference(ocg_h)),
            VisibilityState::Off => off_refs.push(Object::Reference(ocg_h)),
        }
    }
    for (handle, _, was_off) in existing.into_iter().filter(|(h, _, _)| !claimed.contains(h)) {
        ocg_refs.push(Object::Reference(handle));
        if was_off { &mut off_refs } else { &mut on_refs }.push(Object::Reference(handle));
    }

    let ocgs_ah = arena.alloc_array(ocg_refs.clone());
    let on_ah = arena.alloc_array(on_refs);
    let off_ah = arena.alloc_array(off_refs);
    let order_ah = arena.alloc_array(ocg_refs.clone());
    let as_ah = arena.alloc_array(vec![Object::Dictionary(print_application(doc, &ocg_refs))]);

    let mut d_dict = BTreeMap::new();
    // A text string too (Table 100), and left a byte string deliberately: the value is
    // this literal, whose PDFDocEncoded form is byte for byte what is written here. There
    // is no input that could reach it from outside, so no test could tell the two spellings
    // apart, and `Object::Text` would only add a BOM.
    d_dict.insert(arena.name("Name"), Object::String(Bytes::from("Default")));
    d_dict.insert(arena.name("BaseState"), Object::Name(arena.name("ON")));
    d_dict.insert(arena.name("ON"), Object::Array(on_ah));
    d_dict.insert(arena.name("OFF"), Object::Array(off_ah));
    d_dict.insert(arena.name("Order"), Object::Array(order_ah));
    d_dict.insert(arena.name("AS"), Object::Array(as_ah));
    let d_dh = arena.alloc_dict(d_dict);

    let mut oc_props = BTreeMap::new();
    oc_props.insert(arena.name("OCGs"), Object::Array(ocgs_ah));
    oc_props.insert(arena.name("D"), Object::Dictionary(d_dh));
    let oc_dh = arena.alloc_dict(oc_props);
    let oc_h = arena.alloc_object(Object::Dictionary(oc_dh));

    if let Some(cah) = doc.catalog_handle() {
        let cadh = doc.resolve_to_dict(cah)?;
        let mut cdict = arena.get_dict(cadh).unwrap_or_default();
        cdict.insert(arena.name("OCProperties"), Object::Reference(oc_h));
        arena.set_dict(cadh, cdict);
    }
    Ok(())
}

/// The groups the document's `/OCProperties` lists: each one's handle, its name, and
/// whether the default configuration turns it off.
fn existing_groups(doc: &Document) -> Vec<(Handle<Object>, Option<String>, bool)> {
    let arena = doc.arena();
    let entry = |dict, key: &str| arena.dict_entry(dict, arena.name(key)).map(|v| v.resolve(arena));
    let handles = |object: Option<Object>| match object {
        Some(Object::Array(array)) => arena
            .get_array(array)
            .unwrap_or_default()
            .iter()
            .filter_map(Object::as_reference)
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let Some(properties) = doc
        .catalog_handle()
        .and_then(|c| doc.resolve_to_dict(c).ok())
        .and_then(|c| entry(c, "OCProperties"))
        .and_then(|p| p.as_dict_handle())
    else {
        return Vec::new();
    };
    let off =
        entry(properties, "D").and_then(|d| d.as_dict_handle()).map(|d| handles(entry(d, "OFF")));
    let off = off.unwrap_or_default();
    handles(entry(properties, "OCGs"))
        .into_iter()
        .map(|group| {
            let dict = arena.get_object(group).and_then(|o| o.as_dict_handle());
            let name = dict.and_then(|d| entry(d, "Name")).and_then(|n| match n {
                Object::Text(text) => Some(text),
                Object::String(b) | Object::Hex(b) => {
                    Some(fepdf_model::refine::text::recover_string(&b))
                }
                _ => None,
            });
            (group, name, off.contains(&group))
        })
        .collect()
}

/// Writes a group named `name`: into `reused`, keeping what else it says, or as a new one.
fn write_group(
    doc: &Document,
    reused: Option<Handle<Object>>,
    name: String,
    printable: bool,
) -> Handle<Object> {
    let arena = doc.arena();
    let existing = reused.and_then(|h| arena.get_object(h)?.as_dict_handle());
    let mut dict = existing.and_then(|d| arena.get_dict(d)).unwrap_or_default();
    dict.insert(arena.name("Type"), Object::Name(arena.name("OCG")));
    // A text string (Table 98): this is the layer name a reader shows in its UI.
    dict.insert(arena.name("Name"), Object::Text(name));
    dict.insert(arena.name("Usage"), Object::Dictionary(print_usage(doc, printable)));
    match (reused, existing) {
        (Some(handle), Some(dh)) => {
            arena.set_dict(dh, dict);
            handle
        }
        _ => arena.alloc_object(Object::Dictionary(arena.alloc_dict(dict))),
    }
}

/// A group's `/Usage`, carrying whether it should be printed (8.11.4.4, Table 103).
///
/// Only `/Print`. Whether the layer is *visible* is what the configuration's `/ON` and
/// `/OFF` say, and writing a `/View` state beside them would be the same fact twice, free
/// to disagree with itself.
fn print_usage(doc: &Document, printable: bool) -> DictHandle {
    let arena = doc.arena();
    let state = if printable { "ON" } else { "OFF" };
    let mut print = BTreeMap::new();
    print.insert(arena.name("PrintState"), Object::Name(arena.name(state)));
    let mut usage = BTreeMap::new();
    usage.insert(arena.name("Print"), Object::Dictionary(arena.alloc_dict(print)));
    arena.alloc_dict(usage)
}

/// The `/AS` entry that makes those `/Usage` dictionaries take effect (8.11.4.5, Table 101).
///
/// A usage dictionary on its own changes nothing: 8.11.4.5 puts the acting in the
/// *application*, which has to name both the event and the category. One entry covering
/// every group, because every group above is written with a `/Print` usage.
fn print_application(doc: &Document, groups: &[Object]) -> DictHandle {
    let arena = doc.arena();
    let mut application = BTreeMap::new();
    application.insert(arena.name("Event"), Object::Name(arena.name("Print")));
    application.insert(arena.name("OCGs"), Object::Array(arena.alloc_array(groups.to_vec())));
    application.insert(
        arena.name("Category"),
        Object::Array(arena.alloc_array(vec![Object::Name(arena.name("Print"))])),
    );
    arena.alloc_dict(application)
}

/// Attaches an associated file to the catalogue (Clause 14.13).
pub fn apply_attach_associated_file(doc: &Document, file: AssociatedFile) -> PdfResult<()> {
    let arena = doc.arena();
    let filespec_h = create_embedded_filespec(
        arena,
        file.filename.clone(),
        Some(file.mime_type),
        None,
        file.data.len() as u64,
        file.data,
        Some(file.relationship),
    );

    attach_to_catalog(doc, file.filename, filespec_h)
}

/// Sets PDF/X or PDF/A OutputIntents dictionary (Clause 14.11.5).
///
/// # Errors
/// Fails when the profile given is not an ICC profile, or is one of a colour space an ICC
/// stream's `/N` cannot state (8.6.5.5).
pub fn apply_set_output_intent(doc: &Document, intent: OutputIntent) -> PdfResult<()> {
    let arena = doc.arena();
    let components = intent.icc_profile_bytes.as_deref().map(icc_components).transpose()?;
    let mut oi_dict = BTreeMap::new();
    oi_dict.insert(arena.name("Type"), Object::Name(arena.name("OutputIntent")));
    oi_dict.insert(arena.name("S"), Object::Name(arena.name(&intent.subtype)));
    // Both text strings (Table 401). A registered condition's identifier is ASCII in
    // practice, but the entry's type is what decides how it is written, not what the ICC
    // registry happens to hold.
    oi_dict.insert(arena.name("OutputConditionIdentifier"), Object::Text(intent.identifier));
    if let Some(info) = intent.info {
        oi_dict.insert(arena.name("Info"), Object::Text(info));
    }
    if let Some(icc_data) = intent.icc_profile_bytes {
        let mut stream_dict = BTreeMap::new();
        stream_dict.insert(arena.name("N"), Object::Integer(components.unwrap_or(3)));
        let stream_dh = arena.alloc_dict(stream_dict);
        let stream_obj =
            Object::Stream(stream_dh, Arc::new(SublimatedData::Raw(Bytes::from(icc_data))));
        let stream_h = arena.alloc_object(stream_obj);
        oi_dict.insert(arena.name("DestOutputProfile"), Object::Reference(stream_h));
    }
    let oi_dh = arena.alloc_dict(oi_dict);
    let oi_h = arena.alloc_object(Object::Dictionary(oi_dh));

    if let Some(cah) = doc.catalog_handle() {
        let cadh = doc.resolve_to_dict(cah)?;
        let mut cdict = arena.get_dict(cadh).unwrap_or_default();
        let oi_key = arena.name("OutputIntents");
        let mut oi_items = if let Some(existing_oi) = cdict.get(&oi_key) {
            match existing_oi {
                Object::Array(ah) => arena.get_array(*ah).unwrap_or_default(),
                Object::Reference(h) => {
                    if let Some(Object::Array(ah)) = arena.get_object(*h) {
                        arena.get_array(ah).unwrap_or_default()
                    } else {
                        Vec::new()
                    }
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        oi_items.push(Object::Reference(oi_h));
        let oi_ah = arena.alloc_array(oi_items);
        cdict.insert(oi_key, Object::Array(oi_ah));
        arena.set_dict(cadh, cdict);
    }
    Ok(())
}

/// How many components the ICC profile `profile` has, from its header (ICC.1 7.2): what
/// an ICC stream's `/N` states (Table 66). It was 3 whatever the profile, so the CMYK
/// profile a PDF/X output intent names was declared three-component.
///
/// # Errors
/// Fails when `profile` carries no ICC header, or names a colour space `/N` cannot state.
fn icc_components(profile: &[u8]) -> PdfResult<i64> {
    if profile.get(36..40) != Some(b"acsp".as_slice()) {
        return Err(PdfError::refused(
            "SetOutputIntent",
            "the output intent's profile is not an ICC profile: it has no acsp signature",
        ));
    }
    match profile.get(16..20) {
        Some(b"GRAY") => Ok(1),
        Some(b"RGB " | b"Lab ") => Ok(3),
        Some(b"CMYK") => Ok(4),
        other => Err(PdfError::refused(
            "SetOutputIntent",
            format!(
                "the output intent's profile is of the colour space {:?}, which /N cannot state",
                other.map(String::from_utf8_lossy)
            ),
        )),
    }
}

/// Names a pronunciation lexicon in the structure tree root (Table 354, 14.9.6).
///
/// **It was written into the catalogue as `/PL`, a key ISO 32000-2 does not have.** The
/// standard's entry is `/PronunciationLexicon` on the structure tree root, an array of
/// file specifications, so every lexicon this engine wrote was one no reader would look
/// for; the test beside it asserted `/PL` was there. It is an embedded file now, in the
/// entry the standard names, replacing any the root had.
///
/// # Errors
/// Fails when the document has no structure tree: the lexicon is an entry of its root,
/// and making one would make an untagged document claim a structure it does not have.
pub fn apply_set_pronunciation_lexicon(doc: &Document, bytes: Vec<u8>) -> PdfResult<()> {
    let arena = doc.arena();
    let catalog = doc
        .catalog_handle()
        .ok_or_else(|| PdfError::violation("7.7.2", "the document has no catalogue"))?;
    let catalog = doc.resolve_to_dict(catalog)?;
    let Some(root) = arena
        .dict_entry(catalog, arena.name("StructTreeRoot"))
        .and_then(|root| root.resolve(arena).as_dict_handle())
    else {
        return Err(PdfError::refused(
            "SetPronunciationLexicon",
            "a pronunciation lexicon is named by the structure tree root (Table 354), and \
             this document has no structure tree",
        ));
    };
    let size = bytes.len() as u64;
    let spec = create_embedded_filespec(
        arena,
        "lexicon.pls".to_owned(),
        Some("application/pls+xml".to_owned()),
        None,
        size,
        bytes,
        None,
    );
    let mut dict = arena.get_dict(root).unwrap_or_default();
    let named = arena.alloc_array(vec![Object::Reference(spec)]);
    dict.insert(arena.name("PronunciationLexicon"), Object::Array(named));
    arena.set_dict(root, dict);
    Ok(())
}
