//! Creating a form field, not only filling one.
//!
//! **The engine read forms well and wrote them barely.** `/Ch` choice fields were parsed
//! with `/Opt`, `/I` and `/TI`, `inspect interactive` reported every field a document
//! carried, and `SetFormFieldValue` changed values in a form that already existed. No
//! operation created a field, so a document this engine declares PDF/UA-2 conforming
//! could have fields it could only complain about
//! ([ADR-0087](../../../../docs/adr/0087-a-form-field-is-created-here-not-only-filled.md)).
//!
//! **`/TU` is written on every field**, because a field without one is a Matterhorn
//! failure this engine already reports. An auditor that created the defect it names would
//! be writing its own findings.

use crate::operation::{FieldKind, NewField};
use fepdf_model::arena::PdfArena;
use fepdf_model::object::SublimatedData;
use fepdf_model::{Document, Handle, Object, PdfError, PdfName, PdfResult};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The font a created form is set in, until a caller says otherwise.
///
/// One of the standard 14, so a field this engine creates draws in a reader that has no
/// fonts of its own to fall back on.
const DEFAULT_FONT: &str = "Helv";

/// Creates `field`, with its widget on the page it names.
///
/// The widget and the field are one dictionary, which 12.7.5.2 allows and which every
/// form in the corpus does: a field with one widget has no reason to be two objects, and
/// two would be two places for its `/T` to disagree.
///
/// # Errors
/// Fails when the page is not there, when the rectangle has no area, or when the field
/// has no name — a form whose fields cannot be named is a form nothing can fill.
pub fn apply_add_form_field(doc: &Document, field: &NewField) -> PdfResult<()> {
    if field.name.trim().is_empty() {
        return Err(PdfError::Other("a form field with no name cannot be filled".into()));
    }
    if field.tooltip.trim().is_empty() {
        return Err(PdfError::Other(
            format!(
                "the field {:?} has no /TU, which is the entry a reader is announced it by",
                field.name
            )
            .into(),
        ));
    }
    let (width, height) = (field.rect.2 - field.rect.0, field.rect.3 - field.rect.1);
    if width <= 0.0 || height <= 0.0 {
        return Err(PdfError::Other(
            format!(
                "the field {:?} is {width} by {height} points, which draws nothing",
                field.name
            )
            .into(),
        ));
    }
    let page_h = doc
        .get_page_handle(field.page)
        .ok_or_else(|| PdfError::Other(format!("page {} is not there", field.page).into()))?;

    let arena = doc.arena();
    let widget = arena.alloc_object(Object::Dictionary(arena.alloc_dict(BTreeMap::new())));
    let mut dict = widget_dictionary(arena, field, page_h);
    if let Some(appearance) = appearance_for(arena, field, (width, height)) {
        dict.insert(arena.name("AP"), appearance);
    }
    let dict_h = arena.alloc_dict(dict);
    arena.set_object(widget, Object::Dictionary(dict_h));

    put_on_page(doc, page_h, widget)?;
    declare_in_form(doc, widget)
}

/// The dictionary that is both the field and its widget annotation.
fn widget_dictionary(
    arena: &PdfArena,
    field: &NewField,
    page: Handle<Object>,
) -> BTreeMap<Handle<PdfName>, Object> {
    let mut dict = BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Annot")));
    dict.insert(arena.name("Subtype"), Object::Name(arena.name("Widget")));
    dict.insert(arena.name("FT"), Object::Name(arena.name(field.kind.field_type())));
    // Text strings (7.9.2.2), which the writer encodes: the UTF-8 bytes bare are read as
    // PDFDocEncoding by every other reader, and a field named 電話 was named in mojibake.
    dict.insert(arena.name("T"), Object::Text(field.name.clone()));
    dict.insert(arena.name("TU"), Object::Text(field.tooltip.clone()));
    dict.insert(arena.name("P"), Object::Reference(page));
    let rect = [field.rect.0, field.rect.1, field.rect.2, field.rect.3];
    dict.insert(
        arena.name("Rect"),
        Object::Array(arena.alloc_array(rect.iter().map(|n| Object::Real(*n)).collect())),
    );
    let flags = field.kind.flags();
    if flags != 0 {
        dict.insert(arena.name("Ff"), Object::Integer(flags));
    }
    // `/DA` on the field itself, because 12.7.4.3 requires one on a variable-text field
    // when the form has none — which is the defect `isartor-6-9-t01` is made of.
    dict.insert(
        arena.name("DA"),
        Object::String(format!("/{DEFAULT_FONT} 0 Tf 0 g").into_bytes().into()),
    );
    add_kind(arena, &mut dict, &field.kind);
    dict
}

