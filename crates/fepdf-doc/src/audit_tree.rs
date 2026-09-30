//! Matterhorn failure conditions decided while the structure tree is walked (W-21i).
//!
//! Role mapping, notes and table headers.

use crate::structure::{AuditFinding, broken, broken_at};
use fepdf_model::{Handle, Object, PdfArena};
use std::collections::{BTreeMap, BTreeSet};

/// The standard structure types of PDF 1.7, which PDF/UA-1 means by "standard type".
///
/// **Derived from this copy of ISO 32000-2, not from memory of ISO 32000-1**
/// ([ADR-0095](../../../docs/adr/0095-a-condition-citing-a-document-this-copy-lacks-is-not-implemented-from-memory.md)):
/// the PDF 2.0 types of Tables 364 to 375, less the ones Annex M says only PDF 2.0 defines
/// (`DocumentFragment`, `Aside`, `Hn` past six, `Title`, `FENote`, `Sub`, `Em`, `Strong`,
/// `Artifact`), with the ones it says only PDF 1.7 defines.
pub const PDF_1_7_STANDARD: [&str; 44] = [
    "Document",
    "Part",
    "Sect",
    "Div",
    "NonStruct",
    "P",
    "H",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "Lbl",
    "Span",
    "Link",
    "Annot",
    "Form",
    "Ruby",
    "RB",
    "RT",
    "RP",
    "Warichu",
    "WT",
    "WP",
    "L",
    "LI",
    "LBody",
    "Table",
    "TR",
    "TH",
    "TD",
    "THead",
    "TBody",
    "TFoot",
    "Caption",
    "Figure",
    "Formula",
    // Annex M: in the PDF 1.7 namespace and not in PDF 2.0's.
    "Art",
    "BlockQuote",
    "TOC",
    "TOCI",
    "Index",
    "Private",
];

/// The rest of Annex M's list of types only PDF 1.7 defines.
const ALSO_PDF_1_7: [&str; 5] = ["Quote", "Note", "Reference", "BibEntry", "Code"];

/// Whether `tag` is a standard type of PDF 1.7.
pub(crate) fn standard(tag: &str) -> bool {
    PDF_1_7_STANDARD.contains(&tag) || ALSO_PDF_1_7.contains(&tag)
}

/// The failure conditions this module decides.
pub const FROM_TREE_WALK: [&str; 6] = ["02-001", "02-003", "02-004", "15-003", "19-003", "19-004"];

/// The structure tree root's `/RoleMap`, as tag to tag.
pub fn role_map(arena: &PdfArena, root: Handle<Object>) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let Some(root) = arena.get_object(root).and_then(|o| o.as_dict_handle()) else { return map };
    let Some(roles) = arena
        .dict_entry(root, arena.name("RoleMap"))
        .and_then(|m| m.resolve(arena).as_dict_handle())
        .and_then(|m| arena.get_dict(m))
    else {
        return map;
    };
    for (key, value) in roles {
        let (Some(from), Some(to)) =
            (arena.get_name(key), value.resolve(arena).as_name().and_then(|n| arena.get_name(n)))
        else {
            continue;
        };
        map.insert(from.as_str().to_string(), to.as_str().to_string());
    }
    map
}

/// 02-003 and 02-004, from the map itself: a standard type mapped to something, and a
/// chain of mappings that comes back on itself.
pub fn audit_role_map(roles: &BTreeMap<String, String>, findings: &mut Vec<AuditFinding>) {
    for (from, to) in roles {
        if standard(from) {
            findings
                .push(broken("02-004", format!("The standard type /{from} is remapped, to /{to}")));
        }
        if matches!(resolved(roles, from), Resolved::Circular) {
            findings.push(broken("02-003", format!("The mapping of /{from} comes back on itself")));
        }
    }
}

/// Where a tag's mapping ends.
enum Resolved {
    /// At a standard type.
    Standard,
    /// At a type that is not standard and is mapped no further.
    Elsewhere(String),
    /// Back where it has been.
    Circular,
}

