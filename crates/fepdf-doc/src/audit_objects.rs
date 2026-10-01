//! Matterhorn failure conditions decided outside the structure tree (ROADMAP W-21i).
//!
//! Optional content configurations, outlines, annotations and the XObjects a page names.
//!
//! **None of them needs a tag in the document to be asked**, so a file with no structure
//! tree is still asked them — the reason the catalogue's conditions are asked of one. Where
//! an annotation's structure element decides a condition, it is found through the parent
//! tree (W-21j), and a file with no tree has annotations that belong to nothing.

use crate::structure::{AuditFinding, broken, for_a_reader};
use fepdf_model::access::{entry, items, name_in};
use fepdf_model::{Document, Object, PdfArena};
use std::collections::BTreeSet;

/// The failure conditions this module decides.
pub const FROM_OBJECTS: [&str; 18] = [
    "11-003", "11-004", "11-006", "20-001", "20-002", "20-003", "28-002", "28-004", "28-006",
    "28-007", "28-008", "28-009", "28-010", "28-011", "28-012", "28-017", "28-018", "30-001",
];

/// The annotation subtypes ISO 32000-1:2008 defines (Table 169). 28-006 is about the rest.
const DEFINED_SUBTYPES: [&str; 26] = [
    "Text",
    "Link",
    "FreeText",
    "Line",
    "Square",
    "Circle",
    "Polygon",
    "PolyLine",
    "Highlight",
    "Underline",
    "Squiggly",
    "StrikeOut",
    "Stamp",
    "Caret",
    "Ink",
    "Popup",
    "FileAttachment",
    "Sound",
    "Movie",
    "Widget",
    "Screen",
    "PrinterMark",
    "TrapNet",
    "Watermark",
    "3D",
    "Redact",
];

/// The structure element an annotation belongs to, through its `/StructParent` and the
/// parent tree (14.7.5.4), and what that element says.
struct Belonging<'a> {
    doc: &'a Document,
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
            doc,
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
        let tag = name_in(self.arena, &Object::Reference(element), "S")?;
        crate::audit_tree::standard_type(&self.roles, &tag)
    }

    /// Whether the nearest `/Lang` at or above `element` states a language: `Some(false)`
    /// where it is empty, which says the language is unknown (14.9.2.2) and is not stepped
    /// over, since an element inherits only when it has no `/Lang` (14.9.2.3); `None`
    /// where no element on the way up has one, and the catalogue's decides.
    fn language(&self, element: fepdf_model::Handle<Object>) -> Option<bool> {
        let mut at = Some(Object::Reference(element));
        for _ in 0..ANCESTORS {
            let here = at?;
            if entry(self.arena, &here, "Lang").is_some() {
                return Some(says_something(self.arena, &here, "Lang"));
            }
            at = self.arena.get_object(here.as_reference()?).and_then(|o| {
                let dict = o.as_dict_handle()?;
                self.arena.dict_entry(dict, self.arena.name("P"))
            });
        }
        None
    }
}

