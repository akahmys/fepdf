//! Marked content (14.6) and the one thing it decides about drawing: optional content
//! (8.11.3.1).
//!
//! This file used to read, in full, `let _props = self.stack.pop();` under the comment
//! "Skeleton: just pop for now" — so a `/OC /MC0 BDC` was a `BDC` like any other and its
//! group's state was never consulted. See [`fepdf_model::optional_content`] for what the
//! property list is worth and why nothing is hidden on a doubt.

use crate::interpreter::Interpreter;
use fepdf_model::interpretation::Decision;
use fepdf_model::optional_content::{Membership, OptionalContentState};
use fepdf_model::{Object, PdfName, PdfResult};

/// One open marked-content section.
///
/// The interpreter keeps a stack of these rather than a count of hidden ones, because
/// `EMC` has to know *which* kind it is closing: a `/Span` opened inside a hidden `/OC`
/// section closes with an `EMC` that must not bring the page back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkedSection {
    /// A section that changed nothing about visibility.
    Drawn,
    /// An `/OC` section whose group is off. Marks are withheld until its `EMC`.
    Hidden,
}

impl Interpreter<'_> {
    pub(crate) fn handle_marked_content_operator(&mut self, op: &str) -> PdfResult<()> {
        match op {
            // `BMC` carries a tag and no property list, so it can never name a group.
            "BMC" => {
                let _tag = self.pop_name()?;
                self.marked_sections.push(MarkedSection::Drawn);
            }
            "BDC" => {
                let properties = self.stack.pop();
                let tag = self.pop_name()?;
                self.begin_marked_content(&tag, properties.as_ref());
            }
            // 14.6.2's point operators open nothing, so they close nothing either. `DP`
            // pops the same two operands as `BDC` and leaves the section stack alone.
            "MP" => {
                let _tag = self.pop_name()?;
            }
            "DP" => {
                let _properties = self.stack.pop();
                let _tag = self.pop_name()?;
            }
            "EMC" => self.end_marked_content(),
            _ => {}
        }
        Ok(())
    }

    /// Opens a section, hiding what follows when the tag is `/OC` and the group is off.
    pub(crate) fn begin_marked_content(&mut self, tag: &PdfName, properties: Option<&Object>) {
        if tag.as_str() != "OC" {
            self.marked_sections.push(MarkedSection::Drawn);
            return;
        }
        let section = match self.optional_content_membership(properties) {
            Membership::Hidden => {
                self.backend.hide();
                MarkedSection::Hidden
            }
            Membership::Visible => MarkedSection::Drawn,
            Membership::Unreadable(why) => {
                self.doc.record(Decision::violation(
                    "8.11.3.1",
                    format!("a /OC marked-content section could not be resolved: {why}"),
                    "drew the section; a group that is off would have hidden it",
                ));
                MarkedSection::Drawn
            }
        };
        self.marked_sections.push(section);
    }

    /// Closes the innermost section.
    ///
    /// An `EMC` with nothing open is ignored rather than treated as an error: the file is
    /// wrong, and refusing the operator would abort the content stream and take the rest
    /// of the page's text with it — the failure ADR-0018 was written about.
    pub(crate) fn end_marked_content(&mut self) {
        if self.marked_sections.pop() == Some(MarkedSection::Hidden) {
            self.backend.reveal();
        }
    }

    /// What the document says about a `/OC` property list (8.11.3.1).
    ///
    /// The operand is a name into the page's `/Properties` resources, or — legally, and
    /// uselessly for optional content — a dictionary written in place. A group has to be
    /// an indirect object (8.11.2), so an inline one names nothing that `/OCProperties`
    /// could have turned off.
    fn optional_content_membership(&mut self, properties: Option<&Object>) -> Membership {
        let Some(operand) = properties.cloned() else {
            return Membership::Unreadable("/OC was written with no property list".to_string());
        };
        let referenced = match operand {
            Object::Name(handle) => {
                let arena = self.doc.arena();
                let Some(name) = arena.get_name(handle) else {
                    return Membership::Unreadable("the /OC operand is not a name".to_string());
                };
                let key = arena.intern_name(PdfName::new("Properties"));
                // The lookup's own error is not carried through: it can only ever be
                // "resource not found", and its `Handle<PdfName>(16)` says nothing to
                // anyone reading a decision log.
                match self.find_resource(&key, &name) {
                    Ok(entry) => entry,
                    Err(_) => {
                        return Membership::Unreadable(format!(
                            "/{} is not in the page's /Properties (8.11.3.1)",
                            name.as_str()
                        ));
                    }
                }
            }
            other => other,
        };
        self.membership_of(&referenced)
    }

    /// What the document's optional content state says about one object, reading that
    /// state on first use.
    ///
    /// `doc` is copied out of `self` first because it is a shared reference with the
    /// interpreter's own lifetime: the arena it hands back does not borrow `self`, and
    /// filling the cache does.
    fn membership_of(&mut self, object: &Object) -> Membership {
        let doc = self.doc;
        let state = self.optional_content.get_or_insert_with(|| OptionalContentState::read(doc));
        state.membership(doc.arena(), object)
    }

    /// Whether an `/OC` entry on an XObject or an annotation turns it off (8.11.3.2).
    ///
    /// Separate from the marked-content path because there is no section to open: an
    /// XObject is either drawn or it is not.
    pub(crate) fn optional_content_hides(&mut self, entry: &Object, what: &str) -> bool {
        match self.membership_of(entry) {
            Membership::Hidden => true,
            Membership::Visible => false,
            Membership::Unreadable(why) => {
                self.doc.record(Decision::violation(
                    "8.11.3.2",
                    format!("the /OC entry on {what} could not be resolved: {why}"),
                    "drew it; a group that is off would have hidden it",
                ));
                false
            }
        }
    }
}
