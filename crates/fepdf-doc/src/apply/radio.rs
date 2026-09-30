//! Radio buttons: one field, a widget per button, one chosen at a time (12.7.5.2.4).
//!
//! **The group is the field, and a button is a widget of it.** Each button was created as
//! a field of its own under its own name, with the on state every other button had too,
//! `/Yes` — so buttons given the same group shared nothing, and choosing one left the
//! rest as they were. The group names the field the buttons share, as `FieldKind`
//! documents it; each button is a widget in that field's `/Kids`, with no `/T`, and its
//! on state is its own name, which is what choosing it writes into the field's `/V`.

use crate::operation::NewField;
use fepdf_model::{DictHandle, Document, Handle, Object, PdfError, PdfName, PdfResult};
use std::collections::BTreeMap;

/// Adds one button to the group `group`, creating the group's field if the form has none
/// of that name, chosen if `on`.
///
/// # Errors
/// Fails when the group already has a button of this name — two buttons with one state
/// are chosen together, which is what a radio button is not — when the button is named
/// `Off`, the state of none being chosen (12.7.5.2.3), or when a field of the group's name
/// exists and is not a set of radio buttons.
pub fn add_radio_button(
    doc: &Document,
    button: &NewField,
    page: Handle<Object>,
    (group, on): (&str, bool),
) -> PdfResult<()> {
    let arena = doc.arena();
    if button.name == "Off" {
        return Err(PdfError::Other(
            "a radio button cannot be named Off, which is the state of none being chosen".into(),
        ));
    }
    let parent = match group_field(doc, group)? {
        Some(parent) => parent,
        None => new_group(doc, button, group)?,
    };
    let parent_dh = doc.resolve_to_dict(parent)?;
    let kids = kids_of(doc, parent_dh);
    let state = arena.name(&button.name);
    if kids.iter().any(|kid| crate::apply::appearance::button_states(doc, *kid).contains(&state)) {
        return Err(PdfError::Other(
            format!("the group {group:?} already has a button named {:?}", button.name).into(),
        ));
    }
    let widget = widget(doc, button, page, parent, if on { state } else { arena.name("Off") });
    let mut field = arena.get_dict(parent_dh).unwrap_or_default();
    let mut listed = match field.get(&arena.name("Kids")).map(|k| k.resolve(arena)) {
        Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
        _ => Vec::new(),
    };
    listed.push(Object::Reference(widget));
    field.insert(arena.name("Kids"), Object::Array(arena.alloc_array(listed)));
    if on {
        field.insert(arena.name("V"), Object::Name(state));
        for kid in kids {
            crate::apply::appearance::set_button_state(doc, kid, arena.name("Off"));
        }
    }
    arena.set_dict(parent_dh, field);
    crate::apply::fields::put_on_page(doc, page, widget)
}

/// The field named `group`, if the form has one: a set of radio buttons, or an error.
fn group_field(doc: &Document, group: &str) -> PdfResult<Option<Handle<Object>>> {
    let arena = doc.arena();
    let Some(acro) = doc
        .catalog_handle()
        .and_then(|c| doc.resolve_to_dict(c).ok())
        .and_then(|c| arena.dict_entry(c, arena.name("AcroForm")))
        .and_then(|a| a.resolve(arena).as_dict_handle())
    else {
        return Ok(None);
    };
    let found = crate::apply::fields::named_fields(arena, acro)
        .into_iter()
        .find(|(name, _, _)| name == group);
    let Some((_, handle, dict)) = found else { return Ok(None) };
    let flags = arena.dict_entry(dict, arena.name("Ff")).and_then(|f| f.as_integer()).unwrap_or(0);
    if flags & RADIO == 0 {
        return Err(PdfError::Other(
            format!("the form has a field named {group:?}, and it is not a set of radio buttons")
                .into(),
        ));
    }
    Ok(Some(handle))
}

/// `Radio`, bit 16 of a button field's `/Ff` (Table 230).
const RADIO: i64 = 1 << 15;

/// The field a group's buttons share, declared in the form, with nothing chosen yet. Its
/// `/TU` is the first button's, since a field without one is a Matterhorn failure.
fn new_group(doc: &Document, button: &NewField, group: &str) -> PdfResult<Handle<Object>> {
    let arena = doc.arena();
    let mut dict: BTreeMap<Handle<PdfName>, Object> = BTreeMap::new();
    dict.insert(arena.name("FT"), Object::Name(arena.name("Btn")));
    dict.insert(arena.name("Ff"), Object::Integer(button.kind.flags()));
    dict.insert(arena.name("T"), Object::Text(group.to_string()));
    dict.insert(arena.name("TU"), Object::Text(button.tooltip.clone()));
    dict.insert(arena.name("V"), Object::Name(arena.name("Off")));
    dict.insert(arena.name("Kids"), Object::Array(arena.alloc_array(Vec::new())));
    let field = arena.alloc_object(Object::Dictionary(arena.alloc_dict(dict)));
    crate::apply::fields::declare_in_form(doc, field)?;
    Ok(field)
}

/// The widgets already in the group, as dictionaries.
fn kids_of(doc: &Document, parent: DictHandle) -> Vec<DictHandle> {
    let arena = doc.arena();
    match arena.dict_entry(parent, arena.name("Kids")).map(|k| k.resolve(arena)) {
        Some(Object::Array(array)) => arena
            .get_array(array)
            .unwrap_or_default()
            .iter()
            .filter_map(|kid| kid.resolve(arena).as_dict_handle())
            .collect(),
        _ => Vec::new(),
    }
}

/// One button's widget: in the group's `/Kids`, showing `shown`, with its own name as the
/// state that shows it chosen.
fn widget(
    doc: &Document,
    button: &NewField,
    page: Handle<Object>,
    parent: Handle<Object>,
    shown: Handle<PdfName>,
) -> Handle<Object> {
    let arena = doc.arena();
    let (x1, y1, x2, y2) = button.rect;
    let mut dict: BTreeMap<Handle<PdfName>, Object> = BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Annot")));
    dict.insert(arena.name("Subtype"), Object::Name(arena.name("Widget")));
    dict.insert(arena.name("Parent"), Object::Reference(parent));
    dict.insert(arena.name("P"), Object::Reference(page));
    let rect = [x1, y1, x2, y2].iter().map(|n| Object::Real(*n)).collect();
    dict.insert(arena.name("Rect"), Object::Array(arena.alloc_array(rect)));
    dict.insert(arena.name("AS"), Object::Name(shown));
    let appearance =
        crate::apply::fields::button_appearance(arena, (x2 - x1, y2 - y1), &button.name);
    dict.insert(arena.name("AP"), appearance);
    arena.alloc_object(Object::Dictionary(arena.alloc_dict(dict)))
}
