//! Conditions the protocol leaves to a person (`H`), decided where the document gives the
//! answer (ROADMAP W-21v).
//!
//! **Most of them are about something a document may not have at all.** Whether an action
//! flickers, whether audio is available another way, whether a figure's `/ActualText`
//! would serve better as `/Alt` — each is a person's question about a thing, and a document
//! with no such thing has no such question: the machine decides it sound, and says why. A
//! document that has the thing gets a finding for a reader, naming how many there are.
//! **31-010 is the exception**: a font program's `OS/2.fsType` states whether it may be
//! embedded, and a program that says it may not has settled the question itself.

use crate::audit_objects::{entry, name_of};
use crate::structure::{AuditFinding, broken, for_a_reader, sound_because};
use fepdf_model::{Document, Object, PdfArena};
use std::collections::BTreeSet;

/// The failure conditions this module decides.
pub const FROM_PRESENCE: [&str; 17] = [
    "03-001", "03-002", "03-003", "05-001", "05-002", "05-003", "13-002", "13-005", "13-008",
    "16-001", "16-002", "22-001", "28-001", "28-003", "28-013", "29-001", "31-010",
];

/// ISO 32000-1 Table 198's action types, read out of `PDF32000_2008.pdf`.
const ACTION_TYPES: [&str; 18] = [
    "GoTo",
    "GoToR",
    "GoToE",
    "Launch",
    "Thread",
    "URI",
    "Sound",
    "Movie",
    "Hide",
    "Named",
    "SubmitForm",
    "ResetForm",
    "ImportData",
    "JavaScript",
    "SetOCGState",
    "Rendition",
    "Trans",
    "GoTo3DView",
];

/// The action types that move the view once and nothing else — unless chained by `/Next`.
const NAVIGATING: [&str; 5] = ["GoTo", "GoToR", "GoToE", "URI", "Thread"];

/// What the document has, of what the conditions here are about.
#[derive(Default)]
struct Has {
    /// Each action's type, and whether it chains another.
    actions: Vec<(String, bool)>,
    /// Each annotation's subtype.
    annotations: Vec<String>,
    /// Each script's text.
    scripts: Vec<String>,
    /// URI actions stating `/IsMap true`.
    is_map: usize,
    /// Font descriptors carrying a program.
    descriptors: Vec<Object>,
}

/// Asks every condition in [`FROM_PRESENCE`] of `doc`, whose reachable dictionaries are
/// `dicts`.
pub(crate) fn audit_presence(
    doc: &Document,
    dicts: &[Object],
    findings: &mut Vec<AuditFinding>,
    examined: &mut BTreeSet<&'static str>,
) {
    let has = survey(doc, dicts);
    let count = |f: &dyn Fn(&str) -> bool| has.annotations.iter().filter(|s| f(s)).count();
    let actions = |f: &dyn Fn(&str) -> bool| has.actions.iter().filter(|(s, _)| f(s)).count();
    let changing =
        has.actions.iter().filter(|(s, next)| *next || !NAVIGATING.contains(&s.as_str()));
    decide("03-001", changing.count(), "actions that change more than the view once", findings);
    let media = |s: &str| matches!(s, "Screen" | "Movie" | "Sound" | "RichMedia" | "3D");
    let playing = |s: &str| matches!(s, "Sound" | "Movie" | "Rendition" | "GoTo3DView");
    decide("03-002", count(&media) + actions(&playing), "multimedia objects", findings);
    decide("03-003", actions(&|s| s == "JavaScript"), "JavaScript actions", findings);
    decide("29-001", has.scripts.len(), "scripts", findings);
    let beeps = has.scripts.iter().filter(|s| s.contains("beep")).count();
    decide("05-003", beeps, "scripts calling beep", findings);
    let screens = count(&|s| matches!(s, "Screen" | "Movie" | "RichMedia"));
    decide("05-001", screens, "media annotations", findings);
    let sounds = count(&|s| s == "Sound") + actions(&|s| s == "Sound");
    decide("05-002", sounds, "sound annotations and actions", findings);
    decide("13-002", count(&|s| s == "Link"), "link annotations", findings);
    let annotations = count(&|s| s != "Popup");
    decide("28-001", annotations, "annotations", findings);
    decide("28-003", annotations, "annotations", findings);
    decide("28-013", has.is_map, "URI actions stating /IsMap true", findings);
    let threads = crate::audit_objects::items(doc.arena(), &catalogue(doc), "Threads").len();
    decide("22-001", threads, "article threads", findings);
    elements(doc, findings);
    embeddable(doc, &has.descriptors, findings);
    examined.extend(FROM_PRESENCE);
}

