#![allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]

use crate::apply::appearance;
use crate::apply::fields::text_of;
use crate::operation::{
    AnnotationSpec, DecorationPosition, FormFieldSpec, FormValue, GeoSpatialAnchor,
    MeasurementScale, MeshShadingSpec, MeshShadingType, PageSelection, PdfAction, TransitionSpec,
    TransitionStyle,
};
use bytes::Bytes;
use fepdf_model::arena::PdfArena;
use fepdf_model::interpretation::Decision;
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
            // **A file specification string (7.11.2), not a text string.** Table 202 gives
            // `/F` a file specification, and written as a bare string that is the
            // slash-separated path form 7.11.2 defines. A Unicode path belongs in a filespec
            // dictionary's `/UF` beside it — which this action does not build, so a non-ASCII
            // remote path is still not expressible here. Making `/F` text would not fix that;
            // it would write a BOM in front of a path instead.
            dict.insert(arena.name("F"), Object::String(Bytes::from(file_path.clone())));
            let dest_items = vec![Object::Integer(*page as i64), Object::Name(arena.name("Fit"))];
            let dest_ah = arena.alloc_array(dest_items);
            dict.insert(arena.name("D"), Object::Array(dest_ah));
        }
        PdfAction::GoToEmbedded { embedded_name, page } => {
            dict.insert(arena.name("S"), Object::Name(arena.name("GoToE")));
            let mut target_dict = BTreeMap::new();
            target_dict.insert(arena.name("R"), Object::Name(arena.name("C")));
            // A byte string (Table 204): `/N` names a file in the `/EmbeddedFiles` name
            // tree, and is compared against that tree's keys, which
            // `apply::metadata::add_embedded_files_to_catalog` writes as raw bytes.
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
pub fn apply_set_open_action(doc: &Document, action: PdfAction) -> PdfResult<()> {
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
    // **Not a text string.** `/WKT` holds an OGC well-known-text coordinate system, whose
    // grammar is ASCII and whose first token a parser expects at byte zero. A BOM in front
    // of `GEOGCS[...]` makes it unparseable by every consumer of it.
    gcs_dict.insert(arena.name("WKT"), Object::String(Bytes::from(anchor.crs_wkt)));
    let gcs_dh = arena.alloc_dict(gcs_dict);
    measure_dict.insert(arena.name("GCS"), Object::Dictionary(gcs_dh));

    let gpts_items = vec![Object::Real(anchor.latitude), Object::Real(anchor.longitude)];
    let gpts_ah = arena.alloc_array(gpts_items);
    measure_dict.insert(arena.name("GPTS"), Object::Array(gpts_ah));

    let measure_dh = arena.alloc_dict(measure_dict);
    let measure_h = arena.alloc_object(Object::Dictionary(measure_dh));

    // Through the viewport writer the scale uses: the viewport had no `/BBox`, which
    // Table 265 requires, and replaced the whole `/VP` array, scale and all.
    let media = fepdf_model::Page::new(arena, page_h, doc.get_parent_chain(page_h)).media_box();
    crate::measure::set_viewport(
        doc,
        anchor.page,
        [media.x1, media.y1, media.x2, media.y2],
        "GeoSpatial",
        measure_h,
    )
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
pub(crate) fn ensure_page_resources(
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

/// Draws `text` on `page_h`, in a face this machine permits embedding.
///
/// **It used to write `/Helvetica 10 Tf` and a literal string.** Neither half held: the
/// font was named in the page's resources and never embedded, and a Rust `String` escaped
/// into a literal becomes one character code per *byte*, so 図面 went in as six codes of a
/// WinAnsi font and came back as six Latin glyphs and no extracted text. What replaces it
/// is the ladder — a face installed here whose own terms permit it — and glyph codes
/// through an embedded subset
/// ([ADR-0090](../../../../docs/adr/0090-the-face-a-document-embeds-is-not-a-licence-to-set-new-text.md)).
///
/// **This can fail where it used to succeed**, and that is the decision rather than a
/// regression: a decoration that cannot be set in a permitted face is refused, naming the
/// character or the faces that refused, instead of being drawn as something else.
fn overlay_text_on_page(
    doc: &Document,
    page_h: Handle<Object>,
    face: &(String, std::sync::Arc<Vec<u8>>),
    embedded: &crate::apply::font::Embedded,
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

    let shown = crate::apply::font::ShownText {
        program: &face.1,
        base_font: &face.0,
        text,
        at: (x, y),
        size: 10.0,
    };
    let drawing = crate::apply::font::draw_with(doc, page_h, &mut page_dict, embedded, &shown)?;

    let stream_content = match layer {
        Some(group) => {
            let tag = name_layer_in_page(doc, page_h, &mut page_dict, group);
            format!("/OC /{tag} BDC\n{drawing}EMC\n")
        }
        None => drawing,
    };
    crate::apply::font::append_content(doc, page_dh, &mut page_dict, stream_content.into_bytes());
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
    let indices = crate::apply::page::pages_named(pages, count)?;
    // One face, embedded once, shown on every page it is asked for.
    let face = crate::apply::font::face_for(text)
        .map_err(|why| PdfError::Other(format!("{text:?} cannot be set: {why}").into()))?;
    let embedded = crate::apply::font::embed_for(doc, &face.1, &face.0, &[text])?;

    for idx in indices {
        if let Some(page_h) = doc.get_page_handle(idx) {
            overlay_text_on_page(doc, page_h, &face, &embedded, text, position, group)?;
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
    let indices = crate::apply::page::pages_named(pages, count)?;
    // **Every label is known before the first page is touched**, so the face carries the
    // glyphs of all of them and is embedded once. Embedding per page put a subset of the
    // same face on each: thirteen footers took `samples/constitution.pdf` from 244,790
    // bytes to 830,167.
    let labels: Vec<String> = indices
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let num = start_number + i as u64;
            format!("{prefix}{num:0digits$}")
        })
        .collect();
    let every: Vec<&str> = labels.iter().map(String::as_str).collect();
    let together = labels.join("");
    let face = crate::apply::font::face_for(&together)
        .map_err(|why| PdfError::Other(format!("{prefix:?} cannot be set: {why}").into()))?;
    let embedded = crate::apply::font::embed_for(doc, &face.1, &face.0, &every)?;

    for (i, idx) in indices.into_iter().enumerate() {
        if let Some(page_h) = doc.get_page_handle(idx)
            && let Some(label) = labels.get(i)
        {
            overlay_text_on_page(doc, page_h, &face, &embedded, label, position, None)?;
        }
    }
    Ok(())
}

/// Appends an annotation to a target page (Clause 12.5).
pub fn apply_add_annotation(doc: &Document, annot: AnnotationSpec) -> PdfResult<()> {
    let arena = doc.arena();
    let Some(page_h) = doc.get_page_handle(annot.page) else {
        return Err(PdfError::Other("Page index out of bounds".into()));
    };

    let annot_dh = crate::apply::markup::annotation(doc, &annot, page_h)?;
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

/// Declares the page's scale: one point is `scale_ratio` `unit_label`s (12.9).
///
/// **It was written where no reader looks and in a shape none would read.** `/Measure`
/// went straight into the page dictionary, which Table 31 does not give one — a measure
/// belongs to a viewport in `/VP` (Table 265); `/R` held the unit's label where Table 267
/// wants the ratio stated; and `/X` held a bare number where it wants number format
/// dictionaries, with `/D` and `/A`, which it requires, missing. It is a viewport over
/// the whole page now, in place of any rectilinear one the page had.
///
/// # Errors
/// Fails when the page is not there or the ratio is not a positive number.
pub fn apply_set_measurement_scale(doc: &Document, scale: MeasurementScale) -> PdfResult<()> {
    let ratio = f64::from(scale.scale_ratio);
    if !(ratio > 0.0 && ratio.is_finite()) {
        return Err(PdfError::Other(format!("a scale of {ratio} measures nothing").into()));
    }
    let arena = doc.arena();
    let Some(page_h) = doc.get_page_handle(scale.page) else {
        return Err(PdfError::Other(format!("there is no page {}", scale.page + 1).into()));
    };
    let media = fepdf_model::Page::new(arena, page_h, doc.get_parent_chain(page_h)).media_box();
    let measure = crate::measure::write_rectilinear(arena, ratio, &scale.unit_label);
    crate::measure::set_viewport(
        doc,
        scale.page,
        [media.x1, media.y1, media.x2, media.y2],
        &scale.unit_label,
        measure,
    )
}

fn apply_value_to_field_dict(
    arena: &PdfArena,
    dict: &mut BTreeMap<Handle<PdfName>, Object>,
    new_value: &FormValue,
    state: Option<Handle<PdfName>>,
) {
    let v_key = arena.name("V");
    let v_obj = match (new_value, state) {
        (FormValue::Boolean(_), Some(state)) => Object::Name(state),
        // A text string, which the writer encodes (7.9.2.2): these bytes were the UTF-8 of
        // the value, which a reader takes for PDFDocEncoding, so 東京 went in as mojibake.
        (FormValue::Text(s) | FormValue::Choice(s), _) => Object::Text(s.clone()),
        (FormValue::Boolean(b), None) => Object::Name(arena.name(if *b { "Yes" } else { "Off" })),
    };
    dict.insert(v_key, v_obj);

    let target_val = match new_value {
        FormValue::Choice(s) | FormValue::Text(s) => Some(s.as_str()),
        FormValue::Boolean(_) => None,
    };
    if let Some(opt_obj) = dict.get(&arena.name("Opt"))
        && let Some(target_val) = target_val
        && let Some(matched_idx) = find_option_index(arena, opt_obj, target_val)
    {
        let i_key = arena.name("I");
        let arr_h = arena.alloc_array(vec![Object::Integer(matched_idx as i64)]);
        dict.insert(i_key, Object::Array(arr_h));
    }
}

fn find_option_index(arena: &PdfArena, opt_obj: &Object, target_val: &str) -> Option<usize> {
    let arr = match opt_obj.resolve(arena) {
        Object::Array(ah) => arena.get_array(ah)?,
        _ => return None,
    };
    // An option is a string, or a pair whose first is the export value (Table 231); either
    // is decoded before it is compared, since a raw comparison missed every UTF-16 one.
    arr.iter().position(|item| {
        let export = match item.resolve(arena) {
            Object::Array(pair) => arena.get_array(pair).and_then(|pair| pair.first().cloned()),
            other => Some(other),
        };
        export.and_then(|export| text_of(arena, &export)).as_deref() == Some(target_val)
    })
}

/// Sets the value of an AcroForm field (Clause 12.7.3), named by its fully qualified
/// name (12.7.4.2) — the name `form_of` reports.
///
/// # Errors
/// Fails when the document has no form, or no field of that name. **Both used to answer
/// `Ok` having done nothing**, and the name was compared as raw UTF-8 bytes against each
/// field's own `/T`: a nested field's qualified name matched nothing, and so did every
/// field of `sample_02c.pdf`, whose names are UTF-16. The window's form drawer wrote into
/// none of them and said nothing.
pub fn apply_set_form_field_value(doc: &Document, field: FormFieldSpec) -> PdfResult<()> {
    let arena = doc.arena();
    let catalog = doc.resolve_to_dict(
        doc.catalog_handle()
            .ok_or_else(|| PdfError::Other("the document has no catalogue".into()))?,
    )?;
    let acro_dh = arena
        .dict_entry(catalog, arena.name("AcroForm"))
        .and_then(|a| a.resolve(arena).as_dict_handle())
        .ok_or_else(|| PdfError::Other("the document has no form to fill".into()))?;
    let Some((_, _, fdh)) = crate::apply::fields::named_fields(arena, acro_dh)
        .into_iter()
        .find(|(name, _, _)| *name == field.name)
    else {
        return Err(PdfError::Other(
            format!("the form has no field named {:?}", field.name).into(),
        ));
    };

    // `/NeedAppearances` is **not** written. 0.3 lists it among the entries PDF 2.0
    // deprecates, and this engine's rule is not to write what 2.0 deprecates (ADR-0015,
    // which applied it to encryption). Setting it was a producer saying "reader, you work
    // it out" with an entry the reader is no longer obliged to honour; the appearance is
    // built here instead, as 12.7.4.3 describes.
    let acro_dict = arena.get_dict(acro_dh).unwrap_or_default();
    report_scripts_not_run(doc, &acro_dict, &field.name);

    let state = match field.value {
        FormValue::Boolean(on) => Some(button_state(doc, fdh, on, &field.name)?),
        FormValue::Text(_) | FormValue::Choice(_) => None,
    };
    let mut dict = arena.get_dict(fdh).unwrap_or_default();
    apply_value_to_field_dict(arena, &mut dict, &field.value, state);
    arena.set_dict(fdh, dict);
    refresh_appearance(doc, fdh, &acro_dict, &field.value, state)
}

/// The state a button is set to: `/Off`, or its on state — which is whatever name its
/// widgets' appearances use, not `/Yes` (12.7.5.2.3).
///
/// **`/Yes` was written whatever the box called its on state.** On `sample_02c.pdf`, whose
/// boxes are each named after themselves, that set `/V /Yes`, found no appearance for it,
/// recorded a violation of 12.7.5.2.3 against a file that had done nothing wrong, and left
/// the box drawn empty.
///
/// # Errors
/// Fails when the widgets have different on states — a set of radio buttons, where
/// "on" does not say which — or none.
fn button_state(
    doc: &Document,
    field_dh: Handle<BTreeMap<Handle<PdfName>, Object>>,
    on: bool,
    name: &str,
) -> PdfResult<Handle<PdfName>> {
    let arena = doc.arena();
    let off = arena.name("Off");
    if !on {
        return Ok(off);
    }
    let mut states: Vec<Handle<PdfName>> = widgets_of(arena, field_dh)
        .into_iter()
        .flat_map(|widget| appearance::button_states(doc, widget))
        .filter(|state| *state != off)
        .collect();
    states.sort();
    states.dedup();
    match states[..] {
        [state] => Ok(state),
        [] => Err(PdfError::Other(
            format!("{name:?} has no appearance for being on, so there is nothing to turn on")
                .into(),
        )),
        _ => Err(PdfError::Other(
            format!(
                "{name:?} is {} buttons with different states; turning it on does not say which",
                states.len()
            )
            .into(),
        )),
    }
}

fn resolve_choice_display(
    arena: &PdfArena,
    field: &BTreeMap<Handle<PdfName>, Object>,
    value_text: &str,
) -> String {
    let Some(opt_obj) = field.get(&arena.name("Opt")) else {
        return value_text.to_string();
    };
    let Object::Array(ah) = opt_obj.resolve(arena) else {
        return value_text.to_string();
    };
    let Some(arr) = arena.get_array(ah) else {
        return value_text.to_string();
    };
    for item in arr {
        if let Object::Array(pair_h) = item.resolve(arena)
            && let Some(pair) = arena.get_array(pair_h)
        {
            let export_matches =
                pair.first().and_then(|e| text_of(arena, e)).as_deref() == Some(value_text);
            if export_matches && let Some(display) = pair.get(1).and_then(|d| text_of(arena, d)) {
                return display;
            }
        }
    }
    value_text.to_string()
}

/// Rebuilds the appearance of the widget a field's value is shown through (12.7.4.3).
///
/// A field and its widget may be one object or two: a single-widget field merges them,
/// and a field with several widgets keeps them in `/Kids`. Both are handled, because a
/// merged field whose appearance was left alone looks exactly like one that has no widget.
fn refresh_appearance(
    doc: &Document,
    field_dh: Handle<BTreeMap<Handle<PdfName>, Object>>,
    acro: &BTreeMap<Handle<PdfName>, Object>,
    value: &FormValue,
    state: Option<Handle<PdfName>>,
) -> PdfResult<()> {
    let arena = doc.arena();
    let field = arena.get_dict(field_dh).unwrap_or_default();
    // `/DA` and `/Q` are inheritable (12.7.4.2); the form's own are the fallback.
    let da = text_entry(arena, &field, "DA")
        .or_else(|| text_entry(arena, acro, "DA"))
        .unwrap_or_else(|| "/Helv 0 Tf 0 g".to_string());
    let quadding = field
        .get(&arena.name("Q"))
        .or_else(|| acro.get(&arena.name("Q")))
        .and_then(|q| q.resolve(arena).as_integer())
        .unwrap_or(0);

    for widget in widgets_of(arena, field_dh) {
        match value {
            FormValue::Text(text) | FormValue::Choice(text) => {
                let display_text = resolve_choice_display(arena, &field, text);
                appearance::set_text_appearance(doc, widget, acro, &da, quadding, &display_text)?;
            }
            FormValue::Boolean(on) => {
                let state = state.unwrap_or_else(|| arena.name(if *on { "Yes" } else { "Off" }));
                appearance::set_button_state(doc, widget, state);
            }
        }
    }
    Ok(())
}

/// The widgets a field is shown through: its `/Kids`, or the field itself when the two
/// are merged into one object (12.7.4.1).
fn widgets_of(
    arena: &PdfArena,
    field_dh: Handle<BTreeMap<Handle<PdfName>, Object>>,
) -> Vec<Handle<BTreeMap<Handle<PdfName>, Object>>> {
    let field = arena.get_dict(field_dh).unwrap_or_default();
    let kids = field
        .get(&arena.name("Kids"))
        .map(|k| k.resolve(arena))
        .and_then(|k| match k {
            Object::Array(ah) => arena.get_array(ah),
            _ => None,
        })
        .unwrap_or_default();
    let widgets: Vec<_> =
        kids.iter().filter_map(|kid| kid.resolve(arena).as_dict_handle()).collect();
    if widgets.is_empty() { vec![field_dh] } else { widgets }
}

/// Says what the scripts this run does not execute would have done (12.6.3).
///
/// **Setting one value can be the start of a cascade.** 12.6.3 says the effects of a
/// field-related action are limited only by the action itself and may make any other
/// modification to the document, and names the example directly: modifying a field value
/// can trigger calculations and further formatting for *other* fields. A caller writing a
/// value into a form that calculates has to be told, or it gets a document whose fields
/// disagree with each other and no sign that they do.
///
/// **Whether the scripts run is the frontend's to say, not this crate's.** `fepdf-script`
/// executes them and sits above the facade, so an `Operation` arrives here before
/// anything on this side can know
/// ([ADR-0032](../../../../docs/adr/0032-running-scripts-is-a-frontend-verb-not-an-operation.md)).
/// A frontend that will run them calls `Document::declare_script_processor`, and this
/// stays quiet. One that does not gets the warning it had before, which is every caller
/// that has not been wired.
fn report_scripts_not_run(
    doc: &Document,
    acro: &BTreeMap<Handle<PdfName>, Object>,
    field_name: &str,
) {
    if doc.runs_scripts() {
        return;
    }
    let arena = doc.arena();
    let calculated = acro
        .get(&arena.name("CO"))
        .map(|co| co.resolve(arena))
        .and_then(|co| match co {
            Object::Array(ah) => arena.get_array(ah),
            _ => None,
        })
        .map_or(0, |order| order.len());
    if calculated == 0 {
        return;
    }
    doc.record(Decision::violation(
        "12.6.3",
        format!(
            "the form declares {calculated} field(s) in its calculation order, and setting              {field_name} would have run their ECMAScript"
        ),
        "wrote the value and did not run the scripts; fields computed from it are now stale",
    ));
}

/// A text string entry, whichever of the two string forms it was written in.
fn text_entry(
    arena: &PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
    key: &str,
) -> Option<String> {
    match dict.get(&arena.name(key))?.resolve(arena) {
        Object::Text(text) => Some(text),
        Object::String(bytes) | Object::Hex(bytes) => {
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }
        _ => None,
    }
}

/// Alias for `apply_bates_numbering`.
pub use apply_bates_numbering as apply_bates;
