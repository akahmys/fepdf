//! An annotation's appearance, drawn from its own entries (ROADMAP AA-4b).
//!
//! **Table 166 requires an `/AP` on every annotation but a Popup, a Projection, a Link and
//! one whose rectangle is a point**, and XFDF carries one only for a stamp. So what this
//! engine imports, it draws ([ADR-0119]). Each subtype's table in 12.5.6 says which entry
//! holds what, and the drawing reads that entry and nothing it does not have:
//!
//! - `lines` — Line, Square, Circle, Polygon, PolyLine and Ink, and Table 179's endings;
//! - `quads` — Highlight, Underline, StrikeOut, Squiggly and Redact, by `/QuadPoints`;
//! - `icons` — Text, FileAttachment, Sound and Caret, whose icons the standard names and
//!   does not draw;
//! - `words` — FreeText and Stamp, which set text.
//!
//! The drawing is in the annotation's own space: `/BBox` is `[0 0 w h]` over `/Rect`, so
//! a point on the page is moved by the rectangle's lower left corner (12.5.5).
//!
//! [ADR-0119]: ../../../../../docs/adr/0119-an-annotation-without-an-appearance-is-given-one-from-its-own-entries.md

mod icons;
mod lines;
pub mod made;
mod quads;
mod widgets;
mod words;

use super::markup::{Area, appearance};
use fepdf_model::{Document, Handle, Object, PdfArena, PdfName, PdfResult};
use std::collections::BTreeMap;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// The appearance `dict` should carry, drawn from its entries; `None` where Table 166 asks
/// for none, or the subtype is not one this draws.
///
/// # Errors
/// When setting a free text's or a stamp's words fails for a reason other than a face
/// that cannot draw them, which is drawn without the words instead.
pub fn appearance_for(doc: &Document, dict: &Dict) -> PdfResult<Option<Object>> {
    let arena = doc.arena();
    let entries = Entries::new(arena, dict);
    let Some(area) = entries.rect() else { return Ok(None) };
    // A rectangle that is a point needs no appearance (Table 166), and has nothing to
    // draw one in.
    if area.width() <= 0.0 && area.height() <= 0.0 {
        return Ok(None);
    }
    let subtype = entries.name("Subtype").unwrap_or_default();
    let drawn = match subtype.as_str() {
        "Line" => Some(lines::line(&entries, area)),
        "Square" | "Circle" => Some(lines::shape(&entries, area, subtype == "Circle")),
        "Polygon" | "PolyLine" => Some(lines::polygon(&entries, area, subtype == "Polygon")),
        "Ink" => Some(lines::ink(&entries, area)),
        "Highlight" | "Underline" | "StrikeOut" | "Squiggly" => {
            Some(quads::marking(&entries, area, &subtype))
        }
        "Redact" => Some(quads::redaction(&entries, area)),
        "Text" | "FileAttachment" | "Sound" | "Caret" => {
            Some(icons::icon(&entries, area, &subtype))
        }
        "FreeText" => Some(words::free_text(doc, &entries, area)?),
        "Stamp" => Some(words::stamp(doc, &entries, area)?),
        _ => None,
    };
    Ok(drawn.map(|d| finish(arena, &entries, d, area)))
}

/// Subtypes that need no appearance: the three Table 166 excepts, and a screen, which
/// without one "shall not have a default visual appearance" (12.5.6.18).
const EXEMPT: &[&str] = &["Popup", "Link", "Projection", "Screen"];

/// Gives every annotation of `doc` that has no appearance the one it can be given.
///
/// A `Decision` records each (ADR-0120). Called while opening, before the arena is sealed:
/// this is how the document is read, not a change made to it.
///
/// An annotation whose appearance is its content — PrinterMark, TrapNet, Watermark — or
/// whose subtype this engine does not know is left without one, and that is recorded
/// too. Nothing here fails the open: an annotation that cannot be drawn is recorded.
pub fn give_missing(doc: &Document) {
    let Ok(pages) = doc.page_count() else { return };
    for page in 0..pages {
        let Ok(annots) = super::redact_annots::annotations_on(doc, page) else { continue };
        for (index, annot) in annots.iter().enumerate() {
            give_one(doc, (page, index), annot);
        }
    }
}

