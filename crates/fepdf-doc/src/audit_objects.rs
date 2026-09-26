//! Matterhorn failure conditions decided outside the structure tree (ROADMAP W-21i).
//!
//! Optional content configurations, outlines, annotations and the XObjects a page names.
//!
//! **None of them needs a tag in the document**, so a file with no structure tree is still
//! asked them — the reason the catalogue's conditions are asked of one.

use crate::structure::{AuditFinding, broken, for_a_reader};
use fepdf_model::{Document, Object, PdfArena};
use std::collections::BTreeSet;

/// The failure conditions this module decides.
pub const FROM_OBJECTS: [&str; 11] = [
    "11-003", "11-004", "20-001", "20-002", "20-003", "28-004", "28-007", "28-008", "28-009",
    "28-012", "30-001",
];

/// A dictionary entry, resolved.
fn entry(arena: &PdfArena, dict: &Object, key: &str) -> Option<Object> {
    let handle = dict.resolve(arena).as_dict_handle()?;
    arena.dict_entry(handle, arena.name(key)).map(|value| value.resolve(arena))
}

/// A name entry, as its text.
fn name_of(arena: &PdfArena, dict: &Object, key: &str) -> Option<String> {
    let name = entry(arena, dict, key)?.as_name()?;
    arena.get_name(name).map(|n| n.as_str().to_string())
}

/// Whether a text-string entry is there and says something.
fn says_something(arena: &PdfArena, dict: &Object, key: &str) -> bool {
    match entry(arena, dict, key) {
        Some(Object::Text(text)) => !text.trim().is_empty(),
        Some(Object::String(bytes) | Object::Hex(bytes)) => {
            !fepdf_model::refine::text::recover_string(&bytes).trim().is_empty()
        }
        _ => false,
    }
}

/// An array entry's items, resolved; nothing when it is not an array.
fn items(arena: &PdfArena, dict: &Object, key: &str) -> Vec<Object> {
    match entry(arena, dict, key) {
        Some(Object::Array(array)) => arena
            .get_array(array)
            .unwrap_or_default()
            .iter()
            .map(|item| item.resolve(arena))
            .collect(),
        _ => Vec::new(),
    }
}

/// Asks every condition in [`FROM_OBJECTS`] of `doc`, whose catalogue's `/Lang` is
/// `language`.
pub fn audit_objects(
    doc: &Document,
    language: Option<&str>,
    findings: &mut Vec<AuditFinding>,
    examined: &mut BTreeSet<&'static str>,
) {
    let Some(catalogue) = doc.catalog_handle().map(Object::Reference) else { return };
    let arena = doc.arena();
    optional_content(arena, &catalogue, findings);
    outlines(arena, &catalogue, language, findings);
    examined.extend(["11-003", "20-001", "20-002", "20-003"]);
    let Ok(pages) = doc.page_count() else { return };
    for page in 0..pages {
        let Some(handle) = doc.get_page_handle(page) else { continue };
        let page_object = Object::Reference(handle);
        annotations(arena, &page_object, page + 1, language, findings);
        reference_xobjects(doc, page, findings);
    }
    examined.extend(["11-004", "28-004", "28-007", "28-008", "28-009", "28-012", "30-001"]);
}

/// Checkpoint 20: every optional content configuration named, and none carrying `/AS`
/// (UA1:7.10).
fn optional_content(arena: &PdfArena, catalogue: &Object, findings: &mut Vec<AuditFinding>) {
    let Some(properties) = entry(arena, catalogue, "OCProperties") else { return };
    let mut configurations = Vec::new();
    if let Some(default) = entry(arena, &properties, "D") {
        if !says_something(arena, &default, "Name") {
            findings.push(broken(
                "20-002",
                "The default optional content configuration (/OCProperties /D) has no /Name",
            ));
        }
        configurations.push(default);
    }
    for (index, configuration) in items(arena, &properties, "Configs").into_iter().enumerate() {
        if !says_something(arena, &configuration, "Name") {
            findings.push(broken(
                "20-001",
                format!("Optional content configuration {} of /Configs has no /Name", index + 1),
            ));
        }
        configurations.push(configuration);
    }
    if configurations.iter().any(|c| entry(arena, c, "AS").is_some()) {
        findings.push(broken(
            "20-003",
            "An optional content configuration carries /AS, so a layer's visibility changes \
             with the viewing, printing or exporting the reader does",
        ));
    }
}

