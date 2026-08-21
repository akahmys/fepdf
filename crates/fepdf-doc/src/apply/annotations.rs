#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]

use crate::operation::{
    AnnotationKind, AnnotationSpec, DecorationPosition, FormFieldSpec, FormValue, GeoSpatialAnchor,
    MeasurementScale, MeshShadingSpec, MeshShadingType, PageSelection, PdfAction, TransitionSpec,
    TransitionStyle,
};
use bytes::Bytes;
use fepdf_model::arena::PdfArena;
use fepdf_model::object::{PdfName, SublimatedData};
use fepdf_model::{Document, Handle, Object, PdfError, PdfResult};
use std::collections::BTreeMap;
use std::sync::Arc;

fn create_transition_dict(
    arena: &PdfArena,
    spec: &TransitionSpec,
) -> Handle<BTreeMap<Handle<PdfName>, Object>> {
    let style_name = match spec.style {
        TransitionStyle::Split => "Split",
        TransitionStyle::Blinds => "Blinds",
        TransitionStyle::Box => "Box",
        TransitionStyle::Wipe => "Wipe",
        TransitionStyle::Dissolve => "Dissolve",
        TransitionStyle::Glitter => "Glitter",
        TransitionStyle::Fly => "Fly",
    };
    let mut trans_dict = BTreeMap::new();
    trans_dict.insert(arena.name("Type"), Object::Name(arena.name("Trans")));
    trans_dict.insert(arena.name("S"), Object::Name(arena.name(style_name)));
    trans_dict.insert(arena.name("D"), Object::Real(f64::from(spec.duration_seconds)));
    arena.alloc_dict(trans_dict)
}

fn create_action_dict(
    arena: &PdfArena,
    action: &PdfAction,
) -> Handle<BTreeMap<Handle<PdfName>, Object>> {
    let mut dict = BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Action")));

    match action {
        PdfAction::GoToRemote { file_path, page } => {
            dict.insert(arena.name("S"), Object::Name(arena.name("GoToR")));
            dict.insert(arena.name("F"), Object::String(Bytes::from(file_path.clone())));
            let dest_items = vec![Object::Integer(*page as i64), Object::Name(arena.name("Fit"))];
            let dest_ah = arena.alloc_array(dest_items);
            dict.insert(arena.name("D"), Object::Array(dest_ah));
        }
        PdfAction::GoToEmbedded { embedded_name, page } => {
            dict.insert(arena.name("S"), Object::Name(arena.name("GoToE")));
            let mut target_dict = BTreeMap::new();
            target_dict.insert(arena.name("R"), Object::Name(arena.name("C")));
            target_dict.insert(arena.name("N"), Object::String(Bytes::from(embedded_name.clone())));
            let target_dh = arena.alloc_dict(target_dict);
            dict.insert(arena.name("T"), Object::Dictionary(target_dh));
            let dest_items = vec![Object::Integer(*page as i64), Object::Name(arena.name("Fit"))];
            let dest_ah = arena.alloc_array(dest_items);
            dict.insert(arena.name("D"), Object::Array(dest_ah));
        }
        PdfAction::Named(name) => {
            dict.insert(arena.name("S"), Object::Name(arena.name("Named")));
            dict.insert(arena.name("N"), Object::Name(arena.name(name)));
        }
        PdfAction::Transition(spec) => {
            dict.insert(arena.name("S"), Object::Name(arena.name("Trans")));
            let trans_dh = create_transition_dict(arena, spec);
            dict.insert(arena.name("Trans"), Object::Dictionary(trans_dh));
        }
    }
    arena.alloc_dict(dict)
}

/// Sets the document OpenAction in the catalogue (Clause 12.6.2).
pub fn apply_execute_action(doc: &Document, action: PdfAction) -> PdfResult<()> {
    let arena = doc.arena();
    let action_dh = create_action_dict(arena, &action);
    let action_h = arena.alloc_object(Object::Dictionary(action_dh));

    if let Some(cah) = doc.catalog_handle() {
        let cadh = doc.resolve_to_dict(cah)?;
        let mut cdict = arena.get_dict(cadh).unwrap_or_default();
        cdict.insert(arena.name("OpenAction"), Object::Reference(action_h));
        arena.set_dict(cadh, cdict);
    }
    Ok(())
}