/// One annotation's appearance, where it has none and needs one.
fn give_one(doc: &Document, (page, index): (usize, usize), annot: &Object) {
    use fepdf_model::interpretation::Decision;
    let arena = doc.arena();
    let Some(handle) = annot.resolve(arena).as_dict_handle() else { return };
    let dict = arena.get_dict(handle).unwrap_or_default();
    if dict.contains_key(&arena.name("AP")) {
        return;
    }
    let entries = Entries::new(arena, &dict);
    let subtype = entries.name("Subtype").unwrap_or_default();
    if EXEMPT.contains(&subtype.as_str()) {
        return;
    }
    let found = format!("page {page}, annotation {index}: a /{subtype} with no appearance");
    let drawn = match subtype.as_str() {
        "Widget" => widgets::widget(doc, handle).map(|done| done.then_some(())),
        "Movie" | "RichMedia" | "3D" => Ok(entries
            .rect()
            .map(|area| set(doc, handle, icons::media(area, subtype == "3D"), area))),
        _ => appearance_for(doc, &dict).map(|made| made.map(|ap| put(arena, handle, ap))),
    };
    match drawn {
        Ok(Some(())) => doc.record(Decision::repaired(
            "12.5.5",
            found,
            "drew one from its entries, as Table 166 requires one (ADR-0120)",
        )),
        // A point rectangle needs none, and is not a finding.
        Ok(None) if entries.rect().is_some_and(|a| a.width() <= 0.0 && a.height() <= 0.0) => {}
        Ok(None) => doc.record(Decision::violation(
            "12.5.5",
            found,
            "left it without one: Table 166 requires one, and this engine cannot draw what it holds",
        )),
        Err(why) => doc.record(Decision::violation("12.5.5", found, format!("could not draw one: {why}"))),
    }
}

/// Puts `drawing` on the annotation as its appearance.
fn set(doc: &Document, handle: fepdf_model::DictHandle, drawing: Drawing, area: Area) {
    let arena = doc.arena();
    let dict = arena.get_dict(handle).unwrap_or_default();
    let ap = finish(arena, &Entries::new(arena, &dict), drawing, area);
    put(arena, handle, ap);
}

fn put(arena: &PdfArena, handle: fepdf_model::DictHandle, ap: Object) {
    let mut dict = arena.get_dict(handle).unwrap_or_default();
    dict.insert(arena.name("AP"), ap);
    arena.set_dict(handle, dict);
}

/// What a drawing came to: the content, and the resources it names.
pub(super) struct Drawing {
    content: String,
    resources: Dict,
}

impl Drawing {
    pub(super) fn of(content: String) -> Self {
        Self { content, resources: Dict::new() }
    }
}

/// The drawing as an appearance, with the annotation's opacity (`/CA`, Table 172) put in
/// front of it.
fn finish(arena: &PdfArena, entries: &Entries<'_>, mut drawing: Drawing, area: Area) -> Object {
    if let Some(opacity) = entries.number("CA").filter(|a| *a < 1.0) {
        let mut state = Dict::new();
        state.insert(arena.name("CA"), Object::Real(opacity));
        state.insert(arena.name("ca"), Object::Real(opacity));
        merge_state(arena, &mut drawing.resources, "GA", state);
        drawing.content = format!("/GA gs\n{}", drawing.content);
    }
    let resources = (!drawing.resources.is_empty()).then_some(drawing.resources);
    appearance(arena, &drawing.content, area, resources)
}

/// Puts `state` under `name` in the resources' `/ExtGState`.
pub(super) fn merge_state(arena: &PdfArena, resources: &mut Dict, name: &str, state: Dict) {
    let key = arena.name("ExtGState");
    let mut states = match resources.get(&key) {
        Some(Object::Dictionary(d)) => arena.get_dict(*d).unwrap_or_default(),
        _ => Dict::new(),
    };
    states.insert(arena.name(name), Object::Dictionary(arena.alloc_dict(state)));
    resources.insert(key, Object::Dictionary(arena.alloc_dict(states)));
}

/// An annotation dictionary, read as its tables define it.
pub struct Entries<'a> {
    arena: &'a PdfArena,
    dict: &'a Dict,
}

impl<'a> Entries<'a> {
    /// `dict`, read through `arena`.
    pub const fn new(arena: &'a PdfArena, dict: &'a Dict) -> Self {
        Self { arena, dict }
    }

