//! What a reviewer does to an annotation already on a page: removes it, edits its words,
//! answers it, gives it a state (12.5.6.2, 12.5.6.3).
//!
//! **Each names its annotation by page and place in `/Annots`** ([ADR-0115]). Nothing is
//! written at load to make that possible, and a replay of the same operations on the same
//! origin finds the same annotation every time.
//!
//! [ADR-0115]: ../../../../docs/adr/0115-an-operation-names-an-annotation-by-its-place-on-the-page.md

use super::redact_annots;
use crate::operation::{AnnotationAt, AnnotationState, Authorship};
use fepdf_model::{DictHandle, Document, Handle, Missing, Object, PdfArena, PdfError, PdfResult};
use std::collections::{BTreeMap, BTreeSet};

/// How far a chain of replies is followed (Rule 6).
const DEPTH: usize = 64;

type Dict = BTreeMap<Handle<fepdf_model::PdfName>, Object>;

/// Removes the annotation `at` names, and what goes with it (12.5.6.2): its pop-up, every
/// reply, and the structure tree's references to them.
///
/// # Errors
/// Fails when there is no such annotation, or it is a widget.
pub fn apply_remove(doc: &Document, at: AnnotationAt) -> PdfResult<()> {
    let handle = handle_at(doc, at)?;
    if subtype_is(doc.arena(), handle, "Widget") {
        return Err(PdfError::refused(
            "RemoveAnnotation",
            "a widget is half of a form field, and a field is removed as a field",
        ));
    }
    redact_annots::remove_handles(doc, BTreeSet::from([handle]))
}

/// Replaces the words the annotation `at` names carries, and says when.
///
/// # Errors
/// Fails when there is no such annotation, or it is a free text annotation, whose
/// appearance draws the words it would no longer carry.
pub fn apply_edit(
    doc: &Document,
    at: AnnotationAt,
    contents: &str,
    when: Option<&str>,
) -> PdfResult<()> {
    let arena = doc.arena();
    let handle = handle_at(doc, at)?;
    if subtype_is(arena, handle, "FreeText") {
        return Err(PdfError::refused(
            "EditAnnotation",
            "a free text annotation draws its words, and the drawing would keep the old ones; \
             remove it and add another",
        ));
    }
    let dict = dict_of(arena, handle)?;
    let mut entries = arena.get_dict(dict).unwrap_or_default();
    entries.insert(arena.name("Contents"), Object::Text(contents.to_owned()));
    if let Some(when) = when {
        entries.insert(arena.name("M"), Object::Text(when.to_owned()));
    }
    arena.set_dict(dict, entries);
    Ok(())
}

/// Answers the annotation `at` names: a text annotation in reply to it (12.5.6.2, `/IRT`).
///
/// **An appearance that draws nothing**, because the clause says a reply is shown with
/// what it answers and not on its own, and Table 166 still requires one (ADR-0118). It
/// takes the answered annotation's `/Rect` so that a reader which does place it puts it
/// there.
///
/// # Errors
/// Fails when there is no such annotation.
pub fn apply_reply(
    doc: &Document,
    at: AnnotationAt,
    contents: &str,
    by: &Authorship,
) -> PdfResult<()> {
    let answered = handle_at(doc, at)?;
    let mut reply = reply_to(doc, at.page, answered, by)?;
    reply.insert(doc.arena().name("Contents"), Object::Text(contents.to_owned()));
    append(doc, at.page, reply)
}

/// Sets the state of the annotation `at` names, for `by.author` (12.5.6.3).
///
/// Written as a text annotation with `/State` and `/StateModel`, in reply to the
/// annotation the first time this author sets a state in this model, and to their last
/// such reply after that, as the clause says "additional state changes" are made.
///
/// # Errors
/// Fails when there is no such annotation, or no author is given: the clause says the
/// reply's `/T` "shall specify the user".
pub fn apply_state(
    doc: &Document,
    at: AnnotationAt,
    state: AnnotationState,
    by: &Authorship,
) -> PdfResult<()> {
    let arena = doc.arena();
    let author =
        by.author.as_deref().map(str::trim).filter(|a| !a.is_empty()).ok_or_else(|| {
            PdfError::refused(
                "SetAnnotationState",
                "12.5.6.3 requires the user who sets a state, as /T",
            )
        })?;
    let target = handle_at(doc, at)?;
    let previous = last_state_by(doc, at.page, target, author, state.model())?;
    let mut reply = reply_to(doc, at.page, previous.unwrap_or(target), by)?;
    reply.insert(arena.name("State"), Object::Text(state.name().to_owned()));
    reply.insert(arena.name("StateModel"), Object::Text(state.model().to_owned()));
    // A state change keeps the original's place: it is the original's state, whichever
    // reply in the chain it answers.
    if let Some(rect) = entry(arena, target, "Rect") {
        reply.insert(arena.name("Rect"), rect);
    }
    append(doc, at.page, reply)
}

