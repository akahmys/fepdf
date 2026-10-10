//! A page's annotations as a reviewer reads them: who made each, what it says, what it
//! answers, and the state each reviewer has given it (12.5.6.2, 12.5.6.3).
//!
//! **Read from the document as it stands**, so a list taken after an operation shows
//! what the operation did. Each comment carries its [`AnnotationAt`], which is what an
//! operation names it by ([ADR-0115]).
//!
//! [ADR-0115]: ../../../docs/adr/0115-an-operation-names-an-annotation-by-its-place-on-the-page.md

use crate::operation::{AnnotationAt, AnnotationState};
use fepdf_model::{Document, Handle, Object, PdfArena, PdfResult};
use serde::Serialize;
use std::collections::BTreeMap;

/// How far a chain of replies is followed (Rule 6).
const DEPTH: usize = 64;

/// One annotation on a page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Comment {
    /// Where it is, as an operation names it.
    pub at: AnnotationAt,
    /// `/Subtype`.
    pub subtype: String,
    /// `/NM`.
    pub name: Option<String>,
    /// `/T`: by convention, who made it.
    pub author: Option<String>,
    /// `/Contents`.
    pub contents: Option<String>,
    /// `/Subj`.
    pub subject: Option<String>,
    /// `/M`, as the file wrote it.
    pub modified: Option<String>,
    /// `/CreationDate`, as the file wrote it.
    pub created: Option<String>,
    /// `/Rect`, lower left and upper right.
    pub rect: [f64; 4],
    /// The index on this page of the annotation this one answers (`/IRT`), when that is
    /// on this page.
    pub reply_to: Option<usize>,
    /// Whether `/RT` is `Group`: this one is part of what it names, not an answer to it.
    pub grouped: bool,
    /// The state this annotation sets, when it is a state change (Table 175).
    pub sets: Option<AnnotationState>,
    /// The state each reviewer has given this annotation, the latest per reviewer and
    /// model, in `/Annots` order. Empty for an annotation nobody has given a state.
    pub states: Vec<StateMark>,
}

/// A reviewer's latest state for an annotation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateMark {
    /// Who, as `/T` says. Empty where the state change has no `/T`, which 12.5.6.3 does
    /// not allow and some files do anyway.
    pub author: String,
    /// The state.
    pub state: AnnotationState,
}

impl Comment {
    /// Whether this is an answer or a state change rather than a comment of its own: it
    /// answers another annotation and is not grouped with it.
    #[must_use]
    pub const fn is_reply(&self) -> bool {
        self.reply_to.is_some() && !self.grouped
    }
}

/// Every annotation on `page`, in `/Annots` order.
///
/// # Errors
/// Fails when the page is not there.
pub fn on_page(doc: &Document, page: usize) -> PdfResult<Vec<Comment>> {
    let arena = doc.arena();
    let page_dict = doc.resolve_to_dict(doc.page_handle(page)?)?;
    let entries = match arena.dict_entry(page_dict, arena.name("Annots")).map(|a| a.resolve(arena))
    {
        Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
        _ => Vec::new(),
    };
    let handles: Vec<Option<Handle<Object>>> = entries.iter().map(Object::as_reference).collect();
    let index_of: BTreeMap<Handle<Object>, usize> =
        handles.iter().enumerate().filter_map(|(i, h)| h.map(|h| (h, i))).collect();
    let mut comments: Vec<Comment> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| read(arena, entry, AnnotationAt { page, index }, &index_of))
        .collect();
    give_states(&mut comments);
    Ok(comments)
}

/// One `/Annots` entry as a comment.
fn read(
    arena: &PdfArena,
    entry: &Object,
    at: AnnotationAt,
    index_of: &BTreeMap<Handle<Object>, usize>,
) -> Comment {
    let dict = entry.resolve(arena).as_dict_handle();
    let get = |key: &str| dict.and_then(|d| arena.dict_entry(d, arena.name(key)));
    let text = |key: &str| get(key).and_then(|v| crate::apply::fields::text_of(arena, &v));
    let name = |key: &str| get(key).and_then(|v| v.as_name()).and_then(|n| arena.get_name_str(n));
    let sets = match (text("State"), text("StateModel")) {
        (Some(state), Some(model)) => AnnotationState::named(&state, &model),
        _ => None,
    };
    Comment {
        at,
        subtype: name("Subtype").unwrap_or_default(),
        name: text("NM"),
        author: text("T"),
        contents: text("Contents"),
        subject: text("Subj"),
        modified: text("M"),
        created: text("CreationDate"),
        rect: rect(arena, get("Rect")),
        reply_to: get("IRT").and_then(|v| v.as_reference()).and_then(|h| index_of.get(&h).copied()),
        grouped: name("RT").as_deref() == Some("Group"),
        sets,
        states: Vec::new(),
    }
}

/// Gives each comment the latest state each reviewer set on it: the last state change,
/// in `/Annots` order, by that author in that model, whose chain of replies reaches it.
fn give_states(comments: &mut [Comment]) {
    let replies: Vec<(usize, Option<usize>, Option<String>, Option<AnnotationState>)> =
        comments.iter().map(|c| (c.at.index, c.reply_to, c.author.clone(), c.sets)).collect();
    let answered = |from: usize| -> Option<usize> {
        let mut at = from;
        for _ in 0..DEPTH {
            let reply = replies.get(at).and_then(|r| r.1)?;
            if replies.get(reply).and_then(|r| r.3).is_none() {
                return Some(reply);
            }
            at = reply;
        }
        None
    };
    let mut latest: BTreeMap<(usize, String, &'static str), AnnotationState> = BTreeMap::new();
    for (index, _, author, sets) in &replies {
        let (Some(state), Some(target)) = (sets, answered(*index)) else { continue };
        let author = author.clone().unwrap_or_default();
        latest.insert((target, author, state.model()), *state);
    }
    for ((target, author, _), state) in latest {
        if let Some(comment) = comments.get_mut(target) {
            comment.states.push(StateMark { author, state });
        }
    }
}

fn rect(arena: &PdfArena, value: Option<Object>) -> [f64; 4] {
    let numbers: Vec<f64> = value
        .and_then(|v| v.resolve(arena).as_array())
        .and_then(|a| arena.get_array(a))
        .unwrap_or_default()
        .iter()
        .filter_map(|n| n.resolve(arena).as_f64())
        .collect();
    match numbers[..] {
        [x0, y0, x1, y1] => [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)],
        _ => [0.0; 4],
    }
}
