//! Optional content (8.11): which groups a document turns off, and what that hides.
//!
//! **The engine drew hidden layers.** `BDC` popped its property list and discarded it —
//! the comment read "Skeleton: just pop for now" — so content inside an `/OC`
//! marked-content section was painted whatever its group's state. `/OCProperties` gained
//! a reader in Phase K and nothing consulted it, and `Operation::UpdateLayers` writes an
//! `/OFF` array, so the engine created layers it then ignored. That is a wrong answer
//! rather than a missing feature: a non-printing underlay, a "draft" stamp or the other
//! language of a bilingual page all appear on a page that should not carry them.
//!
//! **What a second renderer agrees to.** Thirteen probes were put to PDFKit, each a page
//! black in one quadrant and hidden by a different construction, compared by the method
//! `scripts/test/crosscheck_image.sh` uses. PDFKit honours two of them — a group listed
//! in the default configuration's `/OFF`, and a `/BaseState /OFF` — and paints the other
//! eleven, including an `/OC` on an XObject, every OCMD `/P` policy, a `/VE` expression
//! and a `/Usage` applied through `/AS`. So one construction here has an independent
//! oracle and the rest are held to the clause's own text. Where the two disagree the
//! standard wins (`AGENTS.md`, Hierarchy of Truth); the measurement and what it cost are
//! in [ADR-0021](../../../docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md).
//!
//! **Nothing is hidden on a doubt.** Every reading that does not end in "this document
//! turned this group off" leaves the content visible and records a `Decision` saying so.
//! Painting a layer that should be hidden shows something that was there; hiding one on
//! a guess removes something that was, and there is no way for the reader of the output
//! to tell that it happened. [`Membership::Unreadable`] is that path, and it is the
//! answer for an `/OC` naming a resource the page does not carry, an OCG written
//! directly rather than as an indirect object, and a `/VE` this evaluator cannot finish.

use crate::arena::PdfArena;
use crate::document::Document;
use crate::document::entries::{self, OptionalContent};
use crate::error::PdfResult;
use crate::handle::Handle;
use crate::interpretation::Decision;
use crate::object::{FromPdfObject, Object, PdfName};
use fepdf_macros::FromPdfObject;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// How many nodes of a `/VE` expression are visited before it is assumed to be looping.
///
/// A visibility expression is an array of arrays, and an element may be an indirect
/// reference — so `6 0 obj [/Not 6 0 R] endobj` is a legal-looking file that no
/// traversal terminates on. The number is the same order as [`entries`]'s tree depth
/// limit and for the same reason: large enough that no expression a producer writes
/// reaches it, small enough that a malformed one stops.
const MAX_EXPRESSION_NODES: usize = 512;

/// An `ON`/`OFF` name — `/BaseState` (Table 100) and each of Table 103's usage states.
///
/// `Other` keeps a name the clause does not define, rather than defaulting it away:
/// `DeclaredVersion` and `PageMode` settled the same question the same way. A file that
/// writes `/BaseState /Unchanged` said something, and a reader that reported `On` would
/// be reporting its own default as the document's word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnOff {
    /// `/ON`.
    On,
    /// `/OFF`.
    Off,
    /// A name that is neither, kept verbatim.
    Other(String),
}

impl FromPdfObject for OnOff {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let name = PdfName::from_pdf_object(obj, arena)?;
        Ok(match name.as_str() {
            "ON" => Self::On,
            "OFF" => Self::Off,
            other => Self::Other(other.to_string()),
        })
    }
}

/// `/Intent` (8.11.2.2): what a group or a configuration is *for*.
///
/// One name or an array of them, and both forms occur — which is why this is a type
/// rather than an `Option<PdfName>`. `/View` is the default on both sides, and a group
/// whose intent does not meet the configuration's "shall not be considered": its state
/// is not consulted and its content is drawn. `/Design` is the intent that matters in
/// practice — a CAD layer set that a viewer is not meant to act on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intents(pub Vec<String>);

