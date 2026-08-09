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

fn accumulate_resources(
    arena: &PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
    is_form: bool,
    resources_key: &Handle<PdfName>,
) -> Vec<BTreeMap<Handle<PdfName>, Object>> {
    let mut current_node = Some(dict.clone());
    let mut resource_nodes = Vec::new();

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
            if let Object::Dictionary(parent_dict_h) = resolved_parent {
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
    if let Some(Object::Dictionary(handle)) | Some(Object::Stream(handle, _)) =
        arena.get_object(obj_h)
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
