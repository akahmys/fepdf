use fepdf_model::Document;
use fepdf_model::arena::PdfArena;
use fepdf_model::handle::Handle;
use fepdf_model::object::{Object, PdfName};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Presentation-ready structure tree node for frontends (GUI/MCP/CLI).
/// Contains zero raw arena pointers.
pub struct StructureTreeNode {
    /// Unique identifier within the presentation hierarchy.
    pub id: usize,
    /// Structure element tag name (e.g., "P", "H1", "Document").
    pub tag: String,
    /// Display title or tag name.
    pub title: String,
    /// Alternative text description (/Alt entry).
    pub alt_text: Option<String>,
    /// Bounding box rectangle ([llx, lly, urx, ury]) in default user space.
    ///
    /// `/BBox` when the element declares one (14.8.5.4.5), which no element in any of
    /// the nine samples does. Otherwise it is filled from where the element's marked
    /// content actually drew, which is what [`mcids`] is for and what
    /// `fepdf::PdfDocument::fill_structure_boxes` does.
    ///
    /// [`mcids`]: StructureTreeNode::mcids
    pub rect: Option<[f32; 4]>,
    /// Resolved zero-based target page index (from /Pg entry or inherited).
    pub page_index: Option<usize>,
    /// Handle index of the underlying PdfArena object.
    pub handle_index: Option<u32>,
    /// The marked-content ids this element claims directly (14.7.4.2).
    ///
    /// Its own, not its descendants': `/K` is a tree and each element names the marks it
    /// stands for. An element that holds only other elements names none, and takes its
    /// rectangle from theirs.
    ///
    /// Defaulted on the way in, because this type is also the shape a GUI draft is saved
    /// in and a draft written before the field existed still has to load. The `Option`
    /// fields beside it get that for nothing; a `Vec` has to ask.
    #[serde(default)]
    pub mcids: Vec<u32>,
    /// The natural language of this element's content (14.9.2).
    ///
    /// **The language in force, not the entry.** 14.9.2 makes `/Lang` inheritable: an
    /// element without one is in the language of the element above it, and the tree's
    /// root falls back to the catalogue's. A reader asking what language a paragraph is
    /// in wants the answer that applies to it, and the entry alone answers that for 857
    /// of `print_sample.pdf`'s elements and for none of the other eight samples'.
    #[serde(default)]
    pub lang: Option<String>,
    /// The standard structure type [`tag`] stands for, when `/RoleMap` maps it (14.8.4.4).
    ///
    /// `None` says the tag needs no mapping, which is the answer for every tag in every
    /// sample but one: the whole corpus declares a single `/RoleMap`.
    ///
    /// [`tag`]: StructureTreeNode::tag
    #[serde(default)]
    pub role: Option<String>,
    /// Child nodes in the structure hierarchy.
    pub children: Vec<StructureTreeNode>,
}

/// What does not change as the walk descends.
struct Walk<'a> {
    page_map: &'a BTreeMap<Handle<Object>, usize>,
    /// `/StructTreeRoot` `/RoleMap`, read once (14.8.4.4).
    roles: BTreeMap<String, String>,
}

/// What an element takes from the elements above it when it declares none itself.
#[derive(Clone, Copy, Default)]
struct Inherited<'a> {
    page: Option<usize>,
    lang: Option<&'a str>,
}

/// Visitor that extracts structure tree information from a document.
pub struct StructureTreeVisitor;

impl StructureTreeVisitor {
    /// Extracts the full presentation-ready structure tree from a Document.
    pub fn extract(doc: &Document) -> Option<StructureTreeNode> {
        let arena = doc.arena();
        let cah = doc.catalog_handle()?;
        let cadh = doc.resolve_to_dict(cah).ok()?;
        let dict = arena.get_dict(cadh)?;
        let str_root_key = arena.name("StructTreeRoot");
        let str_root_obj = dict.get(&str_root_key)?;
        let str_root_ref = resolve_to_node_handle(arena, str_root_obj)?;
        let page_map = build_page_handle_map(doc);
        let walk = Walk { page_map: &page_map, roles: read_role_map(arena, str_root_ref) };
        // The document's own language, which every element is in until one says otherwise
        // (14.9.2). `print_sample.pdf` is the only sample that says otherwise.
        let lang = text_entry(arena, &dict, "Lang");
        let inherited = Inherited { page: None, lang: lang.as_deref() };
        let mut visited = BTreeSet::new();
        let mut next_id = 0;
        parse_struct_node(arena, str_root_ref, &mut next_id, &mut visited, &walk, inherited)
    }
}