impl Intents {
    /// The default both a group and a configuration are read as when they say nothing.
    #[must_use]
    pub fn viewing() -> Self {
        Self(vec!["View".to_string()])
    }

    /// Whether these intents and `other` have a name in common.
    #[must_use]
    pub fn meets(&self, other: &Self) -> bool {
        self.0.iter().any(|name| other.0.contains(name))
    }
}

impl FromPdfObject for Intents {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        match obj.resolve(arena) {
            Object::Array(handle) => Ok(Self(
                arena
                    .get_array(handle)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|item| item.resolve(arena).as_name())
                    .filter_map(|name| arena.get_name(name))
                    .map(|name| name.as_str().to_string())
                    .collect(),
            )),
            other => PdfName::from_pdf_object(other, arena)
                .map(|name| Self(vec![name.as_str().to_string()])),
        }
    }
}

/// `/P` in a membership dictionary (Table 97): how a set of group states becomes one
/// answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisibilityPolicy {
    /// `/AnyOn` — visible when any of the groups is on. The default.
    AnyOn,
    /// `/AllOn` — visible only when every group is on.
    AllOn,
    /// `/AnyOff` — visible when any of the groups is off.
    AnyOff,
    /// `/AllOff` — visible only when every group is off.
    AllOff,
    /// A name Table 97 does not define, kept verbatim.
    Other(String),
}

impl FromPdfObject for VisibilityPolicy {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let name = PdfName::from_pdf_object(obj, arena)?;
        Ok(match name.as_str() {
            "AnyOn" => Self::AnyOn,
            "AllOn" => Self::AllOn,
            "AnyOff" => Self::AnyOff,
            "AllOff" => Self::AllOff,
            other => Self::Other(other.to_string()),
        })
    }
}

/// `/Usage` (8.11.4.4, Table 102): the conditions under which a group should be used.
///
/// Three of the eight entries carry a state, and only those three can change what is
/// drawn — the rest describe the group for a user interface. A usage dictionary does
/// nothing on its own: it takes effect only where a configuration's `/AS` names both the
/// group and the category (8.11.4.5), which is why [`OptionalContentState`] reads this
/// through [`UsageApplication`] rather than from the group.
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "8.11.4.4")]
pub struct UsageDictionary {
    #[pdf_key("View")]
    /// `/View`: whether the group should be on when the document is viewed (Table 103).
    pub view: Option<ViewUsage>,
    #[pdf_key("Print")]
    /// `/Print`: whether it should be on when printed. Read, and not applied — nothing
    /// in this engine prints, so applying it would change what a *viewer* sees on the
    /// strength of an event that never happens.
    pub print: Option<PrintUsage>,
    #[pdf_key("Export")]
    /// `/Export`: whether it should be on when the document is converted to another
    /// format. Read, not applied, for the same reason as `/Print`.
    pub export: Option<ExportUsage>,
    #[pdf_key("CreatorInfo")]
    /// `/CreatorInfo`: what the producing application called this group. Reachable, not
    /// modelled — the clause fixes no keys for it beyond `/Creator` and `/Subtype`.
    pub creator_info: Option<Object>,
    #[pdf_key("Language")]
    /// `/Language`: the language this group's content is in, which is how a bilingual
    /// page is built. Reachable, not modelled.
    pub language: Option<Object>,
    #[pdf_key("Zoom")]
    /// `/Zoom`: the magnification range the group is meant to be visible over.
    /// Reachable, not modelled — this engine renders at a scale a caller chose, not at
    /// a magnification the document can reason about.
    pub zoom: Option<Object>,
    #[pdf_key("User")]
    /// `/User`: the individuals or roles the group is for. Reachable, not modelled.
    pub user: Option<Object>,
    #[pdf_key("PageElement")]
    /// `/PageElement`: the pagination artefact this group holds — a header, a footer, a
    /// watermark. Reachable, not modelled.
    pub page_element: Option<Object>,
}