/// What the kind of field puts in the dictionary beyond its type and flags.
fn add_kind(arena: &PdfArena, dict: &mut BTreeMap<Handle<PdfName>, Object>, kind: &FieldKind) {
    let text = |value: &str| Object::Text(value.to_string());
    match kind {
        FieldKind::Text { value } | FieldKind::TextArea { value } => {
            dict.insert(arena.name("V"), text(value));
        }
        // A password's value is not written: a password in a document is a password in
        // the document.
        FieldKind::Password | FieldKind::Signature => {}
        FieldKind::CheckBox { on } | FieldKind::RadioButton { on, .. } => {
            // `/Off` is the name 12.7.5.2.3 gives the state that is not on, and `/AS`
            // says which of the appearances is showing.
            let state = arena.name(if *on { "Yes" } else { "Off" });
            dict.insert(arena.name("V"), Object::Name(state));
            dict.insert(arena.name("AS"), Object::Name(state));
        }
        FieldKind::PushButton { caption } => {
            // A push button holds no value, so the caption is its `/MK` `/CA` — what is
            // drawn on it (12.5.6.19, Table 192).
            let mut mk = BTreeMap::new();
            mk.insert(arena.name("CA"), text(caption));
            dict.insert(arena.name("MK"), Object::Dictionary(arena.alloc_dict(mk)));
        }
        FieldKind::ComboBox { options, value } | FieldKind::ListBox { options, value } => {
            let opt = options.iter().map(|o| text(o)).collect();
            dict.insert(arena.name("Opt"), Object::Array(arena.alloc_array(opt)));
            dict.insert(arena.name("V"), text(value));
            // `/I` is the index of what is chosen (12.7.4.4). A list longer than an
            // `i32` is longer than any `/Opt` a reader scrolls, and one that long has no
            // index this can state.
            if let Some(index) = options.iter().position(|o| o == value)
                && let Ok(index) = i32::try_from(index)
            {
                let indices = vec![Object::Integer(i64::from(index))];
                dict.insert(arena.name("I"), Object::Array(arena.alloc_array(indices)));
            }
        }
    }
}

/// An `/AP` for the kinds that have one to draw, or `None` for the kinds that do not.
fn appearance_for(arena: &PdfArena, field: &NewField, size: (f64, f64)) -> Option<Object> {
    let border = format!("q 0.5 w 0 0 {:.2} {:.2} re S Q", size.0, size.1);
    match &field.kind {
        // A tick and an empty box, keyed by the state `/AS` names (12.7.5.2.3). Both have
        // to be there: a widget whose `/AS` names an appearance it does not have draws
        // nothing, whichever way it is turned.
        FieldKind::CheckBox { .. } | FieldKind::RadioButton { .. } => {
            let tick = format!(
                "{border} q 1 w 2 2 m {:.2} {:.2} l S {:.2} 2 m 2 {:.2} l S Q",
                size.0 - 2.0,
                size.1 - 2.0,
                size.0 - 2.0,
                size.1 - 2.0
            );
            let mut states = BTreeMap::new();
            states.insert(arena.name("Yes"), Object::Reference(form(arena, size, &tick)));
            states.insert(arena.name("Off"), Object::Reference(form(arena, size, &border)));
            let mut ap = BTreeMap::new();
            ap.insert(arena.name("N"), Object::Dictionary(arena.alloc_dict(states)));
            Some(Object::Dictionary(arena.alloc_dict(ap)))
        }
        // A signature field is signed rather than filled, and an empty one that drew a
        // box would look like a field somebody had already dealt with.
        FieldKind::Signature => None,
        // The rest are a box a reader writes in or picks from, and what goes in it is
        // drawn when it is set: `set_text_appearance` rebuilds the `/N` from the value
        // (12.7.4.3). What this gives them is the frame that says where they are.
        FieldKind::Text { .. }
        | FieldKind::TextArea { .. }
        | FieldKind::Password
        | FieldKind::PushButton { .. }
        | FieldKind::ComboBox { .. }
        | FieldKind::ListBox { .. } => {
            let mut ap = BTreeMap::new();
            ap.insert(arena.name("N"), Object::Reference(form(arena, size, &border)));
            Some(Object::Dictionary(arena.alloc_dict(ap)))
        }
    }
}

/// A form XObject of `size` holding `content` (8.10).
fn form(arena: &PdfArena, size: (f64, f64), content: &str) -> Handle<Object> {
    let box_ = [0.0, 0.0, size.0, size.1];
    let mut dict = BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("XObject")));
    dict.insert(arena.name("Subtype"), Object::Name(arena.name("Form")));
    dict.insert(
        arena.name("BBox"),
        Object::Array(arena.alloc_array(box_.iter().map(|n| Object::Real(*n)).collect())),
    );
    let dict_h = arena.alloc_dict(dict);
    let data = SublimatedData::Raw(bytes::Bytes::from(content.to_string()));
    arena.alloc_object(Object::Stream(dict_h, Arc::new(data)))
}