/// Writes who made a markup annotation and when, and gives it a name unique on `page`
/// (Table 166 `/NM`, Table 172 `/T`, `/CreationDate`, Table 166 `/M`).
///
/// # Errors
/// Fails when the page is not there.
pub fn sign(doc: &Document, page: usize, dict: &mut Dict, by: &Authorship) -> PdfResult<()> {
    let arena = doc.arena();
    dict.insert(arena.name("NM"), Object::Text(fresh_name(doc, page)?));
    if let Some(author) = by.author.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
        dict.insert(arena.name("T"), Object::Text(author.to_owned()));
    }
    if let Some(when) = by.when.as_deref() {
        dict.insert(arena.name("CreationDate"), Object::Text(when.to_owned()));
        dict.insert(arena.name("M"), Object::Text(when.to_owned()));
    }
    Ok(())
}

/// A text annotation in reply to `to`, on `page`, made by `by`.
fn reply_to(doc: &Document, page: usize, to: Handle<Object>, by: &Authorship) -> PdfResult<Dict> {
    let arena = doc.arena();
    let mut dict = Dict::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Annot")));
    dict.insert(arena.name("Subtype"), Object::Name(arena.name("Text")));
    if let Some(rect) = entry(arena, to, "Rect") {
        dict.insert(arena.name("Rect"), rect);
    }
    dict.insert(arena.name("P"), Object::Reference(doc.page_handle(page)?));
    dict.insert(arena.name("IRT"), Object::Reference(to));
    dict.insert(arena.name("AP"), super::markup::nothing_drawn(arena));
    sign(doc, page, &mut dict, by)?;
    Ok(dict)
}

/// Puts `dict` at the end of `page`'s `/Annots`.
fn append(doc: &Document, page: usize, dict: Dict) -> PdfResult<()> {
    let arena = doc.arena();
    let handle = arena.alloc_object(Object::Dictionary(arena.alloc_dict(dict)));
    super::annotations::append_to_page(doc, doc.page_handle(page)?, handle)
}

/// `author`'s last state reply in `model` whose chain of `/IRT` reaches `target`, in
/// `/Annots` order.
fn last_state_by(
    doc: &Document,
    page: usize,
    target: Handle<Object>,
    author: &str,
    model: &str,
) -> PdfResult<Option<Handle<Object>>> {
    let arena = doc.arena();
    let text = |handle: Handle<Object>, key: &str| text_of(arena, handle, key);
    Ok(redact_annots::annotations_on(doc, page)?
        .iter()
        .filter_map(Object::as_reference)
        .filter(|a| text(*a, "StateModel").as_deref() == Some(model))
        .filter(|a| text(*a, "T").as_deref().map(str::trim) == Some(author))
        .filter(|a| reaches(arena, *a, target))
        .last())
}

/// Whether following `from`'s `/IRT` arrives at `target`.
fn reaches(arena: &PdfArena, from: Handle<Object>, target: Handle<Object>) -> bool {
    let mut at = from;
    for _ in 0..DEPTH {
        match entry(arena, at, "IRT").and_then(|irt| irt.as_reference()) {
            Some(to) if to == target => return true,
            Some(to) => at = to,
            None => return false,
        }
    }
    false
}

/// `fepdf-N` for the smallest `N` no annotation on `page` is named.
///
/// **Counted, not random**, so a replay of the same history writes the same names.
fn fresh_name(doc: &Document, page: usize) -> PdfResult<String> {
    let arena = doc.arena();
    let taken: BTreeSet<String> = redact_annots::annotations_on(doc, page)?
        .iter()
        .filter_map(Object::as_reference)
        .filter_map(|a| text_of(arena, a, "NM"))
        .collect();
    // One more candidate than there are names, so one of them is free.
    let candidates = 1..=taken.len().saturating_add(1);
    Ok(candidates
        .map(|n| format!("fepdf-{n}"))
        .find(|name| !taken.contains(name))
        .unwrap_or_default())
}

/// The handle of the annotation `at` names.
fn handle_at(doc: &Document, at: AnnotationAt) -> PdfResult<Handle<Object>> {
    let annotations = redact_annots::annotations_on(doc, at.page)?;
    let count = annotations.len();
    let found = annotations
        .get(at.index)
        .ok_or(PdfError::NotFound(Missing::Annotation { index: at.index, count }))?;
    found.as_reference().ok_or_else(|| {
        PdfError::refused(
            "an annotation operation",
            "this annotation is written inside /Annots, where 7.7.3.3 asks for a reference",
        )
    })
}

fn dict_of(arena: &PdfArena, handle: Handle<Object>) -> PdfResult<DictHandle> {
    arena
        .get_object(handle)
        .and_then(|o| o.as_dict_handle())
        .ok_or_else(|| PdfError::violation("12.5.2", "an annotation is not a dictionary"))
}

fn entry(arena: &PdfArena, handle: Handle<Object>, key: &str) -> Option<Object> {
    let dict = arena.get_object(handle)?.as_dict_handle()?;
    arena.dict_entry(dict, arena.name(key))
}

fn text_of(arena: &PdfArena, handle: Handle<Object>, key: &str) -> Option<String> {
    super::fields::text_of(arena, &entry(arena, handle, key)?.resolve(arena))
}

fn subtype_is(arena: &PdfArena, handle: Handle<Object>, subtype: &str) -> bool {
    entry(arena, handle, "Subtype").and_then(|s| s.as_name()) == Some(arena.name(subtype))
}
