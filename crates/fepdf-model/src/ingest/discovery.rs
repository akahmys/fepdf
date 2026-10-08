use crate::Document;
use crate::PdfName;
use crate::arena::PdfArena;
use crate::font::FontResource;
use crate::handle::Handle;
use crate::object::Object;
use std::collections::BTreeMap;
use std::sync::Arc;

#[allow(clippy::too_many_arguments)]
fn process_font_object(
    arena: &PdfArena,
    doc: &Document,
    i: u32,
    type_key: Handle<PdfName>,
    font_val: Handle<PdfName>,
    base_font_key: Handle<PdfName>,
    subtype_key: Handle<PdfName>,
    cache: &mut BTreeMap<u32, Arc<FontResource>>,
) {
    let obj_handle = Handle::new(i);
    if let Some(Object::Dictionary(dict_handle)) = arena.get_object(obj_handle)
        && let Some(dict) = arena.get_dict(dict_handle)
    {
        let type_val = dict.get(&type_key).and_then(|o| o.resolve(arena).as_name());
        let is_font = if let Some(tv) = type_val {
            tv == font_val
        } else {
            dict.contains_key(&base_font_key) && dict.contains_key(&subtype_key)
        };

        if is_font && let Ok(font_res) = FontResource::load(&dict, doc) {
            cache.insert(obj_handle.index(), Arc::new(font_res));
        }
    }
}

/// Gives every font dictionary written direct an object of its own, and the `/Font`
/// entry that held it a reference to that object.
///
/// **7.3.10 lets any object be direct**, and a `/Font` resource written as
/// `/F1 << /Type /Font … >>` is conforming. Every route that finds a font here keys it
/// by object number, so such a font was found by none of them: ingestion recorded a 9.6.2
/// repair saying the resources did not define a font they defined, and drew the text in
/// a substitute. The interpreter's own fallback gave the dictionary a fresh object every
/// time a `Tf` selected it, so the arena grew by one object a selection and the font was
/// built again each time. Measured 2026-09-24 with `--example direct_fonts`: 5 of the 266
/// files with a font resource in the samples and the external corpus, all in
/// `pdf-differences`.
///
/// **Lifted, not special-cased**, because a font with a number reaches every route that
/// already works — refinement, `Document::get_font`, the runs, appearances — and the
/// document it describes is the same one (7.3.10). Nothing is recorded: the file did
/// nothing wrong.
///
/// Every dictionary in a resource dictionary's `/Font` is a font (7.8.3), so none is asked
/// whether it is one. An `ExtGState`'s `/Font` is an array, not a dictionary, and is left
/// as it is.
pub fn lift_direct_fonts(arena: &PdfArena) {
    let font_key = arena.name("Font");
    for holder in arena.all_dict_handles() {
        let Some(Object::Dictionary(fonts)) =
            arena.dict_entry(holder, font_key).map(|entry| entry.resolve(arena))
        else {
            continue;
        };
        let Some(mut entries) = arena.get_dict(fonts) else { continue };
        let mut lifted = false;
        for entry in entries.values_mut() {
            if let Object::Dictionary(font) = entry {
                *entry = Object::Reference(arena.alloc_object(Object::Dictionary(*font)));
                lifted = true;
            }
        }
        if lifted {
            arena.set_dict(fonts, entries);
        }
    }
}

/// Gives a `CIDFontType2` with an embedded program the `/CIDToGIDMap /Identity` it lacks,
/// and says so.
///
/// **Table 115 requires the entry of exactly that font** — a Type 2 CIDFont whose
/// descriptor carries `/FontFile2` — and gives it no default, so a file without it is wrong
/// and `Identity` is the reading every reader takes. This filled it on every `CIDFontType0`
/// and `CIDFontType2`, embedded or not, and recorded nothing, so a Type 0 CIDFont, whose
/// table defines no such entry, left with one, and the audit's 31-004 never saw an absent
/// one (ROADMAP Y-F15). A font this leaves alone still reads its CIDs as glyph indices: the
/// loader takes an absent map for `Identity`.
pub fn require_cid_to_gid_maps(
    arena: &PdfArena,
    decisions: &mut crate::interpretation::DecisionLog,
) {
    let key = arena.name("CIDToGIDMap");
    for holder in arena.all_dict_handles() {
        let Some(mut dict) = arena.get_dict(holder) else { continue };
        let named = |k: &str| {
            dict.get(&arena.name(k))
                .and_then(|o| o.resolve(arena).as_name())
                .and_then(|n| arena.get_name_str(n))
        };
        if named("Subtype").as_deref() != Some("CIDFontType2") || dict.contains_key(&key) {
            continue;
        }
        let embedded = dict
            .get(&arena.name("FontDescriptor"))
            .is_some_and(|d| crate::access::entry(arena, d, "FontFile2").is_some());
        if !embedded {
            continue;
        }
        let name = named("BaseFont").unwrap_or_default();
        dict.insert(key, Object::Name(arena.name("Identity")));
        arena.set_dict(holder, dict);
        decisions.push(crate::interpretation::Decision::repaired(
            "9.7.4.1",
            missing_cid_to_gid_map(&name),
            "gave it /Identity, the mapping a reader takes when it is absent",
        ));
    }
}

