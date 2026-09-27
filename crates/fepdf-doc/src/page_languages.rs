//! 11-001 (UA1:7.2): the natural language of the text in page content can be determined.
//!
//! **By the hierarchy ISO 32000-1 14.9.2.3 gives.** The catalogue's `/Lang` states the
//! language of all text; below it, a structure element's `/Lang` — its own or an
//! ancestor's — for the content its MCIDs mark, and a `Span` sequence's `/Lang` for content
//! outside the structure. An empty `/Lang` states that the language is unknown (14.9.2.2).
//! Text in an `/Artifact` is not the document's content and is not asked about.

use crate::structure::{AuditFinding, broken};
use fepdf_model::{Document, Object};
use std::collections::BTreeSet;

/// Whether the catalogue states a language, which settles every text's.
pub(crate) fn catalogue_states_one(doc: &Document) -> bool {
    let arena = doc.arena();
    doc.catalog_handle().is_some_and(|catalogue| {
        let lang = crate::audit_objects::entry(arena, &Object::Reference(catalogue), "Lang");
        crate::formula_marks::stated(arena, lang).unwrap_or(false)
    })
}

/// 11-001's finding, from the pages — counted from 0 — whose text no `/Lang` reaches.
pub(crate) fn report(
    doc: &Document,
    unlanguaged: &BTreeSet<usize>,
    findings: &mut Vec<AuditFinding>,
) {
    if unlanguaged.is_empty() || catalogue_states_one(doc) {
        return;
    }
    let pages: Vec<String> = unlanguaged.iter().take(10).map(|p| (p + 1).to_string()).collect();
    let more = if unlanguaged.len() > 10 { ", …" } else { "" };
    findings.push(broken(
        "11-001",
        format!(
            "The catalogue states no /Lang, and text on {} pages is in no structure element or \
             Span stating one — pages {}{more}",
            unlanguaged.len(),
            pages.join(", ")
        ),
    ));
}