/// Follows `tag` through `roles` until it reaches a type that is not mapped.
fn resolved(roles: &BTreeMap<String, String>, tag: &str) -> Resolved {
    let mut seen = BTreeSet::new();
    let mut at = tag;
    // A standard type ends the chain even when it is itself (wrongly) mapped: 02-004
    // reports that, and a tag mapped onto it has reached a standard type.
    while !(standard(at) && at != tag) {
        if !seen.insert(at) {
            return Resolved::Circular;
        }
        match roles.get(at) {
            Some(next) => at = next,
            None if standard(at) => return Resolved::Standard,
            None => return Resolved::Elsewhere(at.to_owned()),
        }
    }
    Resolved::Standard
}

/// 02-001, as the walk passes an element: its tag, if not standard, must map to one.
pub fn audit_tag(
    roles: &BTreeMap<String, String>,
    tag: &str,
    at: u32,
    findings: &mut Vec<AuditFinding>,
) {
    if standard(tag) {
        return;
    }
    if let Resolved::Elsewhere(end) = resolved(roles, tag) {
        let said = if end == tag {
            format!("The non-standard type /{tag} is not mapped to a standard type")
        } else {
            format!("The non-standard type /{tag} maps to /{end}, which is not a standard type")
        };
        findings.push(broken_at("02-001", said, at));
    }
}

/// The `<Note>` elements seen, by `/ID`.
#[derive(Default)]
pub struct Notes {
    ids: BTreeMap<Vec<u8>, u32>,
    repeated: Vec<(u32, String)>,
}

impl Notes {
    /// 19-003 now, and what 19-004 is decided from.
    pub fn note(
        &mut self,
        arena: &PdfArena,
        element: Handle<Object>,
        at: u32,
        findings: &mut Vec<AuditFinding>,
    ) {
        let id = arena
            .get_object(element)
            .and_then(|o| o.as_dict_handle())
            .and_then(|d| arena.dict_entry(d, arena.name("ID")))
            .map(|id| id.resolve(arena));
        let bytes = match id {
            Some(Object::String(bytes) | Object::Hex(bytes)) => bytes.to_vec(),
            Some(Object::Text(text)) => text.into_bytes(),
            _ => {
                findings.push(broken_at("19-003", "A <Note> element has no /ID", at));
                return;
            }
        };
        if let Some(first) = self.ids.insert(bytes.clone(), at) {
            self.repeated.push((
                at,
                format!("{} (first at element {first})", String::from_utf8_lossy(&bytes)),
            ));
        }
    }

    /// 19-004, once every note has been seen.
    pub fn conclude(&self, findings: &mut Vec<AuditFinding>) {
        for (at, said) in &self.repeated {
            findings.push(broken_at("19-004", format!("A <Note> repeats the /ID {said}"), *at));
        }
    }
}

/// 15-003 for one `<Table>`: when no cell names its headers, every `<TH>` must say what
/// it heads (UA1:7.5-2).
///
/// `table` is the element, the `/RoleMap` a cell's type is found through, and the
/// `/ClassMap` its attributes may come from.
pub fn audit_table(
    arena: &PdfArena,
    (table, roles, class_map): (
        Handle<Object>,
        &BTreeMap<String, String>,
        &BTreeMap<String, Vec<Object>>,
    ),
    findings: &mut Vec<AuditFinding>,
) {
    let cells = cells_of(arena, roles, table);
    let organised =
        cells.iter().any(|(_, cell)| table_attribute(arena, *cell, class_map, "Headers"));
    if organised {
        return;
    }
    for (tag, cell) in cells {
        if tag == "TH" && !table_attribute(arena, cell, class_map, "Scope") {
            findings.push(broken_at(
                "15-003",
                "A <TH> states no /Scope in a table whose cells name no /Headers",
                cell.index(),
            ));
        }
    }
}

/// How deep below a `<Table>` its cells are looked for: the table, a row group, a row, a
/// cell — with room to spare, and a bound (Rule 6).
const CELL_DEPTH: usize = 8;