/// The catalogue, as an object.
fn catalogue(doc: &Document) -> Object {
    doc.catalog_handle().map_or(Object::Null, Object::Reference)
}

/// A condition a document with none of `what` cannot break, and one with some leaves to a
/// reader.
fn decide(condition: &str, found: usize, what: &str, findings: &mut Vec<AuditFinding>) {
    if found == 0 {
        findings.push(sound_because(
            condition,
            format!(
                "Decided by the machine: the document has no {what}, so the question does not arise"
            ),
        ));
    } else {
        findings.push(for_a_reader(
            condition,
            format!(
                "The document has {found} {what}; whether any breaks {condition} is for a person"
            ),
        ));
    }
}

/// What the document has, from its reachable dictionaries.
fn survey(doc: &Document, dicts: &[Object]) -> Has {
    let arena = doc.arena();
    let mut has = Has::default();
    for dict in dicts {
        if let Some(subtype) = name_of(arena, dict, "Subtype")
            && entry(arena, dict, "Rect").is_some()
        {
            has.annotations.push(subtype);
        }
        if entry(arena, dict, "FontFile").is_some()
            || entry(arena, dict, "FontFile2").is_some()
            || entry(arena, dict, "FontFile3").is_some()
        {
            has.descriptors.push(dict.clone());
        }
        let Some(kind) = name_of(arena, dict, "S").filter(|s| ACTION_TYPES.contains(&s.as_str()))
        else {
            continue;
        };
        if name_of(arena, dict, "Type").as_deref() == Some("StructElem") {
            continue;
        }
        if kind == "JavaScript" {
            has.scripts.push(script(doc, arena, dict));
        }
        if kind == "URI" && matches!(entry(arena, dict, "IsMap"), Some(Object::Boolean(true))) {
            has.is_map += 1;
        }
        has.actions.push((kind, entry(arena, dict, "Next").is_some()));
    }
    has
}