/// `/StructTreeRoot` `/RoleMap`, as tag to standard type (14.8.4.4).
///
/// Read once for the tree rather than per element: it is a single dictionary on the root,
/// and a lookup per element through the arena would be the same answer found again.
fn read_role_map(arena: &PdfArena, root: Handle<Object>) -> BTreeMap<String, String> {
    let mut roles = BTreeMap::new();
    let Some(dict) = arena
        .get_object(root)
        .and_then(|object| object.as_dict_handle())
        .and_then(|dh| arena.get_dict(dh))
    else {
        return roles;
    };
    let Some(map) = dict
        .get(&arena.name("RoleMap"))
        .map(|entry| entry.resolve(arena))
        .and_then(|entry| entry.as_dict_handle())
        .and_then(|dh| arena.get_dict(dh))
    else {
        return roles;
    };
    for (key, value) in &map {
        let Some(from) = arena.get_name(*key) else { continue };
        let Some(to) = value.resolve(arena).as_name().and_then(|n| arena.get_name(n)) else {
            continue;
        };
        roles.insert(from.as_str().to_string(), to.as_str().to_string());
    }
    roles
}

/// A text-string entry of a dictionary, in the shapes a `/Lang` is written in.
fn text_entry(
    arena: &PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
    key: &str,
) -> Option<String> {
    match dict.get(&arena.name(key))?.resolve(arena) {
        Object::String(bytes) | Object::Hex(bytes) => {
            Some(fepdf_model::refine::text::recover_string(&bytes))
        }
        Object::Text(text) => Some(text),
        _ => None,
    }
}

pub(crate) fn resolve_to_node_handle(arena: &PdfArena, obj: &Object) -> Option<Handle<Object>> {
    match obj {
        Object::Reference(h) => Some(*h),
        Object::Dictionary(dh) => Some(Handle::new(dh.index())),
        _ => {
            let resolved = obj.resolve(arena);
            match resolved {
                Object::Reference(h) => Some(h),
                Object::Dictionary(dh) => Some(Handle::new(dh.index())),
                _ => None,
            }
        }
    }
}

/// How deep a `/K` chain is followed before it is taken to be looping.
///
/// The same 64 as the field-tree and `/Next` walks in `fepdf-model`, and a depth rather
/// than a visited set for the reason [ADR-0060] gives: this is a tree, where meeting the
/// same node twice can be legitimate, and a set would prune what it should keep.
///
/// [ADR-0060]: ../../../docs/adr/0060-a-reference-chain-is-bounded-by-what-it-has-seen.md
const MAX_STRUCTURE_DEPTH: usize = 64;

/// Where a moved structure element lands relative to the one it was dropped on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Placement {
    /// Immediately before the target, under the target's own parent.
    Before,
    /// Immediately after it.
    After,
    /// As the target's last child.
    Inside,
}

/// Moves `subject` to sit beside or inside `target` (14.7.4).
///
/// **The landing is worked out before the element is detached.** `/K` is the only record
/// of where an element hangs, so a detach that cannot be followed by an attach loses the
/// element and the whole subtree under it — and the element is still in the arena,
/// referenced by nothing, which is a leak a reader cannot see.
///
/// Answers `false` and changes nothing for a move that would make a cycle, for a target
/// that is not in the tree, and for an element the tree does not hold.
pub fn move_struct_node(
    arena: &PdfArena,
    root: Handle<Object>,
    subject: Handle<Object>,
    target: Handle<Object>,
    placement: Placement,
) -> bool {
    if subject == target || holds(arena, subject, target, 0) {
        return false;
    }
    let landing = match placement {
        Placement::Inside => Some(target),
        Placement::Before | Placement::After => parent_of(arena, root, target, 0),
    };
    let Some(parent) = landing else {
        return false;
    };
    if !delete_struct_node(arena, root, subject) {
        return false;
    }
    let placed = match placement {
        Placement::Inside => insert_kid(arena, parent, None, subject),
        Placement::Before => insert_kid(arena, parent, Some((target, 0)), subject),
        Placement::After => insert_kid(arena, parent, Some((target, 1)), subject),
    };
    if placed {
        set_parent(arena, subject, parent);
    }
    placed
}