/// The `<TH>` and `<TD>` elements under `table`, with the standard type each stands for.
fn cells_of(
    arena: &PdfArena,
    roles: &BTreeMap<String, String>,
    table: Handle<Object>,
) -> Vec<(String, Handle<Object>)> {
    let mut cells = Vec::new();
    let mut waiting = vec![(table, 0_usize)];
    let mut seen = BTreeSet::new();
    while let Some((element, depth)) = waiting.pop() {
        if depth > CELL_DEPTH || !seen.insert(element) {
            continue;
        }
        let Some(dict) = arena.get_object(element).and_then(|o| o.as_dict_handle()) else {
            continue;
        };
        let tag = tag_of(arena, element)
            .map(|tag| standard_type(roles, &tag).unwrap_or(tag))
            .unwrap_or_default();
        if tag == "TH" || tag == "TD" {
            cells.push((tag, element));
            continue;
        }
        let kids = match arena.dict_entry(dict, arena.name("K")).map(|k| k.resolve(arena)) {
            Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
            Some(other) => vec![other],
            None => Vec::new(),
        };
        waiting.extend(
            kids.iter()
                .filter_map(|kid| crate::struct_tree::resolve_to_node_handle(arena, kid))
                .map(|kid| (kid, depth + 1)),
        );
    }
    cells
}

/// Whether `element` states `key` in a `/Table` attribute object, its own through `/A` or
/// its classes' through `/C` and the `/ClassMap` (14.7.6).
fn table_attribute(
    arena: &PdfArena,
    element: Handle<Object>,
    class_map: &BTreeMap<String, Vec<Object>>,
    key: &str,
) -> bool {
    let Some(dict) = arena.get_object(element).and_then(|o| o.as_dict_handle()) else {
        return false;
    };
    let listed = |value: Option<Object>| -> Vec<Object> {
        match value.map(|v| v.resolve(arena)) {
            Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
            Some(other) => vec![other],
            None => Vec::new(),
        }
    };
    let mut objects = listed(arena.dict_entry(dict, arena.name("A")));
    for class in listed(arena.dict_entry(dict, arena.name("C"))) {
        let Some(name) = class.as_name().and_then(|n| arena.get_name(n)) else { continue };
        objects.extend(class_map.get(name.as_str()).cloned().unwrap_or_default());
    }
    objects.iter().any(|object| {
        let Some(attributes) = object.resolve(arena).as_dict_handle() else { return false };
        let owner = arena
            .dict_entry(attributes, arena.name("O"))
            .and_then(|o| o.as_name())
            .and_then(|n| arena.get_name(n));
        owner.is_some_and(|o| o.as_str() == "Table")
            && arena.dict_entry(attributes, arena.name(key)).is_some()
    })
}

/// The structure tree root's `/ClassMap`, as class name to its attribute objects.
pub fn class_map(arena: &PdfArena, root: Handle<Object>) -> BTreeMap<String, Vec<Object>> {
    let mut map = BTreeMap::new();
    let Some(root) = arena.get_object(root).and_then(|o| o.as_dict_handle()) else { return map };
    let Some(classes) = arena
        .dict_entry(root, arena.name("ClassMap"))
        .and_then(|m| m.resolve(arena).as_dict_handle())
        .and_then(|m| arena.get_dict(m))
    else {
        return map;
    };
    for (key, value) in classes {
        let Some(name) = arena.get_name(key) else { continue };
        let objects = match value.resolve(arena) {
            Object::Array(array) => arena.get_array(array).unwrap_or_default(),
            other => vec![other],
        };
        map.insert(name.as_str().to_string(), objects);
    }
    map
}

/// The failure conditions about ISO 32000-1's structure syntax (ROADMAP W-21h).
///
/// Tables, lists, tables of contents, ruby and warichu, each written from its table in
/// `docs/specs/PDF32000_2008.pdf` and from nothing else.
pub const FROM_SYNTAX: [&str; 5] = ["09-004", "09-005", "09-006", "09-007", "09-008"];

/// The standard type `tag` stands for: itself when standard, else where its mapping ends.
pub(crate) fn standard_type(roles: &BTreeMap<String, String>, tag: &str) -> Option<String> {
    if standard(tag) {
        return Some(tag.to_owned());
    }
    let mut seen = BTreeSet::new();
    let mut at = tag;
    while let Some(next) = roles.get(at) {
        if !seen.insert(at) {
            return None;
        }
        if standard(next) {
            return Some(next.clone());
        }
        at = next;
    }
    None
}