/// What [`require_cid_to_gid_maps`] records it found in the font named `name`: the one
/// place these words are written, since the audit's 31-005 reads the repair back by them.
#[must_use]
pub fn missing_cid_to_gid_map(name: &str) -> String {
    format!("the embedded CIDFontType2 /{name} has no /CIDToGIDMap, which Table 115 requires")
}

/// Gives a page an empty `/Resources` where neither it nor any node above it has a
/// dictionary there, and says how many.
///
/// **Table 31 requires the entry, inherited or the page's own.** Where it was missing,
/// `Page::resources_handle` allocated an empty dictionary on every call, so rendering
/// such a page or reading its fonts wrote into the document each time — a reader writing,
/// which the sealed arena turned into a failing test (ROADMAP Y-11).
///
/// Run once the pages are indexed, which ingestion's other passes run before.
pub fn require_page_resources(doc: &Document) {
    let arena = doc.arena();
    let key = arena.name("Resources");
    let mut given = 0;
    for &page in &doc.pages {
        let view = crate::Page::new(arena, page, doc.get_parent_chain(page));
        if view.resolve_attribute("Resources").and_then(|r| r.as_dict_handle()).is_some() {
            continue;
        }
        let Some(holder) = arena.get_object(page).and_then(|o| o.as_dict_handle()) else {
            continue;
        };
        let Some(mut dict) = arena.get_dict(holder) else { continue };
        dict.insert(key, Object::Dictionary(arena.alloc_dict(BTreeMap::new())));
        arena.set_dict(holder, dict);
        given += 1;
    }
    if given > 0 {
        doc.decisions.push(crate::interpretation::Decision::repaired(
            "7.7.3.3",
            format!("{given} pages have no /Resources dictionary, which Table 31 requires"),
            "gave each an empty one, which is what a page with none draws with",
        ));
    }
}

/// Takes every `/ProcSet` out, and says how many went.
///
/// **14.2 deprecates procedure sets since PDF 1.4**: they name PostScript procedures a
/// printer was sent, and nothing in this engine reads one. A save to 2.0 kept them while
/// it moved the `/Info` entries 14.3.3 deprecates — 1,086 arrays in `fy05.pdf`'s output
/// — and the Arlington model reads each as a deprecated key (ROADMAP Y-F25). One
/// `Decision` a document, with the count, rather than one a resource dictionary.
pub fn drop_procsets(arena: &PdfArena, decisions: &mut crate::interpretation::DecisionLog) {
    let Some(key) = arena.get_name_by_str("ProcSet") else { return };
    let mut dropped = 0usize;
    for holder in arena.all_dict_handles() {
        if arena.dict_entry(holder, key).is_none() {
            continue;
        }
        let Some(mut dict) = arena.get_dict(holder) else { continue };
        dict.remove(&key);
        arena.set_dict(holder, dict);
        dropped += 1;
    }
    if dropped > 0 {
        decisions.push(crate::interpretation::Decision::repaired(
            "14.2",
            format!(
                "{dropped} resource dictionaries name procedure sets, deprecated since PDF 1.4"
            ),
            "dropped each /ProcSet; nothing reads one but a PostScript printer",
        ));
    }
}

/// Finds and loads every font the document's pages reference.
pub fn discover_fonts(
    arena: &PdfArena,
    doc: &Document,
    font_objects: Option<&[u32]>,
) -> BTreeMap<u32, Arc<FontResource>> {
    let mut cache = BTreeMap::new();
    let type_key = arena.name("Type");
    let font_val = arena.name("Font");
    let base_font_key = arena.name("BaseFont");
    let subtype_key = arena.name("Subtype");

    if let Some(indices) = font_objects {
        for &i in indices {
            process_font_object(
                arena,
                doc,
                i,
                type_key,
                font_val,
                base_font_key,
                subtype_key,
                &mut cache,
            );
        }
    } else {
        for i in 0..arena.object_count() {
            process_font_object(
                arena,
                doc,
                i,
                type_key,
                font_val,
                base_font_key,
                subtype_key,
                &mut cache,
            );
        }
    }
    cache
}

pub(crate) fn accumulate_resources(
    arena: &PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
    is_form: bool,
    resources_key: &Handle<PdfName>,
) -> Vec<BTreeMap<Handle<PdfName>, Object>> {
    let mut current_node = Some(dict.clone());
    let mut resource_nodes = Vec::new();
    // The page tree nodes already climbed through. A `/Parent` chain that returns to one
    // of them is a cycle (7.7.3.2 makes the tree a tree, but a file can say otherwise),
    // and climbing it never ended while the document opened (ROADMAP Z-1).
    let mut seen = std::collections::BTreeSet::new();

    while let Some(node) = current_node {
        if let Some(res_obj) = node.get(resources_key)
            && let Some(res_dict_h) = res_obj.resolve(arena).as_dict_handle()
            && let Some(res_dict) = arena.get_dict(res_dict_h)
        {
            resource_nodes.push(res_dict);
        }

        if is_form {
            break;
        }

        let parent_key = arena.name("Parent");
        if let Some(parent_ref) = node.get(&parent_key) {
            let resolved_parent = parent_ref.resolve(arena);
            if let Object::Dictionary(parent_dict_h) = resolved_parent
                && seen.insert(parent_dict_h.index())
            {
                current_node = arena.get_dict(parent_dict_h);
            } else {
                current_node = None;
            }
        } else {
            current_node = None;
        }
    }
    resource_nodes
}