/// Whether `target` is `ancestor` or hangs anywhere under it.
///
/// The test that keeps `/K` a tree: dropping a section inside its own paragraph would
/// make a ring, and every walk over this structure is bounded by a depth precisely
/// because one exists in the corpus already ([ADR-0060]).
///
/// [ADR-0060]: ../../../docs/adr/0060-a-reference-chain-is-bounded-by-what-it-has-seen.md
fn holds(arena: &PdfArena, ancestor: Handle<Object>, target: Handle<Object>, depth: usize) -> bool {
    if ancestor == target {
        return true;
    }
    if depth >= MAX_STRUCTURE_DEPTH {
        return false;
    }
    kids_of(arena, ancestor)
        .iter()
        .filter_map(|kid| element_handle(arena, kid))
        .any(|kid| holds(arena, kid, target, depth + 1))
}

/// The element `target` hangs under, searching from `parent`.
fn parent_of(
    arena: &PdfArena,
    parent: Handle<Object>,
    target: Handle<Object>,
    depth: usize,
) -> Option<Handle<Object>> {
    if depth >= MAX_STRUCTURE_DEPTH {
        return None;
    }
    let kids = kids_of(arena, parent);
    let elements: Vec<Handle<Object>> =
        kids.iter().filter_map(|kid| element_handle(arena, kid)).collect();
    if elements.contains(&target) {
        return Some(parent);
    }
    elements.into_iter().find_map(|kid| parent_of(arena, kid, target, depth + 1))
}

/// One `/K` entry as the element it names, or `None` for a mark or an `/OBJR`.
fn element_handle(arena: &PdfArena, kid: &Object) -> Option<Handle<Object>> {
    match classify_kid(arena, kid, &BTreeMap::new()) {
        Kid::Element(handle) => Some(handle),
        Kid::Mark(_, _) | Kid::Nothing => None,
    }
}

/// An element's `/K`, always as a list, empty when it has none.
fn kids_of(arena: &PdfArena, element: Handle<Object>) -> Vec<Object> {
    let Some(dict) = arena
        .get_object(element)
        .and_then(|object| object.as_dict_handle())
        .and_then(|dh| arena.get_dict(dh))
    else {
        return Vec::new();
    };
    let Some(kids) = dict.get(&arena.name("K")) else {
        return Vec::new();
    };
    match kids.resolve(arena) {
        Object::Array(ah) => arena.get_array(ah).unwrap_or_default(),
        _ => vec![kids.clone()],
    }
}

/// Puts `subject` into `parent`'s `/K`, beside `beside` or at the end.
///
/// `/K` is normalised to an array on the way: 14.7.4 allows a single entry written bare,
/// and a second child cannot be added to one that is.
fn insert_kid(
    arena: &PdfArena,
    parent: Handle<Object>,
    beside: Option<(Handle<Object>, usize)>,
    subject: Handle<Object>,
) -> bool {
    let Some(dh) = arena.get_object(parent).and_then(|object| object.as_dict_handle()) else {
        return false;
    };
    let Some(mut dict) = arena.get_dict(dh) else {
        return false;
    };
    let mut kids = kids_of(arena, parent);
    let at = match beside {
        None => kids.len(),
        Some((target, offset)) => {
            let Some(index) =
                kids.iter().position(|kid| element_handle(arena, kid) == Some(target))
            else {
                return false;
            };
            index + offset
        }
    };
    kids.insert(at, Object::Reference(subject));
    dict.insert(arena.name("K"), Object::Array(arena.alloc_array(kids)));
    arena.set_dict(dh, dict);
    true
}

/// Points `subject`'s `/P` at its new parent, which 14.7.2 requires it to carry.
fn set_parent(arena: &PdfArena, subject: Handle<Object>, parent: Handle<Object>) {
    let Some(dh) = arena.get_object(subject).and_then(|object| object.as_dict_handle()) else {
        return;
    };
    let Some(mut dict) = arena.get_dict(dh) else {
        return;
    };
    dict.insert(arena.name("P"), Object::Reference(parent));
    arena.set_dict(dh, dict);
}

