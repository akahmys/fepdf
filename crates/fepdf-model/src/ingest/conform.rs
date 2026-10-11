//! What a file says that ISO 32000-2 deprecates or puts elsewhere, made 2.0's at load.
//!
//! **A save of a 1.x file kept what 2.0 no longer has**, and the Arlington test read each as
//! a departure (ROADMAP Y-F28): a font's `/Name`, a descriptor's `/CIDSet` and `/CharSet`,
//! a form's 1.0 resource `/Encoding`, keys a dictionary of its kind does not have, and a
//! DCT image's `/ColorTransform` written in the image dictionary instead of its decode
//! parameters. A `/BM` written as an array or as `Compatible` is made one name, as 11.6.3
//! and Table 134 read it (ROADMAP AB-2). Each is translated here — moved where 2.0 keeps
//! it, rewritten as what it means, or dropped where 2.0 keeps nothing — and each kind is
//! recorded once a document, with how many.
//!
//! A key no table of the standard names anywhere is left alone: 7.3.7 lets a dictionary
//! carry one, and a reader ignores it. What is taken out here is what the standard names
//! and deprecates, or names for another dictionary.

use crate::arena::PdfArena;
use crate::graphics::BlendMode;
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
            (_, Some("Image")) => {
                counts.color_transform += move_color_transform(arena, holder);
                counts.image_name += drop_keys(arena, holder, &["Name"]);
            }
            (_, Some("Form")) => counts.form_name += drop_keys(arena, holder, &["Name"]),
            _ => {}
        }
        let (array, compatible) = one_blend_mode(arena, holder);
        counts.blend_array += array;
        counts.blend_compatible += compatible;
        counts.checked += spell_checked(arena, holder);
    }
    counts.revisions = drop_revision_numbers(arena);
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

/// What a save leaves out, in [`drop_for_output`]'s order: the clause, and what was found.
const LEFT_OUT: [(&str, &str); 9] = [
    (
        "14.10",
        "catalogues, pages, images or name dictionaries carry web capture's /SpiderInfo, /ID, \
         /IDS or /URLS, which 2.0 deprecates with the clause",
    ),
    ("13.5", "name dictionaries carry /AlternatePresentations, which 2.0 deprecates"),
    (
        "12.2",
        "viewer preferences carry /ViewArea, /ViewClip, /PrintArea or /PrintClip, which 2.0 \
         deprecates",
    ),
    ("14.7.1", "mark information dictionaries carry /Suspects, which 2.0 deprecates"),
    ("14.11.4", "pages carry a separation dictionary, which 2.0 deprecates"),
    ("14.11.7", "images or form XObjects carry an OPI dictionary, which 2.0 deprecates"),
    (
        "14.11.6",
        "pages carry a trap network annotation, which 2.0 deprecates with trapping; no other \
         kind means what it means",
    ),
    (
        "7.11.3",
        "file specifications carry /DOS, /Mac or /Unix beside /F or /UF, which 2.0 deprecates",
    ),
    ("13.6.3", "3D streams carry /Resources, which 2.0 deprecates"),
];