/// Whether a text-string entry is there and says something.
pub(crate) fn says_something(arena: &PdfArena, dict: &Object, key: &str) -> bool {
    match entry(arena, dict, key) {
        Some(Object::Text(text)) => !text.trim().is_empty(),
        Some(Object::String(bytes) | Object::Hex(bytes)) => {
            !fepdf_model::refine::text::recover_string(&bytes).trim().is_empty()
        }
        _ => false,
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
    metadata_language(language, findings);
    examined.extend(["11-003", "11-006", "20-001", "20-002", "20-003"]);
    let Ok(pages) = doc.page_count() else { return };
    let belonging = Belonging::of(doc);
    for page in 0..pages {
        let Some(handle) = doc.get_page_handle(page) else { continue };
        let page_object = Object::Reference(handle);
        annotations(&belonging, &page_object, page + 1, language, findings);
        reference_xobjects(doc, page, findings);
    }
    examined.extend([
        "11-004", "28-002", "28-004", "28-006", "28-007", "28-008", "28-009", "28-010", "28-011",
        "28-012", "28-017", "28-018", "30-001",
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

/// 11-006: the language of the document's metadata is the catalogue's `/Lang`.
///
/// **Without one it is left for a reader, not called broken.** The packet could state its
/// own — an `xml:lang` alternative of `dc:title` — but ingestion rebuilds the packet from
/// the values it settled (`metadata::settle`), and the alternatives do not survive, so
/// the document read here cannot say what the file's packet said
/// ([ADR-0094](../../../docs/adr/0094-the-auditor-reads-the-ingested-document-so-ingestion-answers-checkpoint-06.md)).
fn metadata_language(language: Option<&str>, findings: &mut Vec<AuditFinding>) {
    if language.is_none() {
        findings.push(for_a_reader(
            "11-006",
            "The catalogue states no /Lang, so the language of the document's metadata is \
             whatever its packet said — which ingestion rewrites, so look at the file's own",
        ));
    }
}

/// The rectangle an array of four numbers states, with its corners put in order.
fn rectangle(arena: &PdfArena, object: Option<Object>) -> Option<[f64; 4]> {
    let Some(Object::Array(array)) = object.map(|o| o.resolve(arena)) else { return None };
    let numbers: Vec<f64> = arena
        .get_array(array)
        .unwrap_or_default()
        .iter()
        .filter_map(|n| n.resolve(arena).as_f64())
        .collect();
    let [x1, y1, x2, y2] = numbers[..] else { return None };
    Some([x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)])
}

/// Whether 7.18.1 applies to `annotation` on `page`: it does not to a `/Popup`, to one
/// whose Hidden flag is set, or to one whose rectangle lies outside the crop box.
fn subject_to_7_18_1(belonging: &Belonging<'_>, page: &Object, annotation: &Object) -> bool {
    let arena = belonging.arena;
    if name_in(arena, annotation, "Subtype").as_deref() == Some("Popup") {
        return false;
    }
    #[allow(clippy::cast_possible_truncation)] // a flag word, written as an integer
    let hidden =
        entry(arena, annotation, "F").and_then(|f| f.as_f64()).is_some_and(|f| (f as i64) & 2 != 0);
    let view = page
        .as_reference()
        .map(|h| fepdf_model::Page::new(arena, h, belonging.doc.get_parent_chain(h)));
    let crop = view.as_ref().and_then(|v| {
        rectangle(arena, v.get_attribute("CropBox"))
            .or_else(|| rectangle(arena, v.get_attribute("MediaBox")))
    });
    let outside = match (crop, rectangle(arena, entry(arena, annotation, "Rect"))) {
        (Some([cx1, cy1, cx2, cy2]), Some([x1, y1, x2, y2])) => {
            cx1 >= x2 || cy1 >= y2 || cx2 <= x1 || cy2 <= y1
        }
        _ => false,
    };
    !hidden && !outside
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
    match name_in(arena, page, "Tabs").as_deref() {
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
        let within = subject_to_7_18_1(belonging, page, annotation);
        let broke = one_annotation(belonging, annotation, at, language, within, findings)
            | tagged_as(belonging, annotation, at, within, findings);
        let subtype = name_in(belonging.arena, annotation, "Subtype").unwrap_or_default();
        if broke && !DEFINED_SUBTYPES.contains(&subtype.as_str()) {
            findings.push(broken(
                "28-006",
                format!(
                    "Page {at}: a /{subtype} annotation, a subtype ISO 32000-1 does not \
                     define, does not meet 7.18.1"
                ),
            ));
        }
    }
}

/// The conditions about what one annotation says: its `/Contents`, and its language.
///
/// **Decided through the element the parent tree names**, where 28-004 accepts an `/Alt`
/// there in place of `/Contents` and 11-004 a `/Lang` there or above it. Answers whether
/// it broke 28-004, the one of these 7.18.1 states — and which an annotation 7.18.1 does
/// not apply to (`within` false) is not asked.
fn one_annotation(
    belonging: &Belonging<'_>,
    annotation: &Object,
    at: usize,
    language: Option<&str>,
    within: bool,
    findings: &mut Vec<AuditFinding>,
) -> bool {
    let arena = belonging.arena;
    let subtype = name_in(arena, annotation, "Subtype").unwrap_or_default();
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
    let undescribed = within && subtype != "Widget" && !has_contents && !described;
    if undescribed {
        findings.push(broken(
            "28-004",
            format!(
                "Page {at}: a /{subtype} annotation states no /Contents, and no /Alt on a \
                 structure element describes it"
            ),
        ));
    }
    // The element's own `/Lang`, or its ancestors', comes before the catalogue's (14.9.2.3).
    let spoken = element.and_then(|e| belonging.language(e)).unwrap_or(language.is_some());
    if has_contents && !spoken {
        findings.push(broken(
            "11-004",
            format!(
                "Page {at}: a /{subtype} annotation's /Contents is in no language — neither the \
                 catalogue nor its structure element states a /Lang"
            ),
        ));
    }
    undescribed
}

/// 28-002, 28-010, 28-011 and 28-017: the structure element an annotation belongs to,
/// and 28-018 of a printer's mark. Answers whether it broke 28-002, which 7.18.1 states
/// and so is not asked of an annotation it does not apply to.
fn tagged_as(
    belonging: &Belonging<'_>,
    annotation: &Object,
    at: usize,
    within: bool,
    findings: &mut Vec<AuditFinding>,
) -> bool {
    let subtype = name_in(belonging.arena, annotation, "Subtype").unwrap_or_default();
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
            printer_mark_appearance(belonging, annotation, at, findings);
            return false;
        }
        "Widget" => ("28-010", "Form"),
        "Link" => ("28-011", "Link"),
        _ if !within => return false,
        _ => ("28-002", "Annot"),
    };
    let wrong = kind.as_deref() != Some(wanted);
    if wrong {
        let found = kind.map_or_else(
            || "belongs to no structure element".to_owned(),
            |k| format!("belongs to a <{k}>"),
        );
        findings.push(broken(
            condition,
            format!("Page {at}: a /{subtype} annotation {found}, where it belongs in a <{wanted}>"),
        ));
    }
    wrong && condition == "28-002"
}

/// 28-018 (UA1:7.18.8): every appearance stream of a printer's mark marks what it paints
/// as an `/Artifact` — each stream in `/AP`, and each state of one given by states.
fn printer_mark_appearance(
    belonging: &Belonging<'_>,
    annotation: &Object,
    at: usize,
    findings: &mut Vec<AuditFinding>,
) {
    let arena = belonging.arena;
    let Some(appearances) = entry(arena, annotation, "AP") else { return };
    let mut streams = Vec::new();
    for key in ["N", "R", "D"] {
        let Some(value) =
            appearances.as_dict_handle().and_then(|d| arena.dict_entry(d, arena.name(key)))
        else {
            continue;
        };
        // A stream is reached by reference; a states dictionary is as often written in
        // place, and only the one reached by reference was looked into.
        let reached = match value.as_reference() {
            Some(handle) => arena.get_object(handle).map(|o| (Some(handle), o)),
            None => Some((None, value)),
        };
        match reached {
            Some((Some(handle), Object::Stream(..))) => streams.push(handle),
            Some((_, Object::Dictionary(states))) => streams.extend(
                arena
                    .get_dict(states)
                    .unwrap_or_default()
                    .values()
                    .filter_map(Object::as_reference),
            ),
            _ => {}
        }
    }
    let unmarked = streams.iter().any(|stream| {
        crate::audit_fonts::form_commands(
            belonging.doc,
            *stream,
            &std::collections::BTreeMap::new(),
        )
        .is_some_and(|commands| paints_outside_an_artifact(&commands))
    });
    if unmarked {
        findings.push(broken(
            "28-018",
            format!(
                "Page {at}: a /PrinterMark annotation's appearance paints outside an /Artifact"
            ),
        ));
    }
}

/// Whether `commands` paint anything with no `/Artifact` sequence open.
fn paints_outside_an_artifact(content: &crate::apply::text::Content) -> bool {
    use fepdf_model::object::sublimation::Command;
    let mut open: Vec<bool> = Vec::new();
    for command in content.iter() {
        match command {
            Command::BeginMarkedContent { tag, .. } => open.push(tag.as_str() == "Artifact"),
            Command::EndMarkedContent => {
                open.pop();
            }
            Command::ShowText(_)
            | Command::ShowTextArray(_)
            | Command::Fill(_)
            | Command::Stroke(_)
            | Command::FillStroke(_, _)
            | Command::DrawInlineImage { .. }
            | Command::DrawXObject(_)
                if !open.contains(&true) =>
            {
                return true;
            }
            // `sh` paints a shading, passed through by the parser as it was written.
            Command::RawOperator { name, .. } if name == "sh" && !open.contains(&true) => {
                return true;
            }
            _ => {}
        }
    }
    false
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