/// Removes `target_handle` from wherever it hangs under `parent_handle`.
///
/// **Bounded since 2026-09-05.** A structure tree whose `/K` named an ancestor made this
/// follow it forever: `Operation::DeleteStructElem` with a handle no element carries has
/// to exhaust the tree, and `fatal runtime error: stack overflow` is what it did instead.
/// RR-15 Rule 6, whose enforcement column says "Code review".
pub(crate) fn delete_struct_node(
    arena: &PdfArena,
    parent_handle: Handle<Object>,
    target_handle: Handle<Object>,
) -> bool {
    delete_struct_node_at(arena, parent_handle, target_handle, 0)
}

/// The kids of one node with `target_handle` gone, or `None` if it was not under them.
///
/// Split out of [`delete_struct_node_at`] to keep that under RR-15 Rule 1's fifty lines
/// once it took a depth, not because the two do different jobs.
fn delete_from_kids(
    arena: &PdfArena,
    kids: &[Object],
    target_handle: Handle<Object>,
    depth: usize,
) -> Option<Vec<Object>> {
    let mut removed = false;
    let mut kept = Vec::new();
    for kid in kids {
        let Some(kid_ref) = resolve_to_node_handle(arena, kid) else {
            kept.push(kid.clone());
            continue;
        };
        if kid_ref == target_handle {
            removed = true;
            continue;
        }
        if delete_struct_node_at(arena, kid_ref, target_handle, depth + 1) {
            removed = true;
        }
        kept.push(kid.clone());
    }
    removed.then_some(kept)
}

fn delete_struct_node_at(
    arena: &PdfArena,
    parent_handle: Handle<Object>,
    target_handle: Handle<Object>,
    depth: usize,
) -> bool {
    if depth >= MAX_STRUCTURE_DEPTH {
        return false;
    }
    let Some(obj) = arena.get_object(parent_handle) else {
        return false;
    };
    let Some(dh) = obj.as_dict_handle() else {
        return false;
    };
    let Some(mut dict) = arena.get_dict(dh) else {
        return false;
    };

    let k_key = arena.name("K");
    let Some(kids_obj) = dict.get(&k_key).cloned() else {
        return false;
    };

    let mut removed = false;
    if let Some(kid_ref) = resolve_to_node_handle(arena, &kids_obj) {
        if kid_ref == target_handle {
            dict.remove(&k_key);
            removed = true;
        } else {
            removed = delete_struct_node_at(arena, kid_ref, target_handle, depth + 1);
        }
    } else if let Object::Array(ah) = kids_obj.resolve(arena)
        && let Some(array) = arena.get_array(ah)
        && let Some(kept) = delete_from_kids(arena, &array, target_handle, depth)
    {
        dict.insert(k_key, Object::Array(arena.alloc_array(kept)));
        removed = true;
    }

    if removed {
        arena.set_dict(dh, dict);
    }
    removed
}

fn build_page_handle_map(doc: &Document) -> BTreeMap<Handle<Object>, usize> {
    let mut map = BTreeMap::new();
    if let Ok(count) = doc.page_count() {
        for index in 0..count {
            if let Some(page_h) = doc.get_page_handle(index) {
                map.insert(page_h, index);
            }
        }
    }
    map
}

#[allow(clippy::cast_possible_truncation)]
fn parse_bbox_helper(arena: &PdfArena, bbox_obj: &Object) -> Option<[f32; 4]> {
    let array_h = bbox_obj.resolve(arena).as_array()?;
    let arr = arena.get_array(array_h)?;
    if arr.len() != 4 {
        return None;
    }
    let x1 = arr[0].resolve(arena).as_f64().unwrap_or(0.0) as f32;
    let y1 = arr[1].resolve(arena).as_f64().unwrap_or(0.0) as f32;
    let x2 = arr[2].resolve(arena).as_f64().unwrap_or(0.0) as f32;
    let y2 = arr[3].resolve(arena).as_f64().unwrap_or(0.0) as f32;
    Some([x1, y1, x2, y2])
}