/// Takes out of a document being written what 2.0 deprecates and keeps nothing in place of
/// — the rows of `docs/specs/pdf-2-0-deprecations.md` whose rule is *save* (ROADMAP AB-4)
/// — and says how many of each went.
///
/// **On the copy a save writes, not at load**, as [`drop_subset_claims`] is: the audit
/// reads `/Suspects` and the TrapNet annotation, and `inspect` reports what the file said,
/// so the open document keeps each.
pub fn drop_for_output(arena: &PdfArena, decisions: &mut DecisionLog) {
    let mut dropped = [0usize; 9];
    for holder in arena.all_dict_handles() {
        let ty = name_of(arena, holder, "Type");
        let subtype = name_of(arena, holder, "Subtype");
        match (ty.as_deref(), subtype.as_deref()) {
            (Some("Catalog"), _) => catalogue_for_output(arena, holder, &mut dropped),
            (Some("Page"), _) => {
                dropped[0] += drop_keys(arena, holder, &["ID"]);
                dropped[4] += drop_keys(arena, holder, &["SeparationInfo"]);
                dropped[6] += drop_trap_network(arena, holder);
            }
            (_, Some("Image")) => {
                dropped[0] += drop_keys(arena, holder, &["ID"]);
                dropped[5] += drop_keys(arena, holder, &["OPI"]);
            }
            (_, Some("Form")) => dropped[5] += drop_keys(arena, holder, &["OPI"]),
            (Some("3D"), _) => dropped[8] += drop_keys(arena, holder, &["Resources"]),
            _ => dropped[7] += drop_platform_names(arena, holder),
        }
    }
    for (count, (clause, what)) in dropped.into_iter().zip(LEFT_OUT) {
        if count > 0 {
            decisions.push(Decision::repaired(
                clause,
                format!("{count} {what}"),
                "left them out of what was written; the open document keeps them",
            ));
        }
    }
}

/// The catalogue's share of [`drop_for_output`]: `/SpiderInfo`, the name dictionary's
/// `/IDS`, `/URLS` and `/AlternatePresentations`, the four page-boundary viewer
/// preferences, and `/Suspects`.
fn catalogue_for_output(arena: &PdfArena, catalogue: DictHandle, dropped: &mut [usize; 9]) {
    dropped[0] += drop_keys(arena, catalogue, &["SpiderInfo"]);
    let inner = |key: &str| {
        arena.dict_entry(catalogue, arena.name(key)).and_then(|v| v.resolve(arena).as_dict_handle())
    };
    if let Some(names) = inner("Names") {
        dropped[0] += drop_keys(arena, names, &["IDS", "URLS"]);
        dropped[1] += drop_keys(arena, names, &["AlternatePresentations"]);
    }
    if let Some(viewer) = inner("ViewerPreferences") {
        dropped[2] += drop_keys(arena, viewer, &["ViewArea", "ViewClip", "PrintArea", "PrintClip"]);
    }
    if let Some(mark_info) = inner("MarkInfo") {
        dropped[3] += drop_keys(arena, mark_info, &["Suspects"]);
    }
}

/// Takes `page`'s trap network annotation out of its `/Annots`; 1 if it had one. Nothing
/// else refers to it (12.5.6.21 puts it last in `/Annots` and nowhere else), so the writer,
/// which writes what the catalogue reaches, does not write it.
fn drop_trap_network(arena: &PdfArena, page: DictHandle) -> usize {
    let key = arena.name("Annots");
    let Some(Object::Array(array)) = arena.dict_entry(page, key).map(|a| a.resolve(arena)) else {
        return 0;
    };
    let annots = arena.get_array(array).unwrap_or_default();
    let kept: Vec<Object> = annots
        .iter()
        .filter(|a| {
            let subtype =
                a.resolve(arena).as_dict_handle().and_then(|h| name_of(arena, h, "Subtype"));
            subtype.as_deref() != Some("TrapNet")
        })
        .cloned()
        .collect();
    if kept.len() == annots.len() {
        return 0;
    }
    arena.set_array(array, kept);
    1
}

/// A file specification's `/DOS`, `/Mac` and `/Unix`, and its embedded files' `/Mac`
/// parameters, when it states `/F` or `/UF` beside them; 1 if any went.
///
/// **Only beside `/F` or `/UF`**: with neither, the platform string is all the file says,
/// and making an `/F` of it is a guess at the platform's rules. **Not in a Launch action**,
/// whose `/Mac` and `/Unix` are its own and are kept (row #20); an action says `/S`.
fn drop_platform_names(arena: &PdfArena, holder: DictHandle) -> usize {
    let has = |key: &str| arena.dict_entry(holder, arena.name(key)).is_some();
    if has("S") || !(has("F") || has("UF")) {
        return 0;
    }
    let mut dropped = drop_keys(arena, holder, &["DOS", "Mac", "Unix"]);
    let files = arena
        .dict_entry(holder, arena.name("EF"))
        .and_then(|ef| ef.resolve(arena).as_dict_handle())
        .and_then(|ef| arena.get_dict(ef))
        .unwrap_or_default();
    for stream in files.values() {
        let params = stream
            .resolve(arena)
            .as_dict_handle()
            .and_then(|s| arena.dict_entry(s, arena.name("Params")))
            .and_then(|p| p.resolve(arena).as_dict_handle());
        if let Some(params) = params {
            dropped = dropped.max(drop_keys(arena, params, &["Mac"]));
        }
    }
    dropped
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
    blend_array: usize,
    blend_compatible: usize,
    image_name: usize,
    form_name: usize,
    checked: usize,
    revisions: usize,
}