/// An element's `/S`, as written.
fn tag_of(arena: &PdfArena, element: Handle<Object>) -> Option<String> {
    let dict = arena.get_object(element)?.as_dict_handle()?;
    let name = arena.dict_entry(dict, arena.name("S"))?.as_name()?;
    arena.get_name(name).map(|n| n.as_str().to_string())
}

/// The structure elements among an element's `/K`, in order — not its marks and not its
/// object references, which carry no `/S`.
fn element_kids(arena: &PdfArena, element: Handle<Object>) -> Vec<Handle<Object>> {
    let Some(dict) = arena.get_object(element).and_then(|o| o.as_dict_handle()) else {
        return Vec::new();
    };
    let kids = match arena.dict_entry(dict, arena.name("K")).map(|k| k.resolve(arena)) {
        Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
        Some(other) => vec![other],
        None => Vec::new(),
    };
    kids.iter()
        .filter_map(|kid| crate::struct_tree::resolve_to_node_handle(arena, kid))
        .filter(|kid| tag_of(arena, *kid).is_some())
        .collect()
}

/// An element's children and parent, as the standard types they stand for.
struct Placed {
    kids: Vec<String>,
    parent: Option<String>,
}

impl Placed {
    fn of(arena: &PdfArena, roles: &BTreeMap<String, String>, element: Handle<Object>) -> Self {
        let mapped =
            |handle| tag_of(arena, handle).map(|tag| standard_type(roles, &tag).unwrap_or(tag));
        let kids = element_kids(arena, element).into_iter().filter_map(mapped).collect();
        let parent = arena
            .get_object(element)
            .and_then(|o| o.as_dict_handle())
            .and_then(|d| arena.dict_entry(d, arena.name("P")))
            .and_then(|p| p.as_reference())
            .and_then(mapped);
        Self { kids, parent }
    }

    /// Whether every child is one of `allowed`.
    fn kids_among(&self, allowed: &[&str]) -> bool {
        self.kids.iter().all(|kid| allowed.contains(&kid.as_str()))
    }

    /// Whether the parent is one of `allowed`.
    fn parent_among(&self, allowed: &[&str]) -> bool {
        self.parent.as_deref().is_some_and(|parent| allowed.contains(&parent))
    }
}