/// `/View` in a usage dictionary (Table 103).
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "8.11.4.4")]
pub struct ViewUsage {
    #[pdf_key("ViewState")]
    /// `/ViewState`: the state the group takes when the `/View` usage is applied.
    pub state: Option<OnOff>,
}

/// `/Print` in a usage dictionary (Table 103).
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "8.11.4.4")]
pub struct PrintUsage {
    #[pdf_key("PrintState")]
    /// `/PrintState`: the state the group takes when the `/Print` usage is applied.
    pub state: Option<OnOff>,
    #[pdf_key("Subtype")]
    /// `/Subtype`: what kind of content this is to a printer — `/Trapping`, `/PrintersMarks`.
    pub subtype: Option<PdfName>,
}

/// `/Export` in a usage dictionary (Table 103).
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "8.11.4.4")]
pub struct ExportUsage {
    #[pdf_key("ExportState")]
    /// `/ExportState`: the state the group takes when the `/Export` usage is applied.
    pub state: Option<OnOff>,
}

/// One entry of a configuration's `/AS` (8.11.4.5, Table 101).
///
/// The bridge between a group's `/Usage` and its state: without an entry here naming
/// both the group and the category, a `/Usage << /View << /ViewState /OFF >> >>` changes
/// nothing at all.
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "8.11.4.5")]
pub struct UsageApplication {
    #[pdf_key("Event")]
    /// `/Event`: which of `/View`, `/Print` and `/Export` this application answers to.
    pub event: Option<PdfName>,
    #[pdf_key("OCGs")]
    /// `/OCGs`: the groups whose state this application sets.
    pub groups: Option<Handle<Vec<Object>>>,
    #[pdf_key("Category")]
    /// `/Category`: which entries of each group's `/Usage` are consulted.
    pub category: Option<Handle<Vec<Object>>>,
}

/// A configuration's `/AS` array, read.
///
/// A type of its own rather than an array handle, because this engine *acts* on it: the
/// `/View` event runs while a page is being drawn, and a field typed `Object` would be
/// the "reachable, contents opaque" that ADR-0017 was written about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageApplications(pub Vec<UsageApplication>);

impl FromPdfObject for UsageApplications {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let handle = Handle::<Vec<Object>>::from_pdf_object(obj, arena)?;
        // One malformed application does not discard the others, for the reason
        // `entries::read_array` gives: refusing the entry would lose every application
        // that reads, and report the configuration as carrying none.
        Ok(Self(
            arena
                .get_array(handle)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|item| UsageApplication::from_pdf_object(item, arena).ok())
                .collect(),
        ))
    }
}

/// One optional content group (8.11.2, Table 96).
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "8.11.2")]
pub struct OptionalContentGroup {
    #[pdf_key("Type")]
    /// `/Type`, `OCG` when written.
    pub kind: Option<PdfName>,
    #[pdf_key("Name")]
    /// `/Name`: what a viewer calls this layer in its list. Required by Table 96, and
    /// `Option` anyway — a group that omits it is still a group whose state decides what
    /// is drawn, and refusing it here would draw the layer.
    pub name: Option<String>,
    #[pdf_key("Intent")]
    /// `/Intent`: what the group is for, defaulting to `/View` (8.11.2.2).
    pub intent: Option<Intents>,
    #[pdf_key("Usage")]
    /// `/Usage`: the conditions under which the group should be on, applied only where a
    /// configuration's `/AS` asks for it.
    pub usage: Option<UsageDictionary>,
}