/// What each count of [`Counts`] says, in the order [`Counts::tallies`] gives them: the
/// clause, and what was found.
const SAID: [(&str, &str); 13] = [
    ("9.8.1", "font descriptors carry /Subtype, which Table 120 does not have"),
    ("9.6.2.1", "fonts carry /Name, which 2.0 deprecates, or a key their kind does not have"),
    (
        "7.4.8",
        "DCT images state /ColorTransform in the image dictionary, where Table 13 puts it in \
         the decode parameters",
    ),
    ("14.7.1", "mark information dictionaries carry a /Type Table 353 does not have"),
    ("12.2", "viewer preferences dictionaries carry a /Type Table 147 does not have"),
    ("7.8.3", "form resource dictionaries carry PDF 1.0's /Encoding, which Table 34 does not have"),
    ("12.7.3", "widgets carry /DR, which Table 224 gives the interactive form dictionary"),
    (
        "11.6.3",
        "graphics states or annotations give /BM as an array of names, deprecated in 2.0; \
         each keeps the first mode a reader recognises, as 11.6.3 has it read",
    ),
    (
        "11.3.5",
        "graphics states or annotations give /BM as Compatible, which Table 134 deprecates \
         as the same as Normal",
    ),
    ("8.9.5", "images carry /Name, which Table 87 deprecates in 2.0"),
    ("8.10.2", "form XObjects carry /Name, which Table 93 deprecates in 2.0"),
    (
        "14.8.5.6",
        "PrintField attributes spell their key `checked`, the lower case form Table 383 \
         deprecates; each is now `Checked`",
    ),
    (
        "14.7.6.3",
        "structure elements carry attribute revision numbers — an /R, or integers among their \
         /A and /C — which 2.0 deprecates",
    ),
];

impl Counts {
    /// The counts, in [`SAID`]'s order.
    const fn tallies(&self) -> [usize; 13] {
        [
            self.descriptor,
            self.font,
            self.color_transform,
            self.mark_info,
            self.viewer,
            self.form_encoding,
            self.widget_resources,
            self.blend_array,
            self.blend_compatible,
            self.image_name,
            self.form_name,
            self.checked,
            self.revisions,
        ]
    }

