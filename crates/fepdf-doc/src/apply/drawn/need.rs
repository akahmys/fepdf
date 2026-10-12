//! `/NeedAppearances true`, done as it asks and then dropped (Table 224; ROADMAP AB-5,
//! [ADR-0126](../../../../../docs/adr/0126-need-appearances-is-done-as-it-asks-and-then-dropped.md)).
//!
//! **The entry tells a reader to build every widget's appearance**, and a producer sets it
//! after changing `/V` without changing an `/AP`: the appearances in the file may show what
//! the fields held before. 2.0 deprecates the entry and requires the appearances. So when
//! a form says it, the widgets that have an appearance are rebuilt from their values
//! (`widgets::rebuild`), [`super::give_missing`] gives the others theirs, and the entry
//! goes once every widget that needs an appearance has one. Where one still lacks it, the
//! entry stays: it is the only thing left saying the appearance is to be built.

use super::widgets;
use fepdf_model::interpretation::Decision;
use fepdf_model::{DictHandle, Document, Object};

/// The interactive form's dictionary, when it says `/NeedAppearances true`.
pub(super) fn asked(doc: &Document) -> Option<DictHandle> {
    let arena = doc.arena();
    let catalog = doc.catalog_handle().and_then(|c| doc.resolve_to_dict(c).ok())?;
    let form =
        arena.dict_entry(catalog, arena.name("AcroForm"))?.resolve(arena).as_dict_handle()?;
    let needed = arena.dict_entry(form, arena.name("NeedAppearances"))?.resolve(arena);
    matches!(needed, Object::Boolean(true)).then_some(form)
}

/// What rebuilding the widgets that had an appearance came to.
#[derive(Default)]
pub(super) struct Rebuilt {
    done: usize,
    failed: Vec<String>,
}

impl Rebuilt {
    /// Rebuilds `annot` if it is a widget with an appearance; anything else is left to
    /// [`super::give_missing`]'s own pass.
    pub(super) fn widget(&mut self, doc: &Document, (page, index): (usize, usize), annot: &Object) {
        let arena = doc.arena();
        let Some(handle) = annot.resolve(arena).as_dict_handle() else { return };
        let is_widget = arena
            .dict_entry(handle, arena.name("Subtype"))
            .and_then(|s| s.as_name())
            .and_then(|n| arena.get_name_str(n))
            .is_some_and(|s| s == "Widget");
        if !is_widget || arena.dict_entry(handle, arena.name("AP")).is_none() {
            return;
        }
        match widgets::rebuild(doc, handle) {
            Ok(true) => self.done += 1,
            Ok(false) => self.failed.push(format!("page {page}, annotation {index}")),
            Err(why) => self.failed.push(format!("page {page}, annotation {index}: {why}")),
        }
    }

    /// Drops the form's `/NeedAppearances` where every widget that needs an appearance has
    /// one, and records what was done either way.
    pub(super) fn settle(self, doc: &Document, form: DictHandle) {
        let arena = doc.arena();
        let lacking = lacking(doc);
        if lacking == 0 && self.failed.is_empty() {
            let mut dict = arena.get_dict(form).unwrap_or_default();
            dict.remove(&arena.name("NeedAppearances"));
            arena.set_dict(form, dict);
            doc.record(Decision::repaired(
                "12.7.3",
                format!(
                    "the form says /NeedAppearances true, which 2.0 deprecates; {} widget \
                     appearances were rebuilt from their values",
                    self.done
                ),
                "dropped the entry: every widget carries the appearance it asked for (Table 224)",
            ));
            return;
        }
        doc.record(Decision::violation(
            "12.7.3",
            format!(
                "the form says /NeedAppearances true; {lacking} widgets still lack an appearance \
                 and {} could not be rebuilt {:?}",
                self.failed.len(),
                self.failed
            ),
            "kept the entry: it is all that says those appearances are still to be built",
        ));
    }
}

/// How many widgets lack an appearance they need: one with no `/AP` whose `/Rect` is more
/// than a point, which Table 166 exempts.
fn lacking(doc: &Document) -> usize {
    let arena = doc.arena();
    let Ok(pages) = doc.page_count() else { return 0 };
    let mut count = 0;
    for page in 0..pages {
        let Ok(annots) = super::super::redact_annots::annotations_on(doc, page) else { continue };
        for annot in annots {
            let Some(handle) = annot.resolve(arena).as_dict_handle() else { continue };
            let dict = arena.get_dict(handle).unwrap_or_default();
            let entries = super::Entries::new(arena, &dict);
            let widget = entries.name("Subtype").is_some_and(|s| s == "Widget");
            let point = entries.rect().is_none_or(|a| a.width() <= 0.0 && a.height() <= 0.0);
            if widget && !point && !dict.contains_key(&arena.name("AP")) {
                count += 1;
            }
        }
    }
    count
}