fn extract_context_fonts(
    arena: &PdfArena,
    mut resource_nodes: Vec<BTreeMap<Handle<PdfName>, Object>>,
    font_key: &Handle<PdfName>,
    fonts: &BTreeMap<u32, Arc<FontResource>>,
) -> BTreeMap<String, Arc<FontResource>> {
    let mut context_fonts = BTreeMap::new();
    resource_nodes.reverse(); // Parents first
    for res_dict in resource_nodes {
        if let Some(f_obj) = res_dict.get(font_key)
            && let Some(f_dict_h) = f_obj.resolve(arena).as_dict_handle()
            && let Some(f_dict) = arena.get_dict(f_dict_h)
        {
            for (res_name_h, font_obj) in f_dict {
                if let Some(res_name) = arena.get_name(res_name_h)
                    && let Some(font_obj_h) = font_obj.as_reference()
                    && let Some(font_res) = fonts.get(&font_obj_h.index())
                {
                    context_fonts.insert(res_name.as_str().to_string(), font_res.clone());
                }
            }
        }
    }
    context_fonts
}

#[allow(clippy::too_many_arguments)]
fn process_stream_context(
    arena: &PdfArena,
    fonts: &BTreeMap<u32, Arc<FontResource>>,
    i: u32,
    type_key: Handle<PdfName>,
    page_val: Handle<PdfName>,
    subtype_key: Handle<PdfName>,
    form_val: Handle<PdfName>,
    resources_key: Handle<PdfName>,
    font_key: Handle<PdfName>,
    contents_key: Handle<PdfName>,
    contexts: &mut BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
) {
    let obj_h = Handle::new(i);
    if let Some(Object::Dictionary(handle) | Object::Stream(handle, _)) = arena.get_object(obj_h)
        && let Some(dict) = arena.get_dict(handle)
    {
        let is_page =
            dict.get(&type_key).and_then(|o| o.resolve(arena).as_name()) == Some(page_val);
        let is_form =
            dict.get(&subtype_key).and_then(|o| o.resolve(arena).as_name()) == Some(form_val);

        if is_page || is_form {
            let resource_nodes = accumulate_resources(arena, &dict, is_form, &resources_key);
            let context_fonts = extract_context_fonts(arena, resource_nodes, &font_key, fonts);

            if is_page {
                associate_page_streams(arena, &dict, &contents_key, context_fonts, contexts);
            } else {
                contexts.insert(obj_h.index(), context_fonts);
            }
        }
    }
}

/// Records which resource dictionary each content stream is interpreted under.
pub fn map_stream_contexts(
    arena: &PdfArena,
    fonts: &BTreeMap<u32, Arc<FontResource>>,
    page_and_form_objects: Option<&[u32]>,
) -> BTreeMap<u32, BTreeMap<String, Arc<FontResource>>> {
    let mut contexts = BTreeMap::new();
    let type_key = arena.name("Type");
    let page_val = arena.name("Page");
    let subtype_key = arena.name("Subtype");
    let form_val = arena.name("Form");
    let resources_key = arena.name("Resources");
    let font_key = arena.name("Font");
    let contents_key = arena.name("Contents");

    if let Some(indices) = page_and_form_objects {
        for &i in indices {
            process_stream_context(
                arena,
                fonts,
                i,
                type_key,
                page_val,
                subtype_key,
                form_val,
                resources_key,
                font_key,
                contents_key,
                &mut contexts,
            );
        }
    } else {
        for i in 0..arena.object_count() {
            process_stream_context(
                arena,
                fonts,
                i,
                type_key,
                page_val,
                subtype_key,
                form_val,
                resources_key,
                font_key,
                contents_key,
                &mut contexts,
            );
        }
    }
    contexts
}

fn associate_page_streams(
    arena: &PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
    contents_key: &Handle<PdfName>,
    context_fonts: BTreeMap<String, Arc<FontResource>>,
    contexts: &mut BTreeMap<u32, BTreeMap<String, Arc<FontResource>>>,
) {
    if let Some(contents) = dict.get(contents_key) {
        match contents {
            Object::Reference(h) => {
                contexts.insert(h.index(), context_fonts);
            }
            Object::Array(ah) => {
                if let Some(arr) = arena.get_array(*ah) {
                    for item in arr {
                        if let Object::Reference(h) = item {
                            contexts.insert(h.index(), context_fonts.clone());
                        }
                    }
                }
            }
            Object::Stream(_, _) => {
                // This shouldn't happen for Page Contents (usually references),
                // but if it's a direct stream, we'd need its object handle.
            }
            _ => {}
        }
    }
}