    /// The arena the dictionary is in.
    pub const fn arena(&self) -> &'a PdfArena {
        self.arena
    }

    /// An entry as written: a reference stays a reference, which is what `/IRT` is.
    pub fn written(&self, key: &str) -> Option<Object> {
        self.dict.get(&self.arena.name(key)).cloned()
    }

    /// An entry, resolved.
    pub fn get(&self, key: &str) -> Option<Object> {
        self.dict.get(&self.arena.name(key)).map(|v| v.resolve(self.arena))
    }

    /// A name entry, as text.
    pub fn name(&self, key: &str) -> Option<String> {
        self.get(key)?.as_name().and_then(|n| self.arena.get_name_str(n))
    }

    /// A number entry.
    pub fn number(&self, key: &str) -> Option<f64> {
        self.get(key)?.as_f64()
    }

    /// A text string entry.
    pub fn text(&self, key: &str) -> Option<String> {
        super::fields::text_of(self.arena, &self.get(key)?)
    }

    /// An array of numbers, or empty.
    pub fn numbers(&self, key: &str) -> Vec<f64> {
        self.get(key).map_or_else(Vec::new, |v| numbers_in(self.arena, &v))
    }

    /// An array of arrays of numbers (`/InkList`, `/Path`), or empty.
    pub fn arrays(&self, key: &str) -> Vec<Vec<f64>> {
        let Some(outer) =
            self.get(key).and_then(|v| v.as_array()).and_then(|a| self.arena.get_array(a))
        else {
            return Vec::new();
        };
        outer.iter().map(|inner| numbers_in(self.arena, &inner.resolve(self.arena))).collect()
    }

    /// The names of an array (`/LE`), or a single name as one.
    pub fn names(&self, key: &str) -> Vec<String> {
        match self.get(key) {
            Some(Object::Name(n)) => self.arena.get_name_str(n).into_iter().collect(),
            Some(Object::Array(a)) => self
                .arena
                .get_array(a)
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.as_name().and_then(|n| self.arena.get_name_str(n)))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// `/Rect`, as an upright rectangle.
    pub fn rect(&self) -> Option<Area> {
        match self.numbers("Rect")[..] {
            [x0, y0, x1, y1] => Some(Area {
                left: x0.min(x1),
                bottom: y0.min(y1),
                right: x0.max(x1),
                top: y0.max(y1),
            }),
            _ => None,
        }
    }

    /// An entry from a sub-dictionary (`/BS /W`).
    pub fn within(&self, outer: &str, key: &str) -> Option<Object> {
        let dict = self.get(outer)?.as_dict_handle()?;
        self.arena.dict_entry(dict, self.arena.name(key)).map(|v| v.resolve(self.arena))
    }

    /// The pen Table 168 describes: `/BS`'s width and dashes, or the older `/Border`'s.
    pub(super) fn pen(&self) -> Pen {
        let border = self.numbers("Border");
        let width = self
            .within("BS", "W")
            .and_then(|w| w.as_f64())
            .or_else(|| border.get(2).copied())
            .unwrap_or(1.0)
            .max(0.0);
        let dashed = self.within("BS", "S").and_then(|s| s.as_name()) == Some(self.arena.name("D"));
        let dashes = if dashed {
            let given =
                self.within("BS", "D").map(|d| numbers_in(self.arena, &d)).unwrap_or_default();
            Some(if given.is_empty() { vec![3.0] } else { given })
        } else {
            self.get("Border")
                .and_then(|b| b.as_array())
                .and_then(|a| self.arena.get_array(a))
                .and_then(|items| {
                    items.get(3).map(|d| numbers_in(self.arena, &d.resolve(self.arena)))
                })
                .filter(|d| !d.is_empty())
        };
        Pen { width, dashes }
    }

    /// `/C` as a colour operator, stroking or filling.
    pub(super) fn colour(&self, stroking: bool) -> Option<String> {
        colour_operator(&self.numbers("C"), stroking)
    }

    /// `/IC` as a filling colour operator.
    pub(super) fn interior(&self) -> Option<String> {
        colour_operator(&self.numbers("IC"), false)
    }

    /// `/RD`, the differences Tables 177, 180 and 183 put between `/Rect` and what is drawn:
    /// left, top, right, bottom.
    pub(super) fn inset(&self) -> [f64; 4] {
        match self.numbers("RD")[..] {
            [l, t, r, b] => [l.max(0.0), t.max(0.0), r.max(0.0), b.max(0.0)],
            _ => [0.0; 4],
        }
    }
}

/// The width and dashes a border or a line is drawn with.
pub(super) struct Pen {
    pub(super) width: f64,
    pub(super) dashes: Option<Vec<f64>>,
}

impl Pen {
    /// The operators that set this pen, with round joins and caps.
    pub(super) fn operators(&self) -> String {
        let dash = self.dashes.as_ref().map_or_else(String::new, |d| {
            let parts: Vec<String> = d.iter().map(|n| format!("{n:.2}")).collect();
            format!("[{}] 0 d ", parts.join(" "))
        });
        format!("{:.2} w 1 J 1 j {dash}\n", self.width)
    }
}

/// The numbers of an array object.
fn numbers_in(arena: &PdfArena, value: &Object) -> Vec<f64> {
    value
        .as_array()
        .and_then(|a| arena.get_array(a))
        .unwrap_or_default()
        .iter()
        .filter_map(|n| n.resolve(arena).as_f64())
        .collect()
}

/// A colour of 1, 3 or 4 components as `g`, `rg` or `k` (or their stroking forms); none
/// for an empty array, which Table 166 makes "transparent".
pub(super) fn colour_operator(components: &[f64], stroking: bool) -> Option<String> {
    let parts: Vec<String> =
        components.iter().map(|c| format!("{:.3}", c.clamp(0.0, 1.0))).collect();
    let operator = match (components.len(), stroking) {
        (1, true) => "G",
        (1, false) => "g",
        (3, true) => "RG",
        (3, false) => "rg",
        (4, true) => "K",
        (4, false) => "k",
        _ => return None,
    };
    Some(format!("{} {operator}\n", parts.join(" ")))
}

/// A point on the page in the annotation's own space.
pub(super) fn local(area: Area, x: f64, y: f64) -> (f64, f64) {
    (x - area.left, y - area.bottom)
}
