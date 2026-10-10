//! A widget with no appearance, drawn from its field (12.5.6.19, 12.7.4.3; ADR-0120).
//!
//! **A text or choice value is set as filling it would set it**, through
//! `set_text_appearance`, in the field's `/DA` and `/Q` or the form's. **A check box or
//! radio button gets two states**: its on state, named by `/AS` where that is not `/Off`
//! and `/Yes` where it is, and `/Off`; on draws a check or a dot. **A push button gets its
//! caption**, `/MK /CA`, set the same way as a text value.

use super::super::appearance;
use fepdf_model::object::{PdfName, SublimatedData};
use fepdf_model::{DictHandle, Document, Handle, Object, PdfArena, PdfResult};
use std::collections::BTreeMap;
use std::sync::Arc;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// `/Ff` bit 17: a push button (Table 229).
const PUSH: i64 = 1 << 16;
/// `/Ff` bit 16: a radio button (Table 229).
const RADIO: i64 = 1 << 15;

/// Gives the widget `widget` an appearance from its field; whether it could.
///
/// # Errors
/// When a face found for the value will not embed.
pub(super) fn widget(doc: &Document, widget: DictHandle) -> PdfResult<bool> {
    let arena = doc.arena();
    let acro = form(doc);
    let entry =
        |key: &str| inherited(arena, widget, key).or_else(|| acro.get(&arena.name(key)).cloned());
    let kind = entry("FT")
        .and_then(|t| t.as_name())
        .and_then(|n| arena.get_name_str(n))
        .unwrap_or_default();
    let flags = entry("Ff").and_then(|f| f.as_integer()).unwrap_or(0);
    let da = entry("DA")
        .and_then(|d| super::super::fields::text_of(arena, &d))
        .unwrap_or_else(|| "/Helv 0 Tf 0 g".to_owned());
    let quadding = entry("Q").and_then(|q| q.as_integer()).unwrap_or(0);
    match kind.as_str() {
        "Btn" if flags & PUSH != 0 => {
            let caption = mk_text(arena, widget, "CA").unwrap_or_default();
            words(doc, widget, (&acro, &da, quadding), &caption)
        }
        "Btn" => {
            states(arena, widget, flags & RADIO != 0);
            Ok(true)
        }
        "Tx" | "Ch" => {
            let value = entry("V").map(|v| shown_value(arena, &v)).unwrap_or_default();
            words(doc, widget, (&acro, &da, quadding), &value)
        }
        _ => Ok(false),
    }
}

/// `text` set in the widget, or, where there is no text, an appearance that draws nothing:
/// an empty field has nothing to set, and looking for a face to set nothing in embeds one
/// for no glyph.
fn words(
    doc: &Document,
    widget: DictHandle,
    (acro, da, quadding): (&Dict, &str, i64),
    text: &str,
) -> PdfResult<bool> {
    if !text.trim().is_empty() {
        return appearance::set_text_appearance(doc, widget, acro, da, quadding, text);
    }
    let arena = doc.arena();
    let mut dict = arena.get_dict(widget).unwrap_or_default();
    let (w, h) = size(arena, &dict);
    let mut ap = Dict::new();
    ap.insert(arena.name("N"), Object::Reference(form_xobject(arena, w, h, "")));
    dict.insert(arena.name("AP"), Object::Dictionary(arena.alloc_dict(ap)));
    arena.set_dict(widget, dict);
    Ok(true)
}

/// The form's dictionary, or an empty one.
fn form(doc: &Document) -> Dict {
    let arena = doc.arena();
    doc.catalog_handle()
        .and_then(|c| doc.resolve_to_dict(c).ok())
        .and_then(|c| arena.dict_entry(c, arena.name("AcroForm")))
        .and_then(|a| a.resolve(arena).as_dict_handle())
        .and_then(|a| arena.get_dict(a))
        .unwrap_or_default()
}

/// An entry of the widget, or of the field it belongs to or one of that field's
/// ancestors (12.7.4.2's inheritance), resolved.
fn inherited(arena: &PdfArena, start: DictHandle, key: &str) -> Option<Object> {
    let (key, parent) = (arena.name(key), arena.name("Parent"));
    let mut at = Some(start);
    // As deep as a field tree is climbed elsewhere (Rule 6).
    for _ in 0..64 {
        let here = at?;
        if let Some(value) = arena.dict_entry(here, key) {
            return Some(value.resolve(arena));
        }
        at = arena.dict_entry(here, parent).and_then(|p| p.resolve(arena).as_dict_handle());
    }
    None
}

