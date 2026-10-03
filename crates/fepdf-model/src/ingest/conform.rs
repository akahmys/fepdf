//! What a file says that ISO 32000-2 deprecates or puts elsewhere, made 2.0's at load.
//!
//! **A save of a 1.x file kept what 2.0 no longer has**, and the Arlington test read each as
//! a departure (ROADMAP Y-F28): a font's `/Name`, a descriptor's `/CIDSet` and `/CharSet`,
//! a form's 1.0 resource `/Encoding`, keys a dictionary of its kind does not have, and a
//! DCT image's `/ColorTransform` written in the image dictionary instead of its decode
//! parameters. Each is translated here — moved where 2.0 keeps it, or dropped where 2.0
//! keeps nothing — and each kind is recorded once a document, with how many.
//!
//! A key no table of the standard names anywhere is left alone: 7.3.7 lets a dictionary
//! carry one, and a reader ignores it. What is taken out here is what the standard names
//! and deprecates, or names for another dictionary.

use crate::arena::PdfArena;
use crate::handle::DictHandle;
use crate::interpretation::{Decision, DecisionLog};
use crate::object::Object;

/// Translates every dictionary of the document, as the module says.
pub fn conform_to_2_0(arena: &PdfArena, decisions: &mut DecisionLog) {
    let mut counts = Counts::default();
    for holder in arena.all_dict_handles() {
        let ty = name_of(arena, holder, "Type");
        let subtype = name_of(arena, holder, "Subtype");
        match (ty.as_deref(), subtype.as_deref()) {
            (Some("FontDescriptor"), _) => {
                counts.descriptor += drop_keys(arena, holder, &["Subtype"]);
            }
            (Some("Font"), Some("Type0")) => counts.font += drop_keys(arena, holder, &["Name"]),
            (Some("Font"), Some("Type3")) => {
                counts.font += drop_keys(arena, holder, &["Name", "CIDToGIDMap"]);
            }
            (Some("Font"), Some("Type1" | "MMType1" | "TrueType")) => {
                counts.font += drop_keys(arena, holder, &["Name"]);
            }
            (_, Some("Image")) => counts.color_transform += move_color_transform(arena, holder),
            _ => {}
        }
    }
    (counts.mark_info, counts.viewer) = untyped_catalog_entries(arena);
    (counts.form_encoding, counts.widget_resources) = form_resources(arena);
    counts.record(decisions);
}

/// Takes a descriptor's `/CharSet` and `/CIDSet` out of a document being written, and says
/// how many descriptors carried one.
///
/// **On the copy a save writes, not at load**: 2.0 deprecates both, and the audit asks
/// 31-012 to 31-015 of exactly these claims against the programs they describe (ISO
/// 14289-1 7.21.4.2), which it could not do once loading had taken them away. So the open
/// document keeps what the file claimed, and what is written is 2.0's.
pub fn drop_subset_claims(arena: &PdfArena, decisions: &mut DecisionLog) {
    let mut dropped = 0;
    for holder in arena.all_dict_handles() {
        if name_of(arena, holder, "Type").as_deref() == Some("FontDescriptor") {
            dropped += drop_keys(arena, holder, &["CIDSet", "CharSet"]);
        }
    }
    if dropped > 0 {
        decisions.push(Decision::repaired(
            "9.8.1",
            format!("{dropped} font descriptors claim a subset by /CharSet or /CIDSet, both deprecated in 2.0"),
            "left them out of what was written; the open document keeps them for the audit",
        ));
    }
}

/// How many of each translation were made.
#[derive(Default)]
struct Counts {
    descriptor: usize,
    font: usize,
    color_transform: usize,
    mark_info: usize,
    viewer: usize,
    form_encoding: usize,
    widget_resources: usize,
}