/// Attaches geospatial coordinate anchoring to a page (Clause 12.5.6.22).
pub fn apply_set_geospatial_anchor(doc: &Document, anchor: GeoSpatialAnchor) -> PdfResult<()> {
    let arena = doc.arena();
    let Some(page_h) = doc.get_page_handle(anchor.page) else {
        return Err(PdfError::Other("Page index out of bounds".into()));
    };

    let mut measure_dict = BTreeMap::new();
    measure_dict.insert(arena.name("Type"), Object::Name(arena.name("Measure")));
    measure_dict.insert(arena.name("Subtype"), Object::Name(arena.name("GEO")));

    let mut gcs_dict = BTreeMap::new();
    gcs_dict.insert(arena.name("Type"), Object::Name(arena.name("GEOGCS")));
    gcs_dict.insert(arena.name("WKT"), Object::String(Bytes::from(anchor.crs_wkt)));
    let gcs_dh = arena.alloc_dict(gcs_dict);
    measure_dict.insert(arena.name("GCS"), Object::Dictionary(gcs_dh));

    let gpts_items = vec![Object::Real(anchor.latitude), Object::Real(anchor.longitude)];
    let gpts_ah = arena.alloc_array(gpts_items);
    measure_dict.insert(arena.name("GPTS"), Object::Array(gpts_ah));

    let measure_dh = arena.alloc_dict(measure_dict);
    let measure_h = arena.alloc_object(Object::Dictionary(measure_dh));

    let mut vp_dict = BTreeMap::new();
    vp_dict.insert(arena.name("Type"), Object::Name(arena.name("Viewport")));
    vp_dict.insert(arena.name("Name"), Object::String(Bytes::from("GeoSpatial")));
    vp_dict.insert(arena.name("Measure"), Object::Reference(measure_h));
    let vp_dh = arena.alloc_dict(vp_dict);
    let vp_h = arena.alloc_object(Object::Dictionary(vp_dh));

    let page_dh = doc.resolve_to_dict(page_h)?;
    let mut page_dict = arena.get_dict(page_dh).unwrap_or_default();
    let vp_arr_h = arena.alloc_array(vec![Object::Reference(vp_h)]);
    page_dict.insert(arena.name("VP"), Object::Array(vp_arr_h));
    arena.set_dict(page_dh, page_dict);

    Ok(())
}

fn ensure_catalog_shading_dict(
    arena: &PdfArena,
    cdict: &mut BTreeMap<Handle<PdfName>, Object>,
) -> Handle<BTreeMap<Handle<PdfName>, Object>> {
    let res_key = arena.name("Resources");
    let shading_key = arena.name("Shading");

    let res_dh = if let Some(res_obj) = cdict.get(&res_key)
        && let Some(dh) = res_obj.as_dict_handle()
    {
        dh
    } else {
        let dh = arena.alloc_dict(BTreeMap::new());
        cdict.insert(res_key, Object::Dictionary(dh));
        dh
    };

    let mut res_dict = arena.get_dict(res_dh).unwrap_or_default();
    let sh_dh = if let Some(sh_obj) = res_dict.get(&shading_key)
        && let Some(dh) = sh_obj.as_dict_handle()
    {
        dh
    } else {
        let dh = arena.alloc_dict(BTreeMap::new());
        res_dict.insert(shading_key, Object::Dictionary(dh));
        dh
    };
    arena.set_dict(res_dh, res_dict);
    sh_dh
}