/// A field's value as shown: a text string, a name, or the first of an array of choices.
///
/// One level of array and no more (Rule 6): an array whose first item is an array, or
/// itself, shows nothing rather than being followed.
fn shown_value(arena: &PdfArena, value: &Object) -> String {
    let single = match value {
        Object::Array(items) => {
            arena.get_array(*items).and_then(|i| i.first().map(|v| v.resolve(arena)))
        }
        other => Some(other.clone()),
    };
    match single {
        Some(Object::Name(n)) => arena.get_name_str(n).unwrap_or_default(),
        Some(Object::Array(_)) | None => String::new(),
        Some(other) => super::super::fields::text_of(arena, &other).unwrap_or_default(),
    }
}

/// An entry of the widget's appearance characteristics (Table 192) as text.
fn mk_text(arena: &PdfArena, widget: DictHandle, key: &str) -> Option<String> {
    let mk = arena.dict_entry(widget, arena.name("MK"))?.resolve(arena).as_dict_handle()?;
    super::super::fields::text_of(arena, &arena.dict_entry(mk, arena.name(key))?)
}

/// A check box's or radio button's two states, and `/AS` where it has none.
fn states(arena: &PdfArena, widget: DictHandle, radio: bool) {
    let mut dict = arena.get_dict(widget).unwrap_or_default();
    let (w, h) = size(arena, &dict);
    let off = arena.name("Off");
    let as_now = dict.get(&arena.name("AS")).and_then(Object::as_name);
    let on = as_now.filter(|s| *s != off).unwrap_or_else(|| arena.name("Yes"));
    let side = w.min(h);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let mark = if radio {
        format!("0 g\n{}f\n", super::lines::ellipse(cx, cy, side * 0.25, side * 0.25))
    } else {
        format!(
            "0 G {:.2} w 1 J 1 j\n{:.2} {:.2} m {:.2} {:.2} l {:.2} {:.2} l S\n",
            side * 0.1,
            side.mul_add(-0.3, cx),
            cy,
            side.mul_add(-0.08, cx),
            side.mul_add(-0.25, cy),
            side.mul_add(0.32, cx),
            side.mul_add(0.3, cy)
        )
    };
    let frame = format!("0.5 G 1 w 0.5 0.5 {:.2} {:.2} re S\n", w - 1.0, h - 1.0);
    let mut normal = Dict::new();
    normal.insert(on, Object::Reference(form_xobject(arena, w, h, &format!("{frame}{mark}"))));
    normal.insert(off, Object::Reference(form_xobject(arena, w, h, &frame)));
    let mut ap = Dict::new();
    ap.insert(arena.name("N"), Object::Dictionary(arena.alloc_dict(normal)));
    dict.insert(arena.name("AP"), Object::Dictionary(arena.alloc_dict(ap)));
    dict.entry(arena.name("AS")).or_insert(Object::Name(off));
    arena.set_dict(widget, dict);
}

/// The widget's width and height, from `/Rect`.
fn size(arena: &PdfArena, dict: &Dict) -> (f64, f64) {
    let numbers: Vec<f64> = dict
        .get(&arena.name("Rect"))
        .and_then(|r| r.resolve(arena).as_array())
        .and_then(|a| arena.get_array(a))
        .unwrap_or_default()
        .iter()
        .filter_map(|n| n.resolve(arena).as_f64())
        .collect();
    match numbers[..] {
        [x0, y0, x1, y1] => ((x1 - x0).abs(), (y1 - y0).abs()),
        _ => (0.0, 0.0),
    }
}

fn form_xobject(arena: &PdfArena, w: f64, h: f64, content: &str) -> Handle<Object> {
    let mut dict = Dict::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("XObject")));
    dict.insert(arena.name("Subtype"), Object::Name(arena.name("Form")));
    let bbox = [0.0, 0.0, w, h].iter().map(|v| Object::Real(*v)).collect();
    dict.insert(arena.name("BBox"), Object::Array(arena.alloc_array(bbox)));
    // Table 93 requires `/Resources` in PDF 2.0, though this one names none.
    dict.insert(arena.name("Resources"), Object::Dictionary(arena.alloc_dict(Dict::new())));
    let stream = Object::Stream(
        arena.alloc_dict(dict),
        Arc::new(SublimatedData::Raw(bytes::Bytes::copy_from_slice(content.as_bytes()))),
    );
    arena.alloc_object(stream)
}