/// Puts the widget in the page's `/Annots`, making one if the page has none.
fn put_on_page(doc: &Document, page: Handle<Object>, widget: Handle<Object>) -> PdfResult<()> {
    let page_dh = doc.resolve_to_dict(page)?;
    let arena = doc.arena();
    let mut dict = arena.get_dict(page_dh).unwrap_or_default();
    let key = arena.name("Annots");
    let mut annots = match dict.get(&key).map(|a| a.resolve(arena)) {
        Some(Object::Array(handle)) => arena.get_array(handle).unwrap_or_default(),
        _ => Vec::new(),
    };
    annots.push(Object::Reference(widget));
    dict.insert(key, Object::Array(arena.alloc_array(annots)));
    arena.set_dict(page_dh, dict);
    Ok(())
}

/// Adds the field to `/AcroForm` `/Fields`, writing the form itself if there is none.
///
/// **A document that had no form gets one that works.** `/DA` and `/DR` are what a
/// variable-text field is drawn by (12.7.4.3), and a form declaring fields without them
/// is a form whose fields a reader sees nothing of.
fn declare_in_form(doc: &Document, widget: Handle<Object>) -> PdfResult<()> {
    let arena = doc.arena();
    let catalog_h = doc
        .catalog_handle()
        .ok_or_else(|| PdfError::Other("the document has no catalogue".into()))?;
    let catalog_dh = doc.resolve_to_dict(catalog_h)?;
    let mut catalog = arena.get_dict(catalog_dh).unwrap_or_default();

    let acro_key = arena.name("AcroForm");
    let existing =
        catalog.get(&acro_key).map(|a| a.resolve(arena)).and_then(|a| a.as_dict_handle());
    let acro_dh = existing.unwrap_or_else(|| arena.alloc_dict(BTreeMap::new()));
    let mut acro = arena.get_dict(acro_dh).unwrap_or_default();

    let fields_key = arena.name("Fields");
    let mut fields = match acro.get(&fields_key).map(|f| f.resolve(arena)) {
        Some(Object::Array(handle)) => arena.get_array(handle).unwrap_or_default(),
        _ => Vec::new(),
    };
    fields.push(Object::Reference(widget));
    acro.insert(fields_key, Object::Array(arena.alloc_array(fields)));

    // A form that already says how its fields are drawn keeps saying it: these are
    // written for a document that had no form, not over one that had.
    acro.entry(arena.name("DA"))
        .or_insert_with(|| Object::String(format!("/{DEFAULT_FONT} 0 Tf 0 g").into_bytes().into()));
    acro.entry(arena.name("DR")).or_insert_with(|| Object::Dictionary(default_resources(arena)));
    arena.set_dict(acro_dh, acro);
    if existing.is_none() {
        catalog.insert(acro_key, Object::Dictionary(acro_dh));
        arena.set_dict(catalog_dh, catalog);
    }
    Ok(())
}

/// Writes `/CO`: the fields that calculate, in the order `order` names them (Table 224).
///
/// # Errors
/// Fails when the document has no form, when `order` names a field twice or names one
/// that calculates nothing, or when it leaves out a field that calculates — each naming
/// the field, because the caller has to know which to fix.
pub fn apply_set_calculation_order(doc: &Document, order: &[String]) -> PdfResult<()> {
    let arena = doc.arena();
    let catalog_dh = doc.resolve_to_dict(
        doc.catalog_handle()
            .ok_or_else(|| PdfError::Other("the document has no catalogue".into()))?,
    )?;
    let acro_dh = arena
        .dict_entry(catalog_dh, arena.name("AcroForm"))
        .and_then(|a| a.resolve(arena).as_dict_handle())
        .ok_or_else(|| PdfError::Other("the document has no form to order".into()))?;
    let calculating = calculating_fields(arena, acro_dh);

    let mut references = Vec::with_capacity(order.len());
    for (nth, name) in order.iter().enumerate() {
        if order[..nth].contains(name) {
            return Err(PdfError::Other(format!("the order names {name:?} twice").into()));
        }
        let Some(field) = calculating.get(name) else {
            return Err(PdfError::Other(
                format!("no field named {name:?} has a calculation to order").into(),
            ));
        };
        references.push(Object::Reference(*field));
    }
    if let Some(left_out) = calculating.keys().find(|name| !order.contains(name)) {
        return Err(PdfError::Other(
            format!("the order leaves out {left_out:?}, which calculates").into(),
        ));
    }

    let mut acro = arena.get_dict(acro_dh).unwrap_or_default();
    let key = arena.name("CO");
    if references.is_empty() {
        acro.remove(&key);
    } else {
        acro.insert(key, Object::Array(arena.alloc_array(references)));
    }
    arena.set_dict(acro_dh, acro);
    Ok(())
}