/// Registers mesh shading geometry in the catalogue resources (Clause 8.7.4.5).
pub fn apply_add_mesh_shading(doc: &Document, shading: MeshShadingSpec) -> PdfResult<()> {
    let arena = doc.arena();
    let shading_type_num = match shading.shading_type {
        MeshShadingType::FreeFormTriangleMesh => 4,
        MeshShadingType::LatticeFormTriangleMesh => 5,
        MeshShadingType::CoonsPatchMesh => 6,
        MeshShadingType::TensorProductPatchMesh => 7,
    };

    let mut stream_dict = BTreeMap::new();
    stream_dict.insert(arena.name("Type"), Object::Name(arena.name("Shading")));
    stream_dict.insert(arena.name("ShadingType"), Object::Integer(shading_type_num));
    stream_dict.insert(arena.name("ColorSpace"), Object::Name(arena.name(&shading.color_space)));

    let stream_dh = arena.alloc_dict(stream_dict);
    let stream_obj =
        Object::Stream(stream_dh, Arc::new(SublimatedData::Raw(Bytes::from(shading.data_bytes))));
    let shading_h = arena.alloc_object(stream_obj);

    if let Some(cah) = doc.catalog_handle() {
        let cadh = doc.resolve_to_dict(cah)?;
        let mut cdict = arena.get_dict(cadh).unwrap_or_default();
        let sh_dh = ensure_catalog_shading_dict(arena, &mut cdict);
        let mut sh_dict = arena.get_dict(sh_dh).unwrap_or_default();
        let sh_name = arena.name("Sh0");
        sh_dict.insert(sh_name, Object::Reference(shading_h));
        arena.set_dict(sh_dh, sh_dict);
        arena.set_dict(cadh, cdict);
    }
    Ok(())
}

fn calculate_decoration_coords(
    rect: &fepdf_model::graphics::Rect,
    pos: &DecorationPosition,
) -> (f64, f64) {
    match pos {
        DecorationPosition::TopLeft => (rect.x1 + 36.0, rect.y2 - 36.0),
        DecorationPosition::TopCenter => (rect.x1.midpoint(rect.x2) - 50.0, rect.y2 - 36.0),
        DecorationPosition::TopRight => (rect.x2 - 120.0, rect.y2 - 36.0),
        DecorationPosition::BottomLeft => (rect.x1 + 36.0, rect.y1 + 36.0),
        DecorationPosition::BottomCenter => (rect.x1.midpoint(rect.x2) - 50.0, rect.y1 + 36.0),
        DecorationPosition::BottomRight => (rect.x2 - 120.0, rect.y1 + 36.0),
    }
}

/// The resource dictionary a page's content is interpreted under (7.7.3.4).
///
/// Both callers below used to reach for `/Resources` on the page dictionary alone and
/// build a fresh empty one when it was not there — which is wrong twice over. A page
/// whose `/Resources` is an indirect reference had it **replaced** by the empty
/// dictionary, and a page that *inherits* one from the page tree had it **shadowed**, so
/// the fonts and XObjects its own content stream names stopped resolving. Adding a
/// decoration is not supposed to be able to blank a page.
fn ensure_page_resources(
    doc: &Document,
    page_h: Handle<Object>,
    page_dict: &mut BTreeMap<Handle<PdfName>, Object>,
) -> Handle<BTreeMap<Handle<PdfName>, Object>> {
    let arena = doc.arena();
    let key = arena.name("Resources");
    if let Some(dh) = page_dict.get(&key).and_then(|entry| entry.resolve(arena).as_dict_handle()) {
        return dh;
    }
    // `Page::resources_handle` walks the parent chain, and returns a fresh dictionary
    // only when nothing in the tree carries one. Naming it on the page settles the
    // inheritance into the one state a document is (ADR-0013) rather than shadowing it.
    let chain = doc.get_parent_chain(page_h);
    let inherited = fepdf_model::Page::new(arena, page_h, chain).resources_handle();
    page_dict.insert(key, Object::Dictionary(inherited));
    inherited
}

