//! The replacement text of structure elements whose content a redaction touched, made a
//! marker (14.9.3, 14.9.4, ROADMAP Y-10).
//!
//! **An element's `/ActualText` and `/Alt` read its content again**, and an ancestor's
//! read its descendants'. Where a redaction took any of an element's marks, what they
//! say may say what was taken, so the element's and every ancestor's become a marker —
//! as iText pdfSweep 5.0.8 does, decided by the owner 2026-10-03.

use fepdf_model::{DictHandle, Handle, Object, PdfArena};
use std::collections::BTreeSet;

/// How deep the walk goes (Rule 6).
const DEPTH: usize = 256;

/// The keys that carry what content reads, or stands for.
const SAYING: [&str; 3] = ["ActualText", "Alt", "E"];

/// Makes `marker` the replacement text of every element under `root` that holds one of
/// `marks` — each a page and an MCID — and of each of its ancestors.
pub fn mark_alternates(
    arena: &PdfArena,
    root: Handle<Object>,
    marks: &BTreeSet<(Handle<Object>, i64)>,
    marker: &str,
) {
    let mut touched = BTreeSet::new();
    let mut seen = BTreeSet::new();
    visit(arena, root, None, (marks, &mut Vec::new()), (&mut touched, &mut seen), 0);
    for element in touched {
        let Some(mut dict) = arena.get_dict(element) else { continue };
        let mut changed = false;
        for key in SAYING {
            if let Some(value) = dict.get_mut(&arena.name(key)) {
                *value = Object::String(bytes::Bytes::copy_from_slice(marker.as_bytes()));
                changed = true;
            }
        }
        if changed {
            arena.set_dict(element, dict);
        }
    }
}

/// Visits `node` on `page` unless it names its own, adding it and `ancestors` to
/// `touched` where it holds one of `marks`.
fn visit(
    arena: &PdfArena,
    node: Handle<Object>,
    page: Option<Handle<Object>>,
    (marks, ancestors): (&BTreeSet<(Handle<Object>, i64)>, &mut Vec<DictHandle>),
    (touched, seen): (&mut BTreeSet<DictHandle>, &mut BTreeSet<Handle<Object>>),
    depth: usize,
) {
    if depth >= DEPTH || !seen.insert(node) {
        return;
    }
    let Some(dict) = arena.get_object(node).and_then(|o| o.as_dict_handle()) else { return };
    let page = arena.dict_entry(dict, arena.name("Pg")).and_then(|p| p.as_reference()).or(page);
    let kids = match arena.dict_entry(dict, arena.name("K")).map(|k| k.resolve(arena)) {
        Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
        Some(_) => arena.dict_entry(dict, arena.name("K")).into_iter().collect(),
        None => Vec::new(),
    };
    ancestors.push(dict);
    for kid in kids {
        if holds_mark(arena, &kid, page, marks) {
            touched.extend(ancestors.iter().copied());
        } else if let Some(element) = kid.as_reference() {
            visit(arena, element, page, (marks, ancestors), (touched, seen), depth + 1);
        }
    }
    ancestors.pop();
}

/// Whether `kid`, under an element on `page`, is one of `marks`: an MCID, or a marked
/// content reference to one on its page and not in a form's stream.
fn holds_mark(
    arena: &PdfArena,
    kid: &Object,
    page: Option<Handle<Object>>,
    marks: &BTreeSet<(Handle<Object>, i64)>,
) -> bool {
    match kid.resolve(arena) {
        Object::Integer(mcid) => page.is_some_and(|p| marks.contains(&(p, mcid))),
        Object::Dictionary(dict) => {
            let entry = |key: &str| arena.dict_entry(dict, arena.name(key));
            let is_reference = entry("Type").and_then(|t| t.as_name()) == Some(arena.name("MCR"));
            let own = entry("Pg").and_then(|p| p.as_reference()).or(page);
            let mcid = entry("MCID").and_then(|m| m.as_integer());
            is_reference
                && entry("Stm").is_none()
                && own.zip(mcid).is_some_and(|mark| marks.contains(&mark))
        }
        _ => false,
    }
}
