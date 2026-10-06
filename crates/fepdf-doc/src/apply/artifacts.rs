//! Marking tagged content as an artifact (ISO 32000-2 14.8.2.2; WTPDF 8.3).
//!
//! **The sequence changes what it is, and the structure tree lets go of it.** A marked
//! sequence carrying an `/MCID` is content a structure element claims; an `/Artifact`
//! sequence is content no element may claim (14.8.2.2.1). So the `BDC` becomes an
//! `/Artifact` one, and the element's `/K` and the parent tree's entry stop naming the
//! MCID — or the page would hold an artefact the tree still calls content.

use crate::apply::text::{Content, page_commands, write_page_content};
use fepdf_model::object::PdfName;
use fepdf_model::object::sublimation::{Command, IrObject};
use fepdf_model::{Document, Handle, Object, PdfArena, PdfError, PdfResult};
use std::collections::BTreeMap;

/// Marks the sequence with `mcid` on page `page` as an artifact.
///
/// Of `kind` (`/Type`: `Pagination`, `Layout`, `Page`, `Background`) and `subtype`
/// (`/Subtype`: `Header`, `Footer`, `Watermark`) where given.
///
/// # Errors
/// Refused when the page has no such sequence, or one with another MCID inside it — which
/// would become tagged content inside an artefact (01-004).
pub fn apply_mark_artifact(
    doc: &Document,
    page: usize,
    mcid: i64,
    (kind, subtype): (Option<&str>, Option<&str>),
) -> PdfResult<()> {
    let arena = doc.arena();
    let properties = page_properties(doc, page);
    let content = page_commands(doc, page, &BTreeMap::new())?.ok_or_else(|| {
        PdfError::refused("MarkArtifact", format!("page {} draws nothing", page + 1))
    })?;
    let mut commands: Vec<Command> = match content {
        Content::Shared(_) => content.commands().cloned().collect(),
        Content::Parsed(commands) => commands,
    };
    let carried = |props: Option<&IrObject>| mcid_of(arena, props, &properties);
    let at = commands
        .iter()
        .position(|c| matches!(c, Command::BeginMarkedContent { properties, .. } if carried(properties.as_ref()) == Some(mcid)))
        .ok_or(PdfError::NotFound(fepdf_model::Missing::Mark { page, mcid }))?;
    if nested_mcid(&commands[at + 1..], &carried) {
        return Err(PdfError::refused(
            "MarkArtifact",
            format!(
                "the sequence with MCID {mcid} holds tagged content, which an artifact may not"
            ),
        ));
    }
    let mut entries = BTreeMap::new();
    if let Some(kind) = kind {
        entries.insert("Type".to_string(), IrObject::Name(kind.to_string()));
    }
    if let Some(subtype) = subtype {
        entries.insert("Subtype".to_string(), IrObject::Name(subtype.to_string()));
    }
    commands[at] = Command::BeginMarkedContent {
        tag: PdfName::new("Artifact"),
        properties: (!entries.is_empty()).then_some(IrObject::Dictionary(entries)),
    };
    let bytes = fepdf_model::object::sublimation::serializer::serialize_commands(&commands);
    write_page_content(doc, page, bytes)?;
    release(doc, page, mcid);
    Ok(())
}

/// The MCID a sequence's property list carries, inline or through the page's `/Properties`.
fn mcid_of(
    arena: &PdfArena,
    properties: Option<&IrObject>,
    named: &BTreeMap<String, Handle<Object>>,
) -> Option<i64> {
    match properties? {
        IrObject::Dictionary(inline) => match inline.get("MCID")? {
            IrObject::Integer(mcid) => Some(*mcid),
            _ => None,
        },
        IrObject::Name(name) => {
            let list = arena.get_object(*named.get(name)?)?.as_dict_handle()?;
            arena.dict_entry(list, arena.name("MCID"))?.as_integer()
        }
        _ => None,
    }
}

/// Whether a sequence opened at the start of `after` holds another carrying an MCID, before
/// the `EMC` that closes it.
fn nested_mcid(after: &[Command], carried: &dyn Fn(Option<&IrObject>) -> Option<i64>) -> bool {
    let mut depth = 1_usize;
    for command in after {
        match command {
            Command::BeginMarkedContent { properties, .. } => {
                if carried(properties.as_ref()).is_some() {
                    return true;
                }
                depth += 1;
            }
            Command::EndMarkedContent => {
                depth -= 1;
                if depth == 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    false
}

/// The page's `/Properties` resource, by name.
fn page_properties(doc: &Document, page: usize) -> BTreeMap<String, Handle<Object>> {
    let arena = doc.arena();
    let Some(handle) = doc.get_page_handle(page) else { return BTreeMap::new() };
    let resources =
        fepdf_model::Page::new(arena, handle, doc.get_parent_chain(handle)).resources_handle();
    fepdf_model::access::names_in(arena, resources, "Properties")
}

/// Stops the structure tree claiming `mcid` on `page`: the parent tree's entry for it
/// becomes `null`, and the element it named drops it from `/K`.
fn release(doc: &Document, page: usize, mcid: i64) {
    let arena = doc.arena();
    let (Some(root), Some(page_handle)) =
        (doc.get_structure_root().ok().flatten(), doc.get_page_handle(page))
    else {
        return;
    };
    let key = arena
        .get_object(page_handle)
        .and_then(|p| p.as_dict_handle())
        .and_then(|p| arena.dict_entry(p, arena.name("StructParents")))
        .and_then(|k| k.as_integer());
    let Some(array) = key.and_then(|k| crate::parent_tree::array_for(arena, root, k)) else {
        return;
    };
    let mut elements = arena.get_array(array).unwrap_or_default();
    let Some(slot) = usize::try_from(mcid).ok().and_then(|i| elements.get_mut(i)) else { return };
    let element = std::mem::replace(slot, Object::Null);
    arena.set_array(array, elements);
    if let Some(element) = element.as_reference() {
        drop_kid(arena, element, mcid, page_handle);
    }
}

/// Removes `mcid` on `page` from `element`'s `/K`: an integer, when the element's own page
/// is the page, or a marked-content reference whose `/Pg`, or where it states none the
/// element's, is the page.
///
/// **An integer is on the element's page** (14.7.4.2), and it was dropped on its number
/// alone: an element with MCID 0 on its own page and a reference to MCID 0 on another lost
/// both when the other page's was marked an artifact.
fn drop_kid(arena: &PdfArena, element: Handle<Object>, mcid: i64, page: Handle<Object>) {
    let Some(dh) = arena.get_object(element).and_then(|e| e.as_dict_handle()) else { return };
    let mut dict = arena.get_dict(dh).unwrap_or_default();
    let key = arena.name("K");
    let own = dict.get(&arena.name("Pg")).and_then(Object::as_reference);
    let on_page = |pg: Option<Handle<Object>>| pg.or(own).is_none_or(|p| p == page);
    let names_it = |kid: &Object| match kid.resolve(arena) {
        Object::Integer(k) => k == mcid && on_page(None),
        other => other.as_dict_handle().is_some_and(|d| {
            let pg = arena.dict_entry(d, arena.name("Pg")).and_then(|p| p.as_reference());
            arena.dict_entry(d, arena.name("MCID")).and_then(|m| m.as_integer()) == Some(mcid)
                && on_page(pg)
        }),
    };
    let Some(kids) = dict.get(&key).map(|k| super::wrap::kids(arena, k)) else { return };
    let kept: Vec<Object> = kids.into_iter().filter(|k| !names_it(k)).collect();
    dict.insert(key, Object::Array(arena.alloc_array(kept)));
    arena.set_dict(dh, dict);
}