/// What one entry of `/K` turns out to be (14.7.4.2 Table 355).
enum Kid {
    /// A marked-content id, and the page the reference names when it names one.
    ///
    /// `/MCR` may carry its own `/Pg` (14.7.4.2 Table 324), and `volvo_xc90.pdf` puts it
    /// there and nowhere else: not one of its 13,558 references hangs off an element with
    /// a `/Pg` of its own, so an element that ignored this would know its marks and not
    /// which page they are on — which is the same as not knowing them.
    Mark(u32, Option<usize>),
    /// Another structure element.
    Element(Handle<Object>),
    /// An `/OBJR`, or anything else `/K` is not allowed to hold.
    ///
    /// `/OBJR` names an annotation or a form field, which is content but is not marked
    /// content and has no `/MCID` to place it by. It is dropped rather than descended
    /// into: `print_sample.pdf` writes 20 as indirect objects, and every one of them came
    /// out of this walk as a structure element tagged `P`, because a dictionary with no
    /// `/S` falls back to that tag. (`volvo_xc90.pdf`'s 844 are written in place and were
    /// already being lost, for the reason `classify_kid` gives.)
    Nothing,
}

/// Which of the three an entry is.
///
/// The `/MCR` case is the reason this exists. 14.7.4.2 gives a marked-content reference
/// the same shape as an element — a dictionary — and `volvo_xc90.pdf` writes 13,558 of
/// them.
///
/// **The dictionary is read from the resolved object, not through
/// [`resolve_to_node_handle`].** That function answers `Handle::new(dh.index())` for a
/// dictionary written in place, which is a dictionary's index used as an object's: a
/// different table. Every `/MCR` in `volvo_xc90.pdf` is written in place, so every one
/// of them resolved to some unrelated object and then fell out of the walk in silence.
/// That is the second reason this tree had no geometry to give anyone.
fn classify_kid(arena: &PdfArena, kid: &Object, page_map: &BTreeMap<Handle<Object>, usize>) -> Kid {
    let resolved = kid.resolve(arena);
    if let Object::Integer(mcid) = resolved {
        // A bare integer is a mark in the content stream of whatever page the element
        // already resolved to, so it names none of its own.
        return u32::try_from(mcid).map_or(Kid::Nothing, |mcid| Kid::Mark(mcid, None));
    }
    let Some(dict) = resolved.as_dict_handle().and_then(|dh| arena.get_dict(dh)) else {
        return Kid::Nothing;
    };
    let kind = dict
        .get(&arena.name("Type"))
        .map(|t| t.resolve(arena))
        .and_then(|t| t.as_name())
        .and_then(|n| arena.get_name(n))
        .map(|n| n.as_str().to_string());
    match kind.as_deref() {
        Some("MCR") => match dict.get(&arena.name("MCID")).map(|m| m.resolve(arena)) {
            Some(Object::Integer(mcid)) => u32::try_from(mcid).map_or(Kid::Nothing, |mcid| {
                Kid::Mark(mcid, parse_page_index_helper(arena, &dict, page_map))
            }),
            _ => Kid::Nothing,
        },
        Some("OBJR") => Kid::Nothing,
        _ => resolve_to_node_handle(arena, kid).map_or(Kid::Nothing, Kid::Element),
    }
}

/// What one element's `/K` yields: its children, its marks, and the page its marks name.
#[derive(Default)]
struct Kids {
    children: Vec<StructureTreeNode>,
    mcids: Vec<u32>,
    /// The page the first `/MCR` named, for an element that carries no `/Pg` itself.
    ///
    /// The first rather than all of them: 14.7.4.2 permits an element whose references
    /// name different pages, and nothing in the corpus writes one — a node carries one
    /// page index, and inventing a second field for a shape no file uses would be
    /// answering a question nobody asked.
    page: Option<usize>,
}

fn parse_kids_helper(
    arena: &PdfArena,
    kids_obj: &Object,
    next_id: &mut usize,
    visited: &mut BTreeSet<Handle<Object>>,
    walk: &Walk<'_>,
    inherited: Inherited<'_>,
) -> Kids {
    let mut out = Kids::default();
    if let Object::Array(ah) = kids_obj.resolve(arena)
        && let Some(array) = arena.get_array(ah)
    {
        for kid in array {
            take_kid(arena, &kid, next_id, visited, walk, inherited, &mut out);
        }
        return out;
    }
    take_kid(arena, kids_obj, next_id, visited, walk, inherited, &mut out);
    out
}