/// A configuration (8.11.4.4, Table 100): which groups are on when it is applied.
///
/// The document's `/D` is the one this engine uses. `/Configs` holds alternatives a
/// viewer may *offer*, and offering them is a user interface this engine does not have.
#[derive(Debug, Clone, FromPdfObject, Serialize, Deserialize)]
#[pdf_dict(clause = "8.11.4.4")]
pub struct OptionalContentConfiguration {
    #[pdf_key("Name")]
    /// `/Name`: what a viewer calls this configuration.
    pub name: Option<String>,
    #[pdf_key("Creator")]
    /// `/Creator`: the application that wrote it.
    pub creator: Option<String>,
    #[pdf_key("BaseState")]
    /// `/BaseState`: the state every group in `/OCProperties`'s `/OCGs` starts from,
    /// `/ON` when absent.
    pub base_state: Option<OnOff>,
    #[pdf_key("ON")]
    /// `/ON`: groups this configuration turns on, over the base state.
    pub on: Option<Handle<Vec<Object>>>,
    #[pdf_key("OFF")]
    /// `/OFF`: groups this configuration turns off, over the base state.
    pub off: Option<Handle<Vec<Object>>>,
    #[pdf_key("Intent")]
    /// `/Intent`: what this configuration is for, defaulting to `/View`. A group whose
    /// own intent does not meet it is not considered, and its content is drawn.
    pub intent: Option<Intents>,
    #[pdf_key("AS")]
    /// `/AS`: usage applications, which set group states from their `/Usage` (8.11.4.5).
    pub usage_applications: Option<UsageApplications>,
    #[pdf_key("Order")]
    /// `/Order`: the order and nesting a viewer should present the groups in. Named, not
    /// modelled: it decides a panel's shape and never what is drawn.
    pub order: Option<Handle<Vec<Object>>>,
    #[pdf_key("ListMode")]
    /// `/ListMode`: which groups that panel lists. A user interface concern.
    pub list_mode: Option<PdfName>,
    #[pdf_key("RBGroups")]
    /// `/RBGroups`: sets within which turning one group on turns the others off. Named,
    /// not applied — it constrains what a *user* may do next, not the initial state, and
    /// this engine has no toggle to constrain.
    pub rb_groups: Option<Handle<Vec<Object>>>,
    #[pdf_key("Locked")]
    /// `/Locked`: groups a user may not toggle. Named, not applied, for the same reason.
    pub locked: Option<Handle<Vec<Object>>>,
}

/// What the document says about one `/OC` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Membership {
    /// Nothing turns this content off. The default in every doubtful case.
    Visible,
    /// The default configuration turns it off, and it is not drawn.
    Hidden,
    /// The `/OC` entry could not be read as 8.11 describes one. **The content is
    /// drawn**, and the caller records why — see this module's opening note.
    Unreadable(String),
}

/// Which groups the default configuration turns off, resolved once per document.
///
/// Built from [`OptionalContent`], the Phase K reader for `/OCProperties` — so the
/// catalogue entry is what decides this rather than a second walk of the raw dictionary.
#[derive(Debug, Clone, Default)]
pub struct OptionalContentState {
    /// The groups that are off, by the handle a `/OC` entry names them with. An object's
    /// handle is its object number in this arena, so the handle *is* the identity.
    off: BTreeSet<Handle<Object>>,
}

impl OptionalContentState {
    /// Reads the default configuration, recording anything that stopped it.
    ///
    /// Never fails: a document whose `/OCProperties` will not read hides nothing, which
    /// is the same answer as a document that carries none. The difference between the two
    /// is in the decision log rather than in the return type, because there is nothing a
    /// caller could usefully do differently.
    #[must_use]
    pub fn read(doc: &Document) -> Self {
        let arena = doc.arena();
        let Some(root) = doc.catalog_handle().and_then(|handle| arena.get_object(handle)) else {
            return Self::default();
        };
        let properties = match entries::entry::<OptionalContent>(arena, &root, "OCProperties") {
            Ok(Some(properties)) => properties,
            Ok(None) => return Self::default(),
            Err(error) => {
                doc.record(Decision::violation(
                    "8.11.4.3",
                    format!("/OCProperties would not read: {error}"),
                    "drew every optional-content section; no layer was hidden",
                ));
                return Self::default();
            }
        };
        let Some(configuration) = properties.default_configuration.as_ref() else {
            // 8.11.4.3 makes `/D` required. Without it there is no statement about any
            // group's state, and inventing one would hide content on this engine's word.
            doc.record(Decision::violation(
                "8.11.4.3",
                "/OCProperties carries no /D, which 8.11.4.3 requires",
                "drew every optional-content section; no layer was hidden",
            ));
            return Self::default();
        };
        Self { off: off_groups(doc, &properties, configuration) }
    }