/// A departure from a table's syntax: the condition, and what was found.
type Departure = Option<(&'static str, String)>;

/// Whether an element standing for `kind` sits inside one of `parents`.
fn within(kind: &str, placed: &Placed, condition: &'static str, parents: &[&str]) -> Departure {
    (!placed.parent_among(parents)).then(|| {
        let found =
            format!("<{kind}> is inside <{}>, not {}", parent_name(placed), parents.join(" or "));
        (condition, found)
    })
}

/// Whether an element standing for `kind` holds only `allowed`.
fn holding(kind: &str, placed: &Placed, condition: &'static str, allowed: &[&str]) -> Departure {
    (!placed.kids_among(allowed)).then(|| {
        let found = format!(
            "<{kind}> holds {:?}, where it may hold only {}",
            placed.kids,
            allowed.join(", ")
        );
        (condition, found)
    })
}

/// What checkpoint 09's machine conditions say of an element standing for `kind`, or
/// nothing when it keeps to its table's syntax.
fn misplaced(kind: &str, placed: &Placed) -> Departure {
    table_rule(kind, placed)
        .or_else(|| list_rule(kind, placed))
        .or_else(|| contents_rule(kind, placed))
        .or_else(|| ruby_rule(kind, placed))
}

/// ISO 32000-1, Table 337 (09-004).
fn table_rule(kind: &str, placed: &Placed) -> Departure {
    match kind {
        "Table" => (!table_rows_are_well_formed(&placed.kids)).then(|| {
            let found =
                format!("<Table> holds {:?}, neither rows nor head, bodies and foot", placed.kids);
            ("09-004", found)
        }),
        "TR" => holding(kind, placed, "09-004", &["TH", "TD"])
            .or_else(|| within(kind, placed, "09-004", &["Table", "THead", "TBody", "TFoot"])),
        "THead" | "TBody" | "TFoot" => holding(kind, placed, "09-004", &["TR"])
            .or_else(|| within(kind, placed, "09-004", &["Table"])),
        "TH" | "TD" => within(kind, placed, "09-004", &["TR"]),
        _ => None,
    }
}

/// ISO 32000-1, Table 336 (09-005).
fn list_rule(kind: &str, placed: &Placed) -> Departure {
    match kind {
        "L" => (!list_is_well_formed(&placed.kids)).then(|| {
            let found =
                format!("<L> holds {:?}, not an optional <Caption> then items", placed.kids);
            ("09-005", found)
        }),
        "LI" => holding(kind, placed, "09-005", &["Lbl", "LBody"])
            .or_else(|| within(kind, placed, "09-005", &["L"])),
        "LBody" => within(kind, placed, "09-005", &["LI"]),
        _ => None,
    }
}

/// ISO 32000-1, Table 333's `TOC` and `TOCI` (09-006).
fn contents_rule(kind: &str, placed: &Placed) -> Departure {
    match kind {
        "TOC" => holding(kind, placed, "09-006", &["TOCI", "TOC"]),
        "TOCI" => holding(kind, placed, "09-006", &["Lbl", "Reference", "NonStruct", "P", "TOC"])
            .or_else(|| within(kind, placed, "09-006", &["TOC"])),
        _ => None,
    }
}

/// ISO 32000-1, Table 339, which Table 338 names for ruby and warichu (09-007, 09-008).
fn ruby_rule(kind: &str, placed: &Placed) -> Departure {
    let shape: Vec<&str> = placed.kids.iter().map(String::as_str).collect();
    match kind {
        "Ruby" => (shape != ["RB", "RT"] && shape != ["RB", "RP", "RT", "RP"]).then(|| {
            ("09-007", format!("<Ruby> holds {shape:?}, not RB then RT, or RB, RP, RT, RP"))
        }),
        "RB" | "RT" | "RP" => within(kind, placed, "09-007", &["Ruby"]),
        "Warichu" => (!shape.is_empty() && shape != ["WP", "WT", "WP"])
            .then(|| ("09-008", format!("<Warichu> holds {shape:?}, not WP, WT, WP"))),
        "WT" | "WP" => within(kind, placed, "09-008", &["Warichu"]),
        _ => None,
    }
}

/// The name a finding gives an element's parent.
fn parent_name(placed: &Placed) -> String {
    placed.parent.clone().unwrap_or_else(|| "the tree root".to_owned())
}

/// Table 337: rows, or an optional head, bodies and an optional foot — with a caption, if
/// there is one, first or last.
fn table_rows_are_well_formed(kids: &[String]) -> bool {
    let mut rows: Vec<&str> = kids.iter().map(String::as_str).collect();
    if rows.first() == Some(&"Caption") {
        rows.remove(0);
    } else if rows.last() == Some(&"Caption") {
        rows.pop();
    }
    if rows.is_empty() {
        return false;
    }
    if rows.iter().all(|kid| *kid == "TR") {
        return true;
    }
    let head = usize::from(rows.first() == Some(&"THead"));
    let foot = usize::from(rows.len() > head && rows.last() == Some(&"TFoot"));
    let bodies = &rows[head..rows.len() - foot];
    !bodies.is_empty() && bodies.iter().all(|kid| *kid == "TBody")
}

/// Table 336: an optional caption, then one or more list items.
fn list_is_well_formed(kids: &[String]) -> bool {
    let items = if kids.first().is_some_and(|k| k == "Caption") { &kids[1..] } else { kids };
    !items.is_empty() && items.iter().all(|kid| kid == "LI")
}

/// Checkpoint 09's machine conditions for one element.
pub fn audit_syntax(
    arena: &PdfArena,
    roles: &BTreeMap<String, String>,
    element: Handle<Object>,
    tag: &str,
    findings: &mut Vec<AuditFinding>,
) {
    let Some(kind) = standard_type(roles, tag) else { return };
    let placed = Placed::of(arena, roles, element);
    if let Some((condition, said)) = misplaced(&kind, &placed) {
        findings.push(broken_at(condition, said, element.index()));
    }
}

/// Table 336, 337 and 339's shapes, and what breaks them.
#[cfg(test)]
mod shapes {
    use super::{Placed, list_is_well_formed, misplaced, table_rows_are_well_formed};

    fn placed(kids: &[&str], parent: Option<&str>) -> Placed {
        Placed {
            kids: kids.iter().map(|k| (*k).to_owned()).collect(),
            parent: parent.map(str::to_owned),
        }
    }

    /// **Every rule, broken once and kept once**: each element's children and parent as
    /// its table says, and the condition a departure is reported under.
    #[test]
    fn each_rule_names_its_condition() {
        let cases: [(&str, &[&str], Option<&str>, Option<&str>); 24] = [
            ("TR", &["TH", "TD"], Some("TBody"), None),
            ("TR", &["P"], Some("Table"), Some("09-004")),
            ("TR", &["TD"], Some("Div"), Some("09-004")),
            ("THead", &["TR"], Some("Table"), None),
            ("TBody", &["TD"], Some("Table"), Some("09-004")),
            ("TFoot", &["TR"], Some("Sect"), Some("09-004")),
            ("TD", &[], Some("TR"), None),
            ("TH", &[], Some("Table"), Some("09-004")),
            ("L", &["Caption", "LI"], Some("Div"), None),
            ("LI", &["Lbl", "LBody"], Some("L"), None),
            ("LI", &["P"], Some("L"), Some("09-005")),
            ("LI", &["LBody"], Some("Div"), Some("09-005")),
            ("LBody", &["P"], Some("L"), Some("09-005")),
            ("TOC", &["TOCI", "TOC"], Some("Div"), None),
            ("TOC", &["P"], Some("Div"), Some("09-006")),
            ("TOCI", &["Lbl", "Reference", "NonStruct", "P", "TOC"], Some("TOC"), None),
            ("TOCI", &["Span"], Some("TOC"), Some("09-006")),
            ("TOCI", &["P"], Some("L"), Some("09-006")),
            ("Ruby", &["RB", "RP", "RT", "RP"], Some("P"), None),
            ("Ruby", &["RT", "RB"], Some("P"), Some("09-007")),
            ("RP", &[], Some("P"), Some("09-007")),
            ("Warichu", &["WP", "WT", "WP"], Some("P"), None),
            ("Warichu", &["WT", "WP"], Some("P"), Some("09-008")),
            ("WT", &[], Some("Ruby"), Some("09-008")),
        ];
        for (kind, kids, parent, want) in cases {
            let got = misplaced(kind, &placed(kids, parent)).map(|(condition, _)| condition);
            assert_eq!(got, want, "<{kind}> holding {kids:?} inside {parent:?}");
        }
    }

    fn kids(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn a_table_is_rows_or_a_head_bodies_and_a_foot() {
        for good in [
            &["TR", "TR"][..],
            &["Caption", "TR"],
            &["TR", "Caption"],
            &["THead", "TBody", "TBody", "TFoot"],
            &["TBody"],
            &["Caption", "THead", "TBody"],
        ] {
            assert!(table_rows_are_well_formed(&kids(good)), "{good:?} was refused");
        }
        for bad in [
            &[][..],
            &["TR", "TBody"],
            &["TBody", "THead"],
            &["THead", "TFoot"],
            &["TR", "Caption", "TR"],
            &["P"],
        ] {
            assert!(!table_rows_are_well_formed(&kids(bad)), "{bad:?} was taken");
        }
    }

    #[test]
    fn a_list_is_a_caption_and_items() {
        assert!(list_is_well_formed(&kids(&["LI", "LI"])));
        assert!(list_is_well_formed(&kids(&["Caption", "LI"])));
        assert!(!list_is_well_formed(&kids(&["LI", "Caption"])));
        assert!(!list_is_well_formed(&kids(&["LI", "P"])));
        assert!(!list_is_well_formed(&kids(&["Caption"])));
    }
}