impl Counts {
    fn record(&self, decisions: &mut DecisionLog) {
        let said = [
            (
                self.descriptor,
                "9.8.1",
                "font descriptors carry /Subtype, which Table 120 does not have",
            ),
            (
                self.font,
                "9.6.2.1",
                "fonts carry /Name, which 2.0 deprecates, or a key their kind does not have",
            ),
            (
                self.color_transform,
                "7.4.8",
                "DCT images state /ColorTransform in the image dictionary, where Table 13 puts it in the decode parameters",
            ),
            (
                self.mark_info,
                "14.7.1",
                "mark information dictionaries carry a /Type Table 353 does not have",
            ),
            (
                self.viewer,
                "12.2",
                "viewer preferences dictionaries carry a /Type Table 147 does not have",
            ),
            (
                self.form_encoding,
                "7.8.3",
                "form resource dictionaries carry PDF 1.0's /Encoding, which Table 34 does not have",
            ),
            (
                self.widget_resources,
                "12.7.3",
                "widgets carry /DR, which Table 224 gives the interactive form dictionary",
            ),
        ];
        for (count, clause, what) in said {
            if count > 0 {
                decisions.push(Decision::repaired(
                    clause,
                    format!("{count} {what}"),
                    "made them 2.0's: moved what 2.0 keeps elsewhere, dropped what it does not keep",
                ));
            }
        }
    }
}

/// The name at `key` of the dictionary `holder`.
fn name_of(arena: &PdfArena, holder: DictHandle, key: &str) -> Option<String> {
    arena
        .dict_entry(holder, arena.name(key))
        .and_then(|v| v.resolve(arena).as_name())
        .and_then(|n| arena.get_name_str(n))
}

/// Takes `keys` out of `holder`; 1 if any was there, else 0.
fn drop_keys(arena: &PdfArena, holder: DictHandle, keys: &[&str]) -> usize {
    let names: Vec<_> = keys.iter().filter_map(|k| arena.get_name_by_str(k)).collect();
    if !names.iter().any(|n| arena.dict_entry(holder, *n).is_some()) {
        return 0;
    }
    let Some(mut dict) = arena.get_dict(holder) else { return 0 };
    for name in names {
        dict.remove(&name);
    }
    arena.set_dict(holder, dict);
    1
}

/// Moves an image dictionary's `/ColorTransform` into the `/DecodeParms` of its
/// `DCTDecode` filter (Table 13), or drops it when the image has no such filter, since then
/// nothing reads it; 1 if it was there.
fn move_color_transform(arena: &PdfArena, image: DictHandle) -> usize {
    let key = arena.name("ColorTransform");
    let Some(mut dict) = arena.get_dict(image) else { return 0 };
    let Some(transform) = dict.remove(&key) else { return 0 };
    let filters = match dict.get(&arena.name("Filter")).map(|f| f.resolve(arena)) {
        Some(Object::Array(a)) => arena.get_array(a).unwrap_or_default(),
        Some(single) => vec![single],
        None => Vec::new(),
    };
    let dct = filters.iter().position(|f| {
        f.resolve(arena).as_name().and_then(|n| arena.get_name_str(n)).as_deref()
            == Some("DCTDecode")
    });
    if let Some(at) = dct {
        let params_key = arena.name("DecodeParms");
        let params = with_transform(arena, dict.get(&params_key), at, filters.len(), transform);
        dict.insert(params_key, params);
    }
    arena.set_dict(image, dict);
    1
}

/// `params` — one dictionary, or an array of one a filter — with `/ColorTransform`
/// set in the one for the filter at `at`, unless it states its own.
fn with_transform(
    arena: &PdfArena,
    params: Option<&Object>,
    at: usize,
    filters: usize,
    transform: Object,
) -> Object {
    let key = arena.name("ColorTransform");
    let set = |existing: Option<Object>| -> Object {
        let mut d = existing
            .and_then(|e| e.resolve(arena).as_dict_handle())
            .and_then(|h| arena.get_dict(h))
            .unwrap_or_default();
        d.entry(key).or_insert(transform.clone());
        Object::Dictionary(arena.alloc_dict(d))
    };
    match params.map(|p| p.resolve(arena)) {
        Some(Object::Array(a)) => {
            let mut items = arena.get_array(a).unwrap_or_default();
            items.resize(filters.max(items.len()), Object::Null);
            let current = items.get(at).cloned().filter(|o| !matches!(o, Object::Null));
            if let Some(slot) = items.get_mut(at) {
                *slot = set(current);
            }
            Object::Array(arena.alloc_array(items))
        }
        other if filters <= 1 => set(other),
        other => {
            let mut items = vec![Object::Null; filters];
            if let Some(slot) = items.get_mut(at) {
                *slot = set(other);
            }
            Object::Array(arena.alloc_array(items))
        }
    }
}