fn ensure_helvetica_in_page_dict(
    doc: &Document,
    page_h: Handle<Object>,
    page_dict: &mut BTreeMap<Handle<PdfName>, Object>,
) {
    let arena = doc.arena();
    let font_key = arena.name("Font");
    let helv_key = arena.name("Helvetica");
    let res_dh = ensure_page_resources(doc, page_h, page_dict);

    let mut res_dict = arena.get_dict(res_dh).unwrap_or_default();
    let font_dh = if let Some(font_obj) = res_dict.get(&font_key)
        && let Some(dh) = font_obj.as_dict_handle()
    {
        dh
    } else {
        let dh = arena.alloc_dict(BTreeMap::new());
        res_dict.insert(font_key, Object::Dictionary(dh));
        dh
    };

    let mut font_dict = arena.get_dict(font_dh).unwrap_or_default();
    font_dict.entry(helv_key).or_insert_with(|| {
        let mut helv_dict = BTreeMap::new();
        helv_dict.insert(arena.name("Type"), Object::Name(arena.name("Font")));
        helv_dict.insert(arena.name("Subtype"), Object::Name(arena.name("Type1")));
        helv_dict.insert(arena.name("BaseFont"), Object::Name(arena.name("Helvetica")));
        let helv_dh = arena.alloc_dict(helv_dict);
        Object::Dictionary(helv_dh)
    });
    arena.set_dict(font_dh, font_dict);
    arena.set_dict(res_dh, res_dict);
}