    fn record(&self, decisions: &mut DecisionLog) {
        for (count, (clause, what)) in self.tallies().into_iter().zip(SAID) {
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

/// Makes `holder`'s `/BM` one name: of an array, the first mode Tables 134 and 135 name,
/// or `Normal` if none (11.6.3); `Compatible` as `Normal` (Table 134). `/BM` is a key of
/// graphics state parameter dictionaries and of annotations, and of nothing else. How
/// many of each, 1 or 0.
fn one_blend_mode(arena: &PdfArena, holder: DictHandle) -> (usize, usize) {
    let key = arena.name("BM");
    let Some(value) = arena.dict_entry(holder, key).map(|v| v.resolve(arena)) else {
        return (0, 0);
    };
    let spelled = |item: &Object| item.resolve(arena).as_name().and_then(|n| arena.get_name_str(n));
    let (one, counted) = match value {
        Object::Array(array) => {
            let names = arena.get_array(array).unwrap_or_default();
            let first = names.iter().filter_map(spelled).find(|n| BlendMode::named(n).is_some());
            (first.unwrap_or_else(|| "Normal".to_string()), (1, 0))
        }
        ref name @ Object::Name(_) if spelled(name).as_deref() == Some("Compatible") => {
            ("Normal".to_string(), (0, 1))
        }
        _ => return (0, 0),
    };
    let one = if one == "Compatible" { "Normal".to_string() } else { one };
    let Some(mut dict) = arena.get_dict(holder) else { return (0, 0) };
    dict.insert(key, Object::Name(arena.name(&one)));
    arena.set_dict(holder, dict);
    counted
}

/// Spells a PrintField attribute object's `checked` as `Checked` (Table 383), or drops it
/// where `Checked` is there already, which a reader takes; 1 if it was there.
fn spell_checked(arena: &PdfArena, holder: DictHandle) -> usize {
    let lower = arena.name("checked");
    if arena.dict_entry(holder, lower).is_none()
        || name_of(arena, holder, "O").as_deref() != Some("PrintField")
    {
        return 0;
    }
    let Some(mut dict) = arena.get_dict(holder) else { return 0 };
    if let Some(value) = dict.remove(&lower) {
        dict.entry(arena.name("Checked")).or_insert(value);
    }
    arena.set_dict(holder, dict);
    1
}

/// Takes the attribute revision numbers (14.7.6.3) out of every structure element the tree
/// reaches: its `/R`, and the integer after an attribute object in `/A` or a class name in
/// `/C`. How many elements carried any.
///
/// **Walked from the tree root**, because a structure element need not say `/Type
/// /StructElem` (Table 355), and `/R` and `/C` mean other things in other dictionaries.
/// Both readers of `/A` and `/C` — the audit's `audit_tree.rs` and `apply/structure.rs` —
/// pass over an integer, so nothing that reads the tree reads differently.
fn drop_revision_numbers(arena: &PdfArena) -> usize {
    let mut seen = std::collections::BTreeSet::new();
    let mut queue: Vec<Object> = arena
        .all_dict_handles()
        .into_iter()
        .filter(|h| name_of(arena, *h, "Type").as_deref() == Some("Catalog"))
        .filter_map(|h| arena.dict_entry(h, arena.name("StructTreeRoot")))
        .filter_map(|root| entry_of(arena, &root, "K"))
        .collect();
    let mut dropped = 0;
    while let Some(node) = queue.pop() {
        match node.resolve(arena) {
            Object::Array(array) => queue.extend(arena.get_array(array).unwrap_or_default()),
            Object::Dictionary(element)
                if seen.insert(element) && name_of(arena, element, "S").is_some() =>
            {
                dropped += drop_revisions_of(arena, element);
                queue.extend(arena.dict_entry(element, arena.name("K")));
            }
            _ => {}
        }
    }
    dropped
}

/// The value at `key` of the dictionary `object` is or refers to.
fn entry_of(arena: &PdfArena, object: &Object, key: &str) -> Option<Object> {
    arena.dict_entry(object.resolve(arena).as_dict_handle()?, arena.name(key))
}

/// One element's `/R`, and the integers among its `/A` and `/C`; 1 if it had any.
fn drop_revisions_of(arena: &PdfArena, element: DictHandle) -> usize {
    let Some(mut dict) = arena.get_dict(element) else { return 0 };
    let mut changed = dict.remove(&arena.name("R")).is_some();
    for key in ["A", "C"] {
        let key = arena.name(key);
        let Some(Object::Array(array)) = dict.get(&key).map(|v| v.resolve(arena)) else {
            continue;
        };
        let items = arena.get_array(array).unwrap_or_default();
        let kept: Vec<Object> =
            items.iter().filter(|i| !matches!(i, Object::Integer(_))).cloned().collect();
        if kept.len() != items.len() {
            dict.insert(key, Object::Array(arena.alloc_array(kept)));
            changed = true;
        }
    }
    if changed {
        arena.set_dict(element, dict);
    }
    usize::from(changed)
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