/// 11-003: outline titles are in the document's language, which must be stated —
/// an outline item has no `/Lang` of its own.
fn outlines(
    arena: &PdfArena,
    catalogue: &Object,
    language: Option<&str>,
    findings: &mut Vec<AuditFinding>,
) {
    let has_items = entry(arena, catalogue, "Outlines")
        .is_some_and(|outlines| entry(arena, &outlines, "First").is_some());
    if has_items && language.is_none() {
        findings.push(broken(
            "11-003",
            "The document has outline entries and states no /Lang in its catalogue, so the \
             language of their titles cannot be determined",
        ));
    }
}

/// Checkpoint 28's conditions about one page's annotations, and 11-004.
fn annotations(
    arena: &PdfArena,
    page: &Object,
    at: usize,
    language: Option<&str>,
    findings: &mut Vec<AuditFinding>,
) {
    let annotations = items(arena, page, "Annots");
    if annotations.is_empty() {
        return;
    }
    match name_of(arena, page, "Tabs").as_deref() {
        None => findings.push(broken(
            "28-008",
            format!("Page {at} has annotations and no /Tabs, so the order Tab takes is unstated"),
        )),
        Some("S") => {}
        Some(other) => findings.push(broken(
            "28-009",
            format!("Page {at} has annotations and /Tabs /{other}, where UA-1 wants /S"),
        )),
    }
    for annotation in &annotations {
        one_annotation(arena, annotation, at, language, findings);
    }
}

/// The conditions about one annotation.
///
/// **Left for a reader where the enclosing structure element would decide it**: 28-004
/// accepts an `/Alt` there in place of `/Contents`, and 11-004 a `/Lang` there, and the
/// element is reached through the page's `/StructParents` and the parent tree, which is
/// not followed here.
fn one_annotation(
    arena: &PdfArena,
    annotation: &Object,
    at: usize,
    language: Option<&str>,
    findings: &mut Vec<AuditFinding>,
) {
    let subtype = name_of(arena, annotation, "Subtype").unwrap_or_default();
    let has_contents = says_something(arena, annotation, "Contents");
    if subtype == "TrapNet" {
        findings.push(broken("28-007", format!("Page {at} carries a /TrapNet annotation")));
    }
    if subtype == "Link" && !has_contents {
        findings.push(broken(
            "28-012",
            format!("Page {at}: a link annotation states no /Contents describing it"),
        ));
    }
    if subtype != "Widget" && !has_contents {
        findings.push(for_a_reader(
            "28-004",
            format!(
                "Page {at}: a /{subtype} annotation states no /Contents. Whether an /Alt on \
                 its structure element describes it instead is not resolved here — look at it"
            ),
        ));
    }
    if has_contents && language.is_none() {
        findings.push(for_a_reader(
            "11-004",
            format!(
                "Page {at}: a /{subtype} annotation's /Contents is in no language the \
                 catalogue states. Whether its structure element states a /Lang is not \
                 resolved here — look at it"
            ),
        ));
    }
}

/// 30-001: a reference XObject — a form carrying `/Ref` — among the ones the page names
/// (UA1:7.20). The page's own resources; a form's resources are not descended into.
fn reference_xobjects(doc: &Document, page: usize, findings: &mut Vec<AuditFinding>) {
    let arena = doc.arena();
    let Some(page_handle) = doc.get_page_handle(page) else { return };
    let resources = fepdf_model::Page::new(arena, page_handle, doc.get_parent_chain(page_handle))
        .resources_handle();
    let Some(xobjects) = arena
        .dict_entry(resources, arena.name("XObject"))
        .and_then(|x| x.resolve(arena).as_dict_handle())
        .and_then(|x| arena.get_dict(x))
    else {
        return;
    };
    for (key, value) in xobjects {
        let Some(Object::Stream(dict, _)) = value.as_reference().and_then(|h| arena.get_object(h))
        else {
            continue;
        };
        if arena.dict_entry(dict, arena.name("Ref")).is_some() {
            let name = arena.get_name(key).map(|n| n.as_str().to_string()).unwrap_or_default();
            findings.push(broken(
                "30-001",
                format!("Page {}: /{name} is a reference XObject (/Ref)", page + 1),
            ));
        }
    }
}