/// Files one entry of `/K` into `out`.
fn take_kid(
    arena: &PdfArena,
    kid: &Object,
    next_id: &mut usize,
    visited: &mut BTreeSet<Handle<Object>>,
    walk: &Walk<'_>,
    inherited: Inherited<'_>,
    out: &mut Kids,
) {
    match classify_kid(arena, kid, walk.page_map) {
        Kid::Mark(mcid, page) => {
            out.mcids.push(mcid);
            out.page = out.page.or(page);
        }
        Kid::Element(handle) => {
            if let Some(child) = parse_struct_node(arena, handle, next_id, visited, walk, inherited)
            {
                out.children.push(child);
            }
        }
        Kid::Nothing => {}
    }
}

fn parse_tag_helper(arena: &PdfArena, dict: &BTreeMap<Handle<PdfName>, Object>) -> String {
    let type_key = arena.name("Type");
    let s_key = arena.name("S");

    if let Some(s_obj) = dict.get(&s_key) {
        let resolved = s_obj.resolve(arena);
        if let Some(name_h) = resolved.as_name() {
            arena.get_name(name_h).map_or_else(|| "P".to_string(), |n| n.as_str().to_string())
        } else {
            "P".to_string()
        }
    } else {
        let type_val = dict.get(&type_key).and_then(|t: &Object| t.resolve(arena).as_name());
        if let Some(tv) = type_val {
            if arena.get_name(tv).is_some_and(|n| n.as_str() == "StructTreeRoot") {
                "Document".to_string()
            } else {
                "P".to_string()
            }
        } else {
            "P".to_string()
        }
    }
}

fn parse_alt_text_helper(
    arena: &PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
) -> Option<String> {
    let alt_key = arena.name("Alt");
    let alt_obj = dict.get(&alt_key)?;
    let resolved = alt_obj.resolve(arena);
    let bytes = resolved.as_string()?;
    String::from_utf8(bytes.to_vec()).ok()
}

fn parse_page_index_helper(
    arena: &PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
    page_map: &BTreeMap<Handle<Object>, usize>,
) -> Option<usize> {
    let pg_obj = dict.get(&arena.name("Pg"))?;
    let pg_ref = resolve_to_node_handle(arena, pg_obj)?;
    page_map.get(&pg_ref).copied()
}

fn parse_struct_node(
    arena: &PdfArena,
    handle: Handle<Object>,
    next_id: &mut usize,
    visited: &mut BTreeSet<Handle<Object>>,
    walk: &Walk<'_>,
    inherited: Inherited<'_>,
) -> Option<StructureTreeNode> {
    if !visited.insert(handle) {
        return None;
    }
    let obj = arena.get_object(handle)?;
    let dh = obj.as_dict_handle()?;
    let dict = arena.get_dict(dh)?;

    let tag = parse_tag_helper(arena, &dict);
    let title = tag.clone();
    let alt_text = parse_alt_text_helper(arena, &dict);

    let rect = dict.get(&arena.name("BBox")).and_then(|b| parse_bbox_helper(arena, b));
    let page_index = parse_page_index_helper(arena, &dict, walk.page_map).or(inherited.page);
    let lang = text_entry(arena, &dict, "Lang").or_else(|| inherited.lang.map(str::to_owned));
    let role = walk.roles.get(&tag).cloned();

    let id = *next_id;
    *next_id += 1;

    let below = Inherited { page: page_index, lang: lang.as_deref() };
    let kids = dict
        .get(&arena.name("K"))
        .map_or_else(Kids::default, |k| parse_kids_helper(arena, k, next_id, visited, walk, below));
    // An element with no `/Pg` of its own sits on the page its marks name, or — holding
    // no marks — on the page the first thing it holds is on. Taken after the kids rather
    // than before, because that is where both answers come from.
    //
    // **A container without this has no page at all**, and `volvo_xc90.pdf` is made of
    // them: only its `/MCR`s carry `/Pg`, so every `/Div` and every `/Sect` above them
    // came out unplaced. A consumer drawing "the elements on this page" then either drew
    // them on all 415 or on none.
    let page_index =
        page_index.or(kids.page).or_else(|| kids.children.first().and_then(|c| c.page_index));

    visited.remove(&handle);

    Some(StructureTreeNode {
        id,
        tag,
        title,
        alt_text,
        rect,
        page_index,
        handle_index: Some(handle.index()),
        mcids: kids.mcids,
        lang,
        role,
        children: kids.children,
    })
}