/// A JavaScript action's `/JS`, from a string or a stream.
fn script(doc: &Document, arena: &PdfArena, action: &Object) -> String {
    match entry(arena, action, "JS") {
        Some(Object::Text(text)) => text,
        Some(Object::String(b) | Object::Hex(b)) => fepdf_model::refine::text::recover_string(&b),
        Some(stream @ Object::Stream(..)) => doc
            .decode_stream(&stream)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// The numberings Table 347 gives an ordered list — the ones 16-002 names.
const ORDERED: [&str; 5] = ["Decimal", "UpperRoman", "LowerRoman", "UpperAlpha", "LowerAlpha"];

/// 13-005, 13-008, 16-001 and 16-002, from the `<Figure>` and `<L>` elements the structure
/// tree has — none, in a document with no tree.
fn elements(doc: &Document, findings: &mut Vec<AuditFinding>) {
    let arena = doc.arena();
    let (mut described, mut undescribed, mut unnumbered, mut unordered) = (0, 0, 0, 0);
    if let Some(root) = doc.get_structure_root().ok().flatten() {
        let roles = crate::audit_tree::role_map(arena, root);
        let classes = crate::audit_tree::class_map(arena, root);
        let mut visitor = crate::structure::StructureVisitor::new(arena, root);
        while let Some(element) = visitor.next_element() {
            let element = Object::Reference(element);
            let Some(tag) = name_of(arena, &element, "S") else { continue };
            match crate::audit_tree::standard_type(&roles, &tag).as_deref() {
                Some("Figure") if entry(arena, &element, "ActualText").is_some() => described += 1,
                Some("Figure") => undescribed += 1,
                Some("L") => match list_numbering(arena, &element, &classes) {
                    None => unnumbered += 1,
                    Some(value) if !ORDERED.contains(&value.as_str()) => unordered += 1,
                    Some(_) => {}
                },
                _ => {}
            }
        }
    }
    decide("13-005", described, "figures with /ActualText", findings);
    decide("13-008", undescribed, "figures without /ActualText", findings);
    decide("16-001", unnumbered, "lists stating no ListNumbering", findings);
    decide("16-002", unordered, "lists whose ListNumbering names no ordered numbering", findings);
}

/// An `<L>`'s `ListNumbering` (Table 347), from its `/A` attribute objects owned by `List`
/// or the classes its `/C` names.
fn list_numbering(
    arena: &PdfArena,
    element: &Object,
    classes: &std::collections::BTreeMap<String, Vec<Object>>,
) -> Option<String> {
    let mut attributes = match entry(arena, element, "A") {
        Some(Object::Array(_)) => crate::audit_objects::items(arena, element, "A"),
        Some(single) => vec![single],
        None => Vec::new(),
    };
    let named: Vec<String> = match entry(arena, element, "C") {
        Some(Object::Name(n)) => {
            arena.get_name(n).map(|n| n.as_str().to_string()).into_iter().collect()
        }
        Some(Object::Array(_)) => crate::audit_objects::items(arena, element, "C")
            .iter()
            .filter_map(|c| c.as_name().and_then(|n| arena.get_name(n)))
            .map(|n| n.as_str().to_string())
            .collect(),
        _ => Vec::new(),
    };
    attributes.extend(named.iter().filter_map(|c| classes.get(c)).flatten().cloned());
    attributes
        .iter()
        .filter(|a| name_of(arena, a, "O").as_deref() == Some("List"))
        .find_map(|a| name_of(arena, a, "ListNumbering"))
}

/// 31-010: each embedded program's `OS/2.fsType` permits embedding it as it is embedded —
/// not restricted, not bitmap-only, and not subset where it forbids subsetting. A program
/// with no `OS/2` table states nothing, and is left to a person.
fn embeddable(doc: &Document, descriptors: &[Object], findings: &mut Vec<AuditFinding>) {
    use fepdf_font::embedding::{Embedding, embedding_permission};
    let arena = doc.arena();
    let (mut refused, mut silent) = (Vec::new(), 0_usize);
    for descriptor in descriptors {
        let program = ["FontFile2", "FontFile3", "FontFile"]
            .iter()
            .find_map(|key| entry(arena, descriptor, key))
            .and_then(|file| doc.decode_stream(&file).ok());
        let name = name_of(arena, descriptor, "FontName").unwrap_or_default();
        let subset =
            name.get(6..7) == Some("+") && name[..6].bytes().all(|b| b.is_ascii_uppercase());
        match program.as_deref().and_then(embedding_permission) {
            None => silent += 1,
            Some(p) if p.usage == Embedding::Restricted || p.bitmap_only => refused.push(name),
            Some(p) if subset && !p.subsetting_allowed => refused.push(name),
            Some(_) => {}
        }
    }
    if let Some(first) = refused.first() {
        findings.push(broken(
            "31-010",
            format!(
                "{} embedded programs are embedded as their OS/2 fsType does not permit — \
                 restricted, bitmap-only, or subset where it forbids subsetting — /{first} first",
                refused.len()
            ),
        ));
    } else if silent > 0 {
        findings.push(for_a_reader(
            "31-010",
            format!(
                "{silent} embedded programs state no embedding permission (no OS/2 table, which \
                 a CFF or Type 1 program never has); whether each may be embedded is for a person"
            ),
        ));
    } else {
        findings.push(sound_because(
            "31-010",
            "Decided by the machine: every embedded program's OS/2 fsType permits embedding it as \
             it is embedded",
        ));
    }
}
