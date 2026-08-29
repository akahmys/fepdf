//! Marked content (14.6) and the one thing it decides about drawing: optional content
//! (8.11.3.1).
//!
//! This file used to read, in full, `let _props = self.stack.pop();` under the comment
//! "Skeleton: just pop for now" — so a `/OC /MC0 BDC` was a `BDC` like any other and its
//! group's state was never consulted. See [`fepdf_model::optional_content`] for what the
//! property list is worth and why nothing is hidden on a doubt.

use crate::RenderBackend;
use crate::interpreter::Interpreter;
use fepdf_model::interpretation::Decision;
use fepdf_model::optional_content::{Membership, OptionalContentState};
use fepdf_model::{Object, PdfName, PdfResult};

/// One open marked-content section.
///
/// The interpreter keeps a stack of these rather than a count of hidden ones, because
/// `EMC` has to know *which* kind it is closing: a `/Span` opened inside a hidden `/OC`
/// section closes with an `EMC` that must not bring the page back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct MarkedSection {
    /// An `/OC` section whose group is off. Marks are withheld until its `EMC`.
    pub(crate) hidden: bool,
    /// The section declared `/ActualText`, and told the backend so (14.9.4).
    ///
    /// Tracked per section rather than as a depth count because the two kinds nest
    /// freely: a `/Span` carrying `/ActualText` can open inside a hidden `/OC`, and each
    /// `EMC` has to undo exactly what its own `BDC` did.
    pub(crate) replaced: bool,
}

impl Interpreter<'_> {
    /// 14.6.2's *point* operators, which open nothing and so close nothing.
    ///
    /// **Only these two reach here.** `BMC`, `BDC` and `EMC` become
    /// [`Command::BeginMarkedContent`] and [`Command::EndMarkedContent`] in the parser and
    /// arrive through those arms; `MP` and `DP` are the pair it leaves as raw operators.
    /// This function had arms for all five, and the `BDC` one read no `/ActualText` — a
    /// second way into the same operator that would have silently dropped the text the
    /// other way had just been taught to read (14.9.4).
    ///
    /// [`Command::BeginMarkedContent`]: fepdf_model::object::sublimation::Command
    /// [`Command::EndMarkedContent`]: fepdf_model::object::sublimation::Command
    pub(crate) fn handle_point_marked_content(&mut self, op: &str) -> PdfResult<()> {
        // `DP` pops the same two operands as `BDC`; `MP` the tag alone.
        if op == "DP" {
            let _properties = self.stack.pop();
        }
        let _tag = self.pop_name()?;
        Ok(())
    }

    /// Opens a section, hiding what follows when the tag is `/OC` and the group is off,
    /// and announcing `/ActualText` when the section declares one.
    ///
    /// The text arrives already read rather than being taken from `properties`, because
    /// an inline property list does not survive the conversion to [`Object`] that the
    /// optional-content code needs — see `actual_text_of`, which reads it from the IR.
    pub(crate) fn begin_marked_content(
        &mut self,
        tag: &PdfName,
        properties: Option<&Object>,
        actual_text: Option<String>,
    ) {
        let replaced = if let Some(text) = actual_text {
            self.backend.begin_actual_text(&text);
            true
        } else {
            false
        };
        if tag.as_str() != "OC" {
            self.marked_sections.push(MarkedSection { hidden: false, replaced });
            return;
        }
        let hidden = match self.optional_content_membership(properties) {
            Membership::Hidden => {
                self.backend.hide();
                true
            }
            Membership::Visible => false,
            Membership::Unreadable(why) => {
                self.doc.record(Decision::violation(
                    "8.11.3.1",
                    format!("a /OC marked-content section could not be resolved: {why}"),
                    "drew the section; a group that is off would have hidden it",
                ));
                false
            }
        };
        self.marked_sections.push(MarkedSection { hidden, replaced });
    }

    /// Closes the innermost section.
    ///
    /// An `EMC` with nothing open is ignored rather than treated as an error: the file is
    /// wrong, and refusing the operator would abort the content stream and take the rest
    /// of the page's text with it — the failure ADR-0018 was written about.
    pub(crate) fn end_marked_content(&mut self) {
        let Some(section) = self.marked_sections.pop() else {
            return;
        };
        if section.hidden {
            self.backend.reveal();
        }
        if section.replaced {
            self.backend.end_actual_text();
        }
    }

    /// The `/ActualText` a section declares (14.9.4), from either shape of property list.
    ///
    /// Two shapes, because 14.6.2 allows both: written in place, which is what every one
    /// of the corpus's 6,080 spans does, or a name into the page's `/Properties`, which
    /// none of them does and which costs one lookup to honour anyway.
    ///
    /// An empty `/ActualText` is a section that stands for *no* text, which is a real
    /// thing to say — a decorative glyph, a repeated hyphen at a line break — so it is
    /// kept as `Some("")` and suppresses the glyphs, rather than being read as absent.
    pub(crate) fn actual_text_of(
        &self,
        ir: Option<&fepdf_model::object::sublimation::IrObject>,
        operand: Option<&Object>,
    ) -> Option<String> {
        use fepdf_model::object::sublimation::IrObject;
        if let Some(IrObject::Dictionary(entries)) = ir {
            return match entries.get("ActualText") {
                Some(IrObject::String(b) | IrObject::Hex(b)) => {
                    Some(fepdf_model::refine::text::recover_string(b))
                }
                _ => None,
            };
        }
        // A named property list. The name reaches here already interned, so the lookup is
        // the same one `/OC` does.
        let Some(Object::Name(handle)) = operand else {
            return None;
        };
        let arena = self.doc.arena();
        let name = arena.get_name(*handle)?;
        let key = arena.intern_name(PdfName::new("Properties"));
        let resource = self.find_resource(&key, &name).ok()?;
        let arena = self.doc.arena();
        let dict = arena.get_dict(resource.as_dict_handle()?)?;
        match dict.get(&arena.name("ActualText")).map(|v| v.resolve(arena)) {
            Some(Object::String(b) | Object::Hex(b)) => {
                Some(fepdf_model::refine::text::recover_string(&b))
            }
            Some(Object::Text(t)) => Some(t),
            _ => None,
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