fn overlay_text_on_page(
    doc: &Document,
    page_h: Handle<Object>,
    text: &str,
    position: &DecorationPosition,
    layer: Option<Handle<Object>>,
) -> PdfResult<()> {
    let arena = doc.arena();
    let page_dh = doc.resolve_to_dict(page_h)?;
    let mut page_dict = arena.get_dict(page_dh).unwrap_or_default();

    let parent_chain = doc.get_parent_chain(page_h);
    let page_view = fepdf_model::Page::new(arena, page_h, parent_chain);
    let mbox = page_view.media_box();
    let (x, y) = calculate_decoration_coords(&mbox, position);

    let escaped_text = text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let drawing =
        format!("q\nBT\n/Helvetica 10 Tf\n1 0 0 1 {x:.2} {y:.2} Tm\n({escaped_text}) Tj\nET\nQ\n");
    let stream_content = match layer {
        Some(group) => {
            let tag = name_layer_in_page(doc, page_h, &mut page_dict, group);
            format!("/OC /{tag} BDC\n{drawing}EMC\n")
        }
        None => drawing,
    };

    let stream_dict = arena.alloc_dict(BTreeMap::new());
    let stream_obj = Object::Stream(
        stream_dict,
        Arc::new(SublimatedData::Raw(Bytes::from(stream_content.into_bytes()))),
    );
    let stream_h = arena.alloc_object(stream_obj);

    ensure_helvetica_in_page_dict(doc, page_h, &mut page_dict);

    let contents_key = arena.name("Contents");
    let mut contents_items = if let Some(existing_contents) = page_dict.get(&contents_key) {
        match existing_contents {
            Object::Array(ah) => arena.get_array(*ah).unwrap_or_default(),
            Object::Reference(h) => vec![Object::Reference(*h)],
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };
    contents_items.push(Object::Reference(stream_h));
    let contents_ah = arena.alloc_array(contents_items);
    page_dict.insert(contents_key, Object::Array(contents_ah));
    arena.set_dict(page_dh, page_dict);

    Ok(())
}

/// Gives a page's `/Properties` a name for `group`, and returns it (8.11.3.1).
///
/// A `/OC BDC` names its group through the page's resources, not by writing the
/// reference inline: 8.11.2 requires a group to be an indirect object, and an inline
/// dictionary would name nothing `/OCProperties` could turn off. The name is chosen past
/// whatever the page already carries, so a second decoration does not take the first
/// one's slot.
fn name_layer_in_page(
    doc: &Document,
    page_h: Handle<Object>,
    page_dict: &mut BTreeMap<Handle<PdfName>, Object>,
    group: Handle<Object>,
) -> String {
    let arena = doc.arena();
    let res_dh = ensure_page_resources(doc, page_h, page_dict);
    let mut resources = arena.get_dict(res_dh).unwrap_or_default();
    let properties_key = arena.name("Properties");
    let properties_dh = resources
        .get(&properties_key)
        .and_then(|entry| entry.resolve(arena).as_dict_handle())
        .unwrap_or_else(|| {
            let fresh = arena.alloc_dict(BTreeMap::new());
            resources.insert(properties_key, Object::Dictionary(fresh));
            fresh
        });
    arena.set_dict(res_dh, resources);

    let mut properties = arena.get_dict(properties_dh).unwrap_or_default();
    // A page that already names this group keeps the name it gave it. Decorating every
    // page of a document puts the same group in one shared resource dictionary when the
    // tree carries one, and a fresh entry per page would grow it without saying anything
    // new.
    let reference = Object::Reference(group);
    if let Some(existing) = properties.iter().find(|(_, value)| **value == reference)
        && let Some(name) = arena.get_name(*existing.0)
    {
        return name.as_str().to_string();
    }
    let tag = format!("fepdfOC{}", properties.len());
    properties.insert(arena.name(&tag), reference);
    arena.set_dict(properties_dh, properties);
    tag
}

/// Overlays header/footer text decorations onto pages.
///
/// # Errors
/// Fails when `layer` names an optional content group the document does not declare.
/// Drawing it unconditionally instead would put the decoration on every page of a
/// document whose author asked for a layer they could switch off.
pub fn apply_add_page_decoration(
    doc: &Document,
    pages: &PageSelection,
    text: &str,
    position: &DecorationPosition,
    layer: Option<&str>,
) -> PdfResult<()> {
    let group = match layer {
        Some(name) => {
            Some(fepdf_model::optional_content::group_named(doc, name)?.ok_or_else(|| {
                PdfError::Other(
                    format!("no optional content group is named {name:?}; add it first").into(),
                )
            })?)
        }
        None => None,
    };
    let count = doc.page_count()?;
    let indices = match pages {
        PageSelection::All => (0..count).collect(),
        PageSelection::Single(i) => vec![*i],
        PageSelection::Indices(idx) => idx.clone(),
    };
    for idx in indices {
        if idx < count
            && let Some(page_h) = doc.get_page_handle(idx)
        {
            overlay_text_on_page(doc, page_h, text, position, group)?;
        }
    }
    Ok(())
}

/// Overlays Bates numbering sequences across selected pages.
pub fn apply_bates_numbering(
    doc: &Document,
    pages: &PageSelection,
    prefix: &str,
    start_number: u64,
    digits: usize,
    position: &DecorationPosition,
) -> PdfResult<()> {
    let count = doc.page_count()?;
    let indices = match pages {
        PageSelection::All => (0..count).collect(),
        PageSelection::Single(i) => vec![*i],
        PageSelection::Indices(idx) => idx.clone(),
    };
    for (i, idx) in indices.into_iter().enumerate() {
        if idx < count
            && let Some(page_h) = doc.get_page_handle(idx)
        {
            let num = start_number + i as u64;
            let bates_text = format!("{prefix}{num:0digits$}");
            overlay_text_on_page(doc, page_h, &bates_text, position, None)?;
        }
    }
    Ok(())
}

fn populate_annotation_kind(
    arena: &PdfArena,
    dict: &mut BTreeMap<Handle<PdfName>, Object>,
    kind: &AnnotationKind,
    get_page_handle: impl Fn(usize) -> Option<Handle<Object>>,
) {
    match kind {
        AnnotationKind::Link { destination_page, url } => {
            dict.insert(arena.name("Subtype"), Object::Name(arena.name("Link")));
            if let Some(uri) = url {
                let mut action_dict = BTreeMap::new();
                action_dict.insert(arena.name("Type"), Object::Name(arena.name("Action")));
                action_dict.insert(arena.name("S"), Object::Name(arena.name("URI")));
                action_dict.insert(arena.name("URI"), Object::String(Bytes::from(uri.clone())));
                let action_dh = arena.alloc_dict(action_dict);
                dict.insert(arena.name("A"), Object::Dictionary(action_dh));
            } else if let Some(target_page_h) = get_page_handle(*destination_page) {
                let dest_items =
                    vec![Object::Reference(target_page_h), Object::Name(arena.name("Fit"))];
                let dest_ah = arena.alloc_array(dest_items);
                dict.insert(arena.name("Dest"), Object::Array(dest_ah));
            }
        }
        AnnotationKind::Highlight { color_rgb } => {
            dict.insert(arena.name("Subtype"), Object::Name(arena.name("Highlight")));
            let c_items = vec![
                Object::Real(f64::from(color_rgb[0])),
                Object::Real(f64::from(color_rgb[1])),
                Object::Real(f64::from(color_rgb[2])),
            ];
            let c_ah = arena.alloc_array(c_items);
            dict.insert(arena.name("C"), Object::Array(c_ah));
        }
        AnnotationKind::TextComment { contents } => {
            dict.insert(arena.name("Subtype"), Object::Name(arena.name("Text")));
            dict.insert(arena.name("Contents"), Object::String(Bytes::from(contents.clone())));
        }
        AnnotationKind::Stamp { stamp_image_bytes: _ } => {
            dict.insert(arena.name("Subtype"), Object::Name(arena.name("Stamp")));
            dict.insert(arena.name("Name"), Object::Name(arena.name("Draft")));
        }
    }
}

fn create_annotation_dict(
    arena: &PdfArena,
    annot: &AnnotationSpec,
    get_page_handle: impl Fn(usize) -> Option<Handle<Object>>,
) -> Handle<BTreeMap<Handle<PdfName>, Object>> {
    let mut dict = BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Annot")));
    let rect_items = vec![
        Object::Real(f64::from(annot.rect[0])),
        Object::Real(f64::from(annot.rect[1])),
        Object::Real(f64::from(annot.rect[2])),
        Object::Real(f64::from(annot.rect[3])),
    ];
    let rect_ah = arena.alloc_array(rect_items);
    dict.insert(arena.name("Rect"), Object::Array(rect_ah));

    populate_annotation_kind(arena, &mut dict, &annot.kind, get_page_handle);
    arena.alloc_dict(dict)
}

/// Appends an annotation to a target page (Clause 12.5).
pub fn apply_add_annotation(doc: &Document, annot: AnnotationSpec) -> PdfResult<()> {
    let arena = doc.arena();
    let Some(page_h) = doc.get_page_handle(annot.page) else {
        return Err(PdfError::Other("Page index out of bounds".into()));
    };

    let annot_dh = create_annotation_dict(arena, &annot, |idx| doc.get_page_handle(idx));
    let annot_h = arena.alloc_object(Object::Dictionary(annot_dh));

    let page_dh = doc.resolve_to_dict(page_h)?;
    let mut page_dict = arena.get_dict(page_dh).unwrap_or_default();
    let annots_key = arena.name("Annots");
    let mut annots_items = if let Some(existing_annots) = page_dict.get(&annots_key) {
        match existing_annots {
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
    annots_items.push(Object::Reference(annot_h));
    let annots_ah = arena.alloc_array(annots_items);
    page_dict.insert(annots_key, Object::Array(annots_ah));
    arena.set_dict(page_dh, page_dict);

    Ok(())
}

/// Sets viewport measurement scale on a page (Clause 12.5.6.21).
pub fn apply_set_measurement_scale(doc: &Document, scale: MeasurementScale) -> PdfResult<()> {
    let arena = doc.arena();
    let Some(page_h) = doc.get_page_handle(scale.page) else {
        return Err(PdfError::Other("Page index out of bounds".into()));
    };

    let mut measure_dict = BTreeMap::new();
    measure_dict.insert(arena.name("Type"), Object::Name(arena.name("Measure")));
    measure_dict.insert(arena.name("Subtype"), Object::Name(arena.name("RL")));
    measure_dict.insert(arena.name("R"), Object::String(Bytes::from(scale.unit_label)));
    let x_items = vec![Object::Real(f64::from(scale.scale_ratio))];
    let x_ah = arena.alloc_array(x_items);
    measure_dict.insert(arena.name("X"), Object::Array(x_ah));

    let measure_dh = arena.alloc_dict(measure_dict);
    let measure_h = arena.alloc_object(Object::Dictionary(measure_dh));

    let page_dh = doc.resolve_to_dict(page_h)?;
    let mut page_dict = arena.get_dict(page_dh).unwrap_or_default();
    page_dict.insert(arena.name("Measure"), Object::Reference(measure_h));
    arena.set_dict(page_dh, page_dict);

    Ok(())
}

fn update_form_field_value_in_dict(
    arena: &PdfArena,
    field_dh: Handle<BTreeMap<Handle<PdfName>, Object>>,
    target_name: &str,
    new_value: &FormValue,
) -> bool {
    let Some(mut dict) = arena.get_dict(field_dh) else { return false };
    let t_key = arena.name("T");
    let name_matches = dict.get(&t_key).and_then(|obj| match obj {
        Object::String(b) => std::str::from_utf8(b).ok(),
        Object::Text(s) => Some(s.as_str()),
        _ => None,
    }) == Some(target_name);

    if name_matches {
        let v_key = arena.name("V");
        let v_obj = match new_value {
            FormValue::Text(s) => Object::String(Bytes::from(s.clone())),
            FormValue::Choice(c) => Object::Name(arena.name(c)),
            FormValue::Boolean(b) => Object::Name(arena.name(if *b { "Yes" } else { "Off" })),
        };
        dict.insert(v_key, v_obj);
        arena.set_dict(field_dh, dict);
        return true;
    }

    if let Some(kids_obj) = dict.get(&arena.name("Kids")) {
        let kids = match kids_obj {
            Object::Array(ah) => arena.get_array(*ah).unwrap_or_default(),
            Object::Reference(h) => {
                if let Some(Object::Array(ah)) = arena.get_object(*h) {
                    arena.get_array(ah).unwrap_or_default()
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        };
        for kid in kids {
            if let Some(kh) = kid.as_reference()
                && let Some(Object::Dictionary(kdh)) = arena.get_object(kh)
                && update_form_field_value_in_dict(arena, kdh, target_name, new_value)
            {
                return true;
            }
        }
    }
    false
}

/// Sets the value of an AcroForm field (Clause 12.7.3).
pub fn apply_set_form_field_value(doc: &Document, field: FormFieldSpec) -> PdfResult<()> {
    let arena = doc.arena();
    let Some(cah) = doc.catalog_handle() else { return Ok(()) };
    let cadh = doc.resolve_to_dict(cah)?;
    let cdict = arena.get_dict(cadh).unwrap_or_default();

    let acro_key = arena.name("AcroForm");
    let Some(acro_obj) = cdict.get(&acro_key) else {
        return Ok(());
    };
    let Some(acro_dh) = (match acro_obj {
        Object::Dictionary(dh) => Some(*dh),
        Object::Reference(h) => match arena.get_object(*h) {
            Some(Object::Dictionary(dh)) => Some(dh),
            _ => None,
        },
        _ => None,
    }) else {
        return Ok(());
    };

    let mut acro_dict = arena.get_dict(acro_dh).unwrap_or_default();
    acro_dict.insert(arena.name("NeedAppearances"), Object::Boolean(true));
    arena.set_dict(acro_dh, acro_dict.clone());

    if let Some(fields_obj) = acro_dict.get(&arena.name("Fields")) {
        let fields = match fields_obj {
            Object::Array(ah) => arena.get_array(*ah).unwrap_or_default(),
            Object::Reference(h) => match arena.get_object(*h) {
                Some(Object::Array(ah)) => arena.get_array(ah).unwrap_or_default(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        for f in fields {
            if let Some(fh) = f.as_reference()
                && let Some(Object::Dictionary(fdh)) = arena.get_object(fh)
                && update_form_field_value_in_dict(arena, fdh, &field.name, &field.value)
            {
                break;
            }
        }
    }
    Ok(())
}

/// Alias for `apply_bates_numbering`.
pub use apply_bates_numbering as apply_bates;
