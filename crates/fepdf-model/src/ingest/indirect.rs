//! Dictionaries written in place where the reader needs an object, given one at load.
//!
//! **A node of a tree is found by the object that holds it.** The outline and structure
//! readers walk `/First`, `/Next` and `/K` by reference, and a dictionary written directly
//! has no number to walk by. The function they shared answered one by taking the
//! dictionary's index in the `dicts` pool as an index in the `objects` pool, so a direct
//! `/Outlines` read as whatever object shared its number — no bookmarks, in the case
//! measured (ROADMAP Y-F21). Lifted here, a direct node reaches every reader as a
//! reference, as `lift_direct_fonts` does for a font.
//!
//! Table 29 and Table 151 say the outline's dictionaries *shall be* indirect references,
//! so a direct one is a repair and is recorded. A structure element in `/K` may be written
//! in place (Table 355), so lifting one changes nothing the file said and records nothing.

use crate::arena::PdfArena;
use crate::handle::Handle;
use crate::interpretation::{Decision, DecisionLog};
use crate::object::Object;

/// How deep an outline or structure tree is followed: the bound the readers use.
const MAX_DEPTH: usize = 64;

/// Lifts what the catalogue at `root` reaches of the outline and the structure tree.
pub fn lift_direct_nodes(arena: &PdfArena, root: Handle<Object>, decisions: &mut DecisionLog) {
    let catalog = Object::Reference(root);
    if lift_entry(arena, &catalog, "Outlines") | lift_outline_items(arena, &catalog) {
        decisions.push(Decision::repaired(
            "7.7.2",
            "an outline dictionary is written in place, where Table 29 and Table 151 say an \
             indirect reference",
            "gave it an object of its own",
        ));
    }
    if let Some(tree) = crate::access::entry(arena, &catalog, "StructTreeRoot") {
        lift_structure_elements(arena, &tree);
    }
}

/// Replaces `key` of `holder` with a reference to an object of its own when its value
/// is a dictionary written in place; whether it did.
fn lift_entry(arena: &PdfArena, holder: &Object, key: &str) -> bool {
    let Some(dh) = holder.resolve(arena).as_dict_handle() else { return false };
    let Some(mut dict) = arena.get_dict(dh) else { return false };
    let name = arena.name(key);
    let Some(Object::Dictionary(direct)) = dict.get(&name).cloned() else { return false };
    dict.insert(name, Object::Reference(arena.alloc_object(Object::Dictionary(direct))));
    arena.set_dict(dh, dict);
    true
}

/// Lifts every outline item written in place under the outline root, level by level, and
/// mends `/Parent`, `/Prev` and `/Last` of a level that had one: a copy written twice in
/// place is two dictionaries, and the links have to name the one that was lifted.
fn lift_outline_items(arena: &PdfArena, catalog: &Object) -> bool {
    let Some(outlines) = crate::access::dict_of(arena, catalog)
        .and_then(|c| c.get(&arena.name("Outlines")).cloned())
    else {
        return false;
    };
    let mut lifted = false;
    let mut levels = vec![(outlines, 0usize)];
    while let Some((parent, depth)) = levels.pop() {
        if depth >= MAX_DEPTH {
            continue;
        }
        let (items, changed) = lift_level(arena, &parent);
        if changed {
            mend_level(arena, &parent, &items);
            lifted = true;
        }
        levels.extend(items.into_iter().map(|item| (Object::Reference(item), depth + 1)));
    }
    lifted
}

/// The items of one level under `parent`, each lifted if it was written in place, and
/// whether any was.
fn lift_level(arena: &PdfArena, parent: &Object) -> (Vec<Handle<Object>>, bool) {
    let (mut items, mut changed) = (Vec::new(), lift_entry(arena, parent, "First"));
    let mut current =
        crate::access::dict_of(arena, parent).and_then(|d| d.get(&arena.name("First")).cloned());
    while let Some(Object::Reference(item)) = current {
        if items.contains(&item) || items.len() > 1_000_000 {
            break;
        }
        items.push(item);
        changed |= lift_entry(arena, &Object::Reference(item), "Next");
        current = crate::access::dict_of(arena, &Object::Reference(item))
            .and_then(|d| d.get(&arena.name("Next")).cloned());
    }
    (items, changed)
}