    /// Whether a group is on, by the handle that names it.
    ///
    /// Private: the question a caller has is about a piece of *content*, which is
    /// [`OptionalContentState::membership`]. A layer panel would want this one, and there
    /// is no layer panel.
    fn is_on(&self, group: Handle<Object>) -> bool {
        !self.off.contains(&group)
    }

    /// What the document says about content marked with `oc` — a `/OC` entry on an
    /// XObject, or the property list of a `/OC BDC`.
    #[must_use]
    pub fn membership(&self, arena: &PdfArena, oc: &Object) -> Membership {
        let Some(handle) = oc.as_reference() else {
            // 8.11.2 requires a group to be an indirect object, and 8.11.3.1's property
            // list is a reference into `/Properties`. A dictionary written in place has
            // no identity to match against `/OFF`, so nothing here can turn it off.
            return Membership::Unreadable(
                "the /OC entry is written in place, so it names no group (8.11.2)".to_string(),
            );
        };
        let Some(dict) = arena.get_object(handle).and_then(|obj| dict_of(arena, &obj)) else {
            return Membership::Unreadable(format!(
                "the /OC entry points at object {}, which is not a dictionary",
                handle.index()
            ));
        };
        if name_at(arena, &dict, "Type").is_some_and(|name| name.as_str() == "OCMD") {
            return self.membership_of_ocmd(arena, &dict);
        }
        if self.is_on(handle) { Membership::Visible } else { Membership::Hidden }
    }

    /// A membership dictionary (8.11.2.3, Table 97).
    fn membership_of_ocmd(&self, arena: &PdfArena, dict: &Dict) -> Membership {
        // `/VE` takes precedence: where it is present, Table 97 says `/OCGs` and `/P`
        // shall be ignored. An expression this evaluator cannot finish therefore cannot
        // fall back on them — that would answer a question the document did not ask.
        if let Some(expression) = dict.get(&arena.name("VE")) {
            return match self.evaluate(arena, expression) {
                Some(true) => Membership::Visible,
                Some(false) => Membership::Hidden,
                None => Membership::Unreadable(
                    "the /VE visibility expression could not be evaluated (8.11.2.3)".to_string(),
                ),
            };
        }
        let groups = referenced_groups(arena, dict.get(&arena.name("OCGs")));
        if groups.is_empty() {
            // Table 97 allows `/OCGs` to be absent, and says nothing about what an empty
            // set means under `/AllOn`. Visible, per this module's opening note.
            return Membership::Visible;
        }
        let policy = dict
            .get(&arena.name("P"))
            .and_then(|obj| VisibilityPolicy::from_pdf_object(obj.clone(), arena).ok())
            .unwrap_or(VisibilityPolicy::AnyOn);
        let mut on = groups.iter().map(|handle| self.is_on(*handle));
        let visible = match policy {
            VisibilityPolicy::AnyOn => on.any(|state| state),
            VisibilityPolicy::AllOn => on.all(|state| state),
            VisibilityPolicy::AnyOff => on.any(|state| !state),
            VisibilityPolicy::AllOff => on.all(|state| !state),
            // A policy Table 97 does not define is not a licence to invent one.
            VisibilityPolicy::Other(ref name) => {
                return Membership::Unreadable(format!(
                    "/P is /{name}, which Table 97 does not define"
                ));
            }
        };
        if visible { Membership::Visible } else { Membership::Hidden }
    }

