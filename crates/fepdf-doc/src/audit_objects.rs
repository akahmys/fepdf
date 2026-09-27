//! Matterhorn failure conditions decided outside the structure tree (ROADMAP W-21i).
//!
//! Optional content configurations, outlines, annotations and the XObjects a page names.
//!
//! **None of them needs a tag in the document to be asked**, so a file with no structure
//! tree is still asked them — the reason the catalogue's conditions are asked of one. Where
//! an annotation's structure element decides a condition, it is found through the parent
//! tree (W-21j), and a file with no tree has annotations that belong to nothing.

use crate::structure::{AuditFinding, broken};
use fepdf_model::{Document, Object, PdfArena};
use std::collections::BTreeSet;

/// The failure conditions this module decides.
pub const FROM_OBJECTS: [&str; 15] = [
    "11-003", "11-004", "20-001", "20-002", "20-003", "28-002", "28-004", "28-007", "28-008",
    "28-009", "28-010", "28-011", "28-012", "28-017", "30-001",
];

/// The structure element an annotation belongs to, through its `/StructParent` and the
/// parent tree (14.7.5.4), and what that element says.
struct Belonging<'a> {
    arena: &'a PdfArena,
    parents: std::collections::BTreeMap<i64, fepdf_model::Handle<Object>>,
    roles: std::collections::BTreeMap<String, String>,
}

/// How far up `/P` an element's language is looked for (Rule 6).
const ANCESTORS: usize = 256;

impl<'a> Belonging<'a> {
    fn of(doc: &'a Document) -> Self {
        let arena = doc.arena();
        let root = doc.get_structure_root().ok().flatten();
        Self {
            arena,
            parents: root.map(|r| crate::parent_tree::single_entries(arena, r)).unwrap_or_default(),
            roles: root.map(|r| crate::audit_tree::role_map(arena, r)).unwrap_or_default(),
        }
    }

    /// The element `annotation` belongs to, if the parent tree names one.
    fn element(&self, annotation: &Object) -> Option<fepdf_model::Handle<Object>> {
        let Some(Object::Integer(key)) = entry(self.arena, annotation, "StructParent") else {
            return None;
        };
        self.parents.get(&key).copied()
    }

    /// The standard type `element` stands for.
    fn kind(&self, element: fepdf_model::Handle<Object>) -> Option<String> {
        let tag = name_of(self.arena, &Object::Reference(element), "S")?;
        crate::audit_tree::standard_type(&self.roles, &tag)
    }

    /// The `/Lang` in force at `element`: its own, or the nearest ancestor's.
    fn language(&self, element: fepdf_model::Handle<Object>) -> Option<String> {
        let mut at = Some(Object::Reference(element));
        for _ in 0..ANCESTORS {
            let here = at?;
            if says_something(self.arena, &here, "Lang") {
                return match entry(self.arena, &here, "Lang") {
                    Some(Object::Text(text)) => Some(text),
                    Some(Object::String(b) | Object::Hex(b)) => {
                        Some(fepdf_model::refine::text::recover_string(&b))
                    }
                    _ => None,
                };
            }
            at = self.arena.get_object(here.as_reference()?).and_then(|o| {
                let dict = o.as_dict_handle()?;
                self.arena.dict_entry(dict, self.arena.name("P"))
            });
        }
        None
    }
}

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
    let belonging = Belonging::of(doc);
    for page in 0..pages {
        let Some(handle) = doc.get_page_handle(page) else { continue };
        let page_object = Object::Reference(handle);
        annotations(&belonging, &page_object, page + 1, language, findings);
        reference_xobjects(doc, page, findings);
    }
    examined.extend([
        "11-004", "28-002", "28-004", "28-007", "28-008", "28-009", "28-010", "28-011", "28-012",
        "28-017", "30-001",
    ]);
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
    belonging: &Belonging<'_>,
    page: &Object,
    at: usize,
    language: Option<&str>,
    findings: &mut Vec<AuditFinding>,
) {
    let arena = belonging.arena;
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
        one_annotation(belonging, annotation, at, language, findings);
        tagged_as(belonging, annotation, at, findings);
    }
}

/// The conditions about what one annotation says: its `/Contents`, and its language.
///
/// **Decided through the element the parent tree names**, where 28-004 accepts an `/Alt`
/// there in place of `/Contents` and 11-004 a `/Lang` there or above it.
fn one_annotation(
    belonging: &Belonging<'_>,
    annotation: &Object,
    at: usize,
    language: Option<&str>,
    findings: &mut Vec<AuditFinding>,
) {
    let arena = belonging.arena;
    let subtype = name_of(arena, annotation, "Subtype").unwrap_or_default();
    let has_contents = says_something(arena, annotation, "Contents");
    let element = belonging.element(annotation);
    if subtype == "TrapNet" {
        findings.push(broken("28-007", format!("Page {at} carries a /TrapNet annotation")));
    }
    if subtype == "Link" && !has_contents {
        findings.push(broken(
            "28-012",
            format!("Page {at}: a link annotation states no /Contents describing it"),
        ));
    }
    let described = element.is_some_and(|e| says_something(arena, &Object::Reference(e), "Alt"));
    if subtype != "Widget" && !has_contents && !described {
        findings.push(broken(
            "28-004",
            format!(
                "Page {at}: a /{subtype} annotation states no /Contents, and no /Alt on a \
                 structure element describes it"
            ),
        ));
    }
    let spoken = language.is_some() || element.and_then(|e| belonging.language(e)).is_some();
    if has_contents && !spoken {
        findings.push(broken(
            "11-004",
            format!(
                "Page {at}: a /{subtype} annotation's /Contents is in no language — neither the \
                 catalogue nor its structure element states a /Lang"
            ),
        ));
    }
}

/// 28-002, 28-010, 28-011 and 28-017: the structure element an annotation belongs to.
fn tagged_as(
    belonging: &Belonging<'_>,
    annotation: &Object,
    at: usize,
    findings: &mut Vec<AuditFinding>,
) {
    let subtype = name_of(belonging.arena, annotation, "Subtype").unwrap_or_default();
    let element = belonging.element(annotation);
    let kind = element.and_then(|e| belonging.kind(e));
    let (condition, wanted) = match subtype.as_str() {
        "PrinterMark" => {
            if element.is_some() {
                findings.push(broken(
                    "28-017",
                    format!("Page {at}: a /PrinterMark annotation is in the logical structure"),
                ));
            }
            return;
        }
        "Widget" => ("28-010", "Form"),
        "Link" => ("28-011", "Link"),
        _ => ("28-002", "Annot"),
    };
    if kind.as_deref() != Some(wanted) {
        let found = kind.map_or_else(
            || "belongs to no structure element".to_owned(),
            |k| format!("belongs to a <{k}>"),
        );
        findings.push(broken(
            condition,
            format!("Page {at}: a /{subtype} annotation {found}, where it belongs in a <{wanted}>"),
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