/// Every field with a calculation action — `/AA` holding `/C` (12.6.3) — by its fully
/// qualified name.
fn calculating_fields(
    arena: &PdfArena,
    acro: fepdf_model::DictHandle,
) -> BTreeMap<String, Handle<Object>> {
    let (aa, c) = (arena.name("AA"), arena.name("C"));
    named_fields(arena, acro)
        .into_iter()
        .filter(|(_, _, dict)| {
            arena
                .dict_entry(*dict, aa)
                .and_then(|actions| actions.resolve(arena).as_dict_handle())
                .is_some_and(|actions| arena.dict_entry(actions, c).is_some())
        })
        .map(|(name, handle, _)| (name, handle))
        .collect()
}

/// Every field of the form that has a name of its own, by its fully qualified name
/// (12.7.4.2), in the order `/Fields` and `/Kids` give them.
///
/// **A field, not a widget below it.** A widget with no `/T` is the widget of the field
/// above it and is not listed; the field is what a value is written to and what `/CO`
/// names. Names are decoded the way `form_of` decodes them, so a name read there finds
/// its field here — `sample_02c.pdf` names its fields in UTF-16, and a comparison of the
/// raw bytes as UTF-8 found none of them.
///
/// `/CO` and a value both need the object, so a field written direct is not listed; the
/// walk stops at the depth `fepdf-model`'s own field walk does, so a cycle in `/Kids`
/// ends it rather than the process.
pub(crate) fn named_fields(
    arena: &PdfArena,
    acro: fepdf_model::DictHandle,
) -> Vec<(String, Handle<Object>, fepdf_model::DictHandle)> {
    let mut found = Vec::new();
    let roots = match arena.dict_entry(acro, arena.name("Fields")).map(|f| f.resolve(arena)) {
        Some(Object::Array(handle)) => arena.get_array(handle).unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut pending: Vec<(Object, Option<String>, usize)> =
        roots.into_iter().rev().map(|field| (field, None, 0)).collect();
    let (t, kids) = (arena.name("T"), arena.name("Kids"));
    while let Some((field, parent, depth)) = pending.pop() {
        let Some(handle) = field.as_reference() else { continue };
        let Some(dict) = arena.get_object(handle).and_then(|o| o.as_dict_handle()) else {
            continue;
        };
        let own = arena.dict_entry(dict, t).and_then(|name| text_of(arena, &name));
        let name = match (&parent, &own) {
            (Some(parent), Some(own)) => Some(format!("{parent}.{own}")),
            (None, own) => own.clone(),
            (Some(parent), None) => Some(parent.clone()),
        };
        if own.is_some()
            && let Some(name) = &name
        {
            found.push((name.clone(), handle, dict));
        }
        if depth < 64
            && let Some(Object::Array(children)) =
                arena.dict_entry(dict, kids).map(|k| k.resolve(arena))
        {
            for child in arena.get_array(children).unwrap_or_default().into_iter().rev() {
                pending.push((child, name.clone(), depth + 1));
            }
        }
    }
    found
}

/// A text string, decoded the way the form's reading decodes one, so a name matches the
/// name `form_of` reports.
pub(crate) fn text_of(arena: &PdfArena, object: &Object) -> Option<String> {
    match object.resolve(arena) {
        Object::String(bytes) | Object::Hex(bytes) => {
            Some(fepdf_model::refine::text::recover_string(&bytes))
        }
        Object::Text(text) => Some(text),
        _ => None,
    }
}

/// `/DR`, carrying the one font a created field is set in.
fn default_resources(arena: &PdfArena) -> fepdf_model::DictHandle {
    let mut font = BTreeMap::new();
    font.insert(arena.name("Type"), Object::Name(arena.name("Font")));
    font.insert(arena.name("Subtype"), Object::Name(arena.name("Type1")));
    font.insert(arena.name("BaseFont"), Object::Name(arena.name("Helvetica")));
    font.insert(arena.name("Name"), Object::Name(arena.name(DEFAULT_FONT)));
    let font_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(font)));

    let mut fonts = BTreeMap::new();
    fonts.insert(arena.name(DEFAULT_FONT), Object::Reference(font_h));
    let mut resources = BTreeMap::new();
    resources.insert(arena.name("Font"), Object::Dictionary(arena.alloc_dict(fonts)));
    arena.alloc_dict(resources)
}