    /// Evaluates a `/VE` visibility expression (8.11.2.3), iteratively.
    ///
    /// An explicit stack rather than recursion (RR-15 Rule 6), and budgeted: an element
    /// of the array may be an indirect reference, so an array that contains itself is a
    /// file a naive traversal never returns from.
    fn evaluate(&self, arena: &PdfArena, expression: &Object) -> Option<bool> {
        let mut work = vec![Task::Eval(expression.clone())];
        let mut values: Vec<bool> = Vec::new();
        let mut budget = MAX_EXPRESSION_NODES;
        while let Some(task) = work.pop() {
            budget = budget.checked_sub(1)?;
            match task {
                Task::Eval(object) => self.expand(arena, &object, &mut work, &mut values)?,
                Task::Apply(operator, count) => {
                    let at = values.len().checked_sub(count)?;
                    let operands: Vec<bool> = values.split_off(at);
                    values.push(match operator {
                        Operator::Not => !*operands.first()?,
                        Operator::And => operands.iter().all(|value| *value),
                        Operator::Or => operands.iter().any(|value| *value),
                    });
                }
            }
        }
        values.pop().filter(|_| values.is_empty())
    }

    /// One node of a `/VE`: either a group whose state is the answer, or an operator and
    /// its operands.
    fn expand(
        &self,
        arena: &PdfArena,
        object: &Object,
        work: &mut Vec<Task>,
        values: &mut Vec<bool>,
    ) -> Option<()> {
        // A reference that resolves to a dictionary is a group; one that resolves to an
        // array is a sub-expression written as its own object, which is legal and which
        // the loop guard above exists for.
        if let Some(handle) = object.as_reference()
            && matches!(arena.get_object(handle), Some(Object::Dictionary(_)))
        {
            values.push(self.is_on(handle));
            return Some(());
        }
        let Object::Array(array) = object.resolve(arena) else { return None };
        let items = arena.get_array(array)?;
        let (head, operands) = items.split_first()?;
        let operator = match arena.get_name(head.resolve(arena).as_name()?)?.as_str() {
            "Not" if operands.len() == 1 => Operator::Not,
            "And" => Operator::And,
            "Or" => Operator::Or,
            _ => return None,
        };
        work.push(Task::Apply(operator, operands.len()));
        for operand in operands {
            work.push(Task::Eval(operand.clone()));
        }
        Some(())
    }
}

/// One step of the `/VE` evaluator's explicit stack.
enum Task {
    /// Turn this object into either a value or an operator and its operands.
    Eval(Object),
    /// Combine the top `n` values.
    Apply(Operator, usize),
}

/// The three operators 8.11.2.3 defines for a visibility expression.
#[derive(Clone, Copy)]
enum Operator {
    /// `/Not`, over exactly one operand.
    Not,
    /// `/And`.
    And,
    /// `/Or`.
    Or,
}

/// The groups the default configuration leaves off.
///
/// `/BaseState` first, then `/ON` and `/OFF` over it, then the `/View` usage where `/AS`
/// asks for it, and finally the intent filter — which can only ever *remove* a group
/// from the set, because a group that is not considered is drawn.
fn off_groups(
    doc: &Document,
    properties: &OptionalContent,
    configuration: &OptionalContentConfiguration,
) -> BTreeSet<Handle<Object>> {
    let arena = doc.arena();
    let declared = referenced_groups(arena, properties.groups.map(Object::Array).as_ref());
    let mut off: BTreeSet<Handle<Object>> = match configuration.base_state {
        // 8.11.4.4: `/OFF` means every group in `/OCProperties`'s `/OCGs`. A group the
        // document forgot to list there is not reachable from here and stays on.
        Some(OnOff::Off) => declared.iter().copied().collect(),
        Some(OnOff::On) | None => BTreeSet::new(),
        Some(OnOff::Other(ref name)) => {
            doc.record(Decision::violation(
                "8.11.4.4",
                format!("/BaseState is /{name}, which Table 100 does not define"),
                "read it as /ON, the entry's default; no layer was hidden by it",
            ));
            BTreeSet::new()
        }
    };
    for group in referenced_groups(arena, configuration.on.map(Object::Array).as_ref()) {
        off.remove(&group);
    }
    off.extend(referenced_groups(arena, configuration.off.map(Object::Array).as_ref()));
    apply_view_usage(arena, configuration, &mut off);
    retain_considered(arena, configuration, &mut off);
    off
}