/// Drops the `/Type` that `/MarkInfo` and `/ViewerPreferences` dictionaries carry, which
/// neither of their tables (Tables 353 and 147) has; how many of each went.
fn untyped_catalog_entries(arena: &PdfArena) -> (usize, usize) {
    let mut dropped = (0, 0);
    for holder in arena.all_dict_handles() {
        if name_of(arena, holder, "Type").as_deref() != Some("Catalog") {
            continue;
        }
        let typed = |key: &str| {
            arena
                .dict_entry(holder, arena.name(key))
                .and_then(|e| e.resolve(arena).as_dict_handle())
        };
        if let Some(dh) = typed("MarkInfo") {
            dropped.0 += drop_keys(arena, dh, &["Type"]);
        }
        if let Some(dh) = typed("ViewerPreferences") {
            dropped.1 += drop_keys(arena, dh, &["Type"]);
        }
    }
    dropped
}

/// The interactive form's default resources: a PDF 1.0 `/Encoding` dropped from them, and
/// a widget's own `/DR` — which Table 224 gives the form, not a field or a widget — merged
/// into the form's and taken off the widget. How many of each.
fn form_resources(arena: &PdfArena) -> (usize, usize) {
    let mut counts = (0, 0);
    let Some(form) = arena.all_dict_handles().into_iter().find_map(|h| {
        (name_of(arena, h, "Type").as_deref() == Some("Catalog"))
            .then(|| arena.dict_entry(h, arena.name("AcroForm")))
            .flatten()
            .and_then(|f| f.resolve(arena).as_dict_handle())
    }) else {
        return counts;
    };
    for holder in arena.all_dict_handles() {
        if name_of(arena, holder, "Subtype").as_deref() != Some("Widget") {
            continue;
        }
        let Some(own) = arena.dict_entry(holder, arena.name("DR")) else { continue };
        merge_resources(arena, form, &own);
        counts.1 += drop_keys(arena, holder, &["DR"]);
    }
    if let Some(dr) =
        arena.dict_entry(form, arena.name("DR")).and_then(|d| d.resolve(arena).as_dict_handle())
    {
        counts.0 += drop_keys(arena, dr, &["Encoding"]);
    }
    counts
}

/// Adds to `form`'s `/DR` each resource of `own` it does not name, category by category.
fn merge_resources(arena: &PdfArena, form: DictHandle, own: &Object) {
    let Some(own) = own.resolve(arena).as_dict_handle().and_then(|h| arena.get_dict(h)) else {
        return;
    };
    let key = arena.name("DR");
    let mut dr = arena
        .dict_entry(form, key)
        .and_then(|d| d.resolve(arena).as_dict_handle())
        .and_then(|h| arena.get_dict(h))
        .unwrap_or_default();
    for (category, entries) in own {
        let Some(entries) = entries.resolve(arena).as_dict_handle().and_then(|h| arena.get_dict(h))
        else {
            continue;
        };
        let mut have = dr
            .get(&category)
            .and_then(|c| c.resolve(arena).as_dict_handle())
            .and_then(|h| arena.get_dict(h))
            .unwrap_or_default();
        for (name, value) in entries {
            have.entry(name).or_insert(value);
        }
        dr.insert(category, Object::Dictionary(arena.alloc_dict(have)));
    }
    let Some(mut form_dict) = arena.get_dict(form) else { return };
    form_dict.insert(key, Object::Dictionary(arena.alloc_dict(dr)));
    arena.set_dict(form, form_dict);
}