/// Points `/Parent`, `/Prev` and the parent's `/Last` at the lifted items of a level.
fn mend_level(arena: &PdfArena, parent: &Object, items: &[Handle<Object>]) {
    let parent_ref = match parent {
        Object::Reference(h) => Some(*h),
        _ => None,
    };
    for (i, &item) in items.iter().enumerate() {
        let Some(dh) = arena.get_object(item).and_then(|o| o.as_dict_handle()) else { continue };
        let mut dict = arena.get_dict(dh).unwrap_or_default();
        if let Some(p) = parent_ref {
            dict.insert(arena.name("Parent"), Object::Reference(p));
        }
        match i.checked_sub(1).and_then(|p| items.get(p)) {
            Some(&prev) => dict.insert(arena.name("Prev"), Object::Reference(prev)),
            None => dict.remove(&arena.name("Prev")),
        };
        arena.set_dict(dh, dict);
    }
    if let (Some(&last), Some(dh)) = (items.last(), parent.resolve(arena).as_dict_handle()) {
        let mut dict = arena.get_dict(dh).unwrap_or_default();
        dict.insert(arena.name("Last"), Object::Reference(last));
        arena.set_dict(dh, dict);
    }
}

/// Lifts every structure element written in place in a `/K`, from `tree` down.
///
/// A dictionary in `/K` is a structure element unless it says it is a marked-content or
/// an object reference (`/Type /MCR`, `/Type /OBJR`), which stay where they are written.
fn lift_structure_elements(arena: &PdfArena, tree: &Object) {
    let mut stack = vec![(tree.clone(), 0usize)];
    while let Some((node, depth)) = stack.pop() {
        if depth >= MAX_DEPTH {
            continue;
        }
        let Some(dh) = node.resolve(arena).as_dict_handle() else { continue };
        let Some(mut dict) = arena.get_dict(dh) else { continue };
        let key = arena.name("K");
        let Some(kids) = dict.get(&key).cloned() else { continue };
        let lifted = lift_kids(arena, &kids, &mut stack, depth);
        if lifted != kids {
            dict.insert(key, lifted);
            arena.set_dict(dh, dict);
        }
    }
}

/// `kids` with each structure element written in place replaced by a reference, and every
/// element queued on `stack` to be walked.
fn lift_kids(
    arena: &PdfArena,
    kids: &Object,
    stack: &mut Vec<(Object, usize)>,
    depth: usize,
) -> Object {
    let mut one = |kid: &Object| -> Object {
        match kid {
            Object::Dictionary(d) if !is_reference_dict(arena, *d) => {
                let lifted = Object::Reference(arena.alloc_object(Object::Dictionary(*d)));
                stack.push((lifted.clone(), depth + 1));
                lifted
            }
            Object::Reference(_) => {
                stack.push((kid.clone(), depth + 1));
                kid.clone()
            }
            other => other.clone(),
        }
    };
    match kids {
        Object::Array(ah) => {
            let items = arena.get_array(*ah).unwrap_or_default();
            let lifted: Vec<Object> = items.iter().map(&mut one).collect();
            if lifted != items {
                return Object::Array(arena.alloc_array(lifted));
            }
            kids.clone()
        }
        other => one(other),
    }
}

/// Whether the dictionary `d` is a marked-content or an object reference (Tables 357 and
/// 358) rather than a structure element.
fn is_reference_dict(arena: &PdfArena, d: crate::handle::DictHandle) -> bool {
    let ty = arena
        .dict_entry(d, arena.name("Type"))
        .and_then(|t| t.resolve(arena).as_name())
        .and_then(|n| arena.get_name_str(n));
    let has = |key: &str| arena.dict_entry(d, arena.name(key)).is_some();
    matches!(ty.as_deref(), Some("MCR" | "OBJR")) || (ty.is_none() && (has("MCID") || has("Obj")))
}