/// Applies the `/View` event of the configuration's `/AS` (8.11.4.5).
///
/// The event a renderer is: this engine draws for viewing, so `/View` is the one usage
/// application whose conditions are met. `/Print` and `/Export` are read
/// ([`UsageDictionary`]) and not applied, because nothing here prints or exports — and
/// applying `/Print` to a screen would hide the trapping layer of a drawing from someone
/// looking at it.
fn apply_view_usage(
    arena: &PdfArena,
    configuration: &OptionalContentConfiguration,
    off: &mut BTreeSet<Handle<Object>>,
) {
    let Some(UsageApplications(ref applications)) = configuration.usage_applications else {
        return;
    };
    for application in applications {
        let event = application.event.as_ref().map(PdfName::as_str);
        if event != Some("View") || !names_category(arena, application, "View") {
            continue;
        }
        for handle in referenced_groups(arena, application.groups.map(Object::Array).as_ref()) {
            let state = group_at(arena, handle).and_then(|group| group.usage?.view?.state);
            match state {
                Some(OnOff::Off) => {
                    off.insert(handle);
                }
                Some(OnOff::On) => {
                    off.remove(&handle);
                }
                // A group named by an application whose `/Usage` says nothing about
                // viewing keeps whatever `/ON` and `/OFF` gave it.
                Some(OnOff::Other(_)) | None => {}
            }
        }
    }
}

/// Whether a usage application's `/Category` names `category`.
///
/// Absent `/Category` is *not* "all categories": Table 101 requires the entry, and a
/// missing one leaves no statement about which part of `/Usage` to consult.
fn names_category(arena: &PdfArena, application: &UsageApplication, category: &str) -> bool {
    application
        .category
        .and_then(|handle| arena.get_array(handle))
        .unwrap_or_default()
        .iter()
        .filter_map(|item| item.resolve(arena).as_name())
        .filter_map(|name| arena.get_name(name))
        .any(|name| name.as_str() == category)
}

/// Drops from `off` every group whose `/Intent` does not meet the configuration's.
///
/// 8.11.2.2: such a group "shall not be considered", and content whose group is not
/// considered is drawn. A `/Design` layer set in a CAD drawing is the case — turned off
/// for an editor's benefit, and not something a viewer should act on.
fn retain_considered(
    arena: &PdfArena,
    configuration: &OptionalContentConfiguration,
    off: &mut BTreeSet<Handle<Object>>,
) {
    let wanted = configuration.intent.clone().unwrap_or_else(Intents::viewing);
    off.retain(|handle| {
        let intent = group_at(arena, *handle)
            .and_then(|group| group.intent)
            .unwrap_or_else(Intents::viewing);
        intent.meets(&wanted)
    });
}

/// The indirect objects an array names, in order, ignoring elements that are not
/// references — a group has to be one (8.11.2).
fn referenced_groups(arena: &PdfArena, array: Option<&Object>) -> Vec<Handle<Object>> {
    let resolved = array.map(|obj| obj.resolve(arena));
    let Some(Object::Array(handle)) = resolved else { return Vec::new() };
    arena.get_array(handle).unwrap_or_default().iter().filter_map(Object::as_reference).collect()
}

/// One group, read through the handle that names it (Table 96).
fn group_at(arena: &PdfArena, handle: Handle<Object>) -> Option<OptionalContentGroup> {
    OptionalContentGroup::from_pdf_object(Object::Reference(handle), arena).ok()
}

fn dict_of(arena: &PdfArena, object: &Object) -> Option<Dict> {
    arena.get_dict(object.resolve(arena).as_dict_handle()?)
}

fn name_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<PdfName> {
    arena.get_name(dict.get(&arena.name(key))?.resolve(arena).as_name()?)
}
