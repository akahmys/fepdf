//! What a document *does* — the actions it can run, and what has to happen first (12.6).
//!
//! **A different question from "what interactive features are here".**
//! [`crate::interactive`] counts actions by `/S`, which says a file carries one
//! `/JavaScript` and nothing about what the script is or when it runs.
//! [ADR-0019](../../docs/adr/0019-semantic-understanding-is-measured-against-what-a-corpus-presents.md)
//! kept actions out of the coverage index because *"reads an action"* has no settled
//! meaning — a `/GoTo`'s destination resolves through the name tree while a `/URI`'s
//! target is never looked at. **"What can this document do, and does the reader have to
//! do anything first"** is settled, and it is the question a security screen asks.
//!
//! So this reports two things per action and nothing else: the [`Capability`] it grants,
//! and the [`Trigger`] that fires it. `app.alert("Hello World!")` behind a link is a
//! different fact from the same script in `/OpenAction`, and both are different from a
//! `/Launch` naming `TextPad.exe` — all three of which are files of the external corpus.
//!
//! **Executing is a subset this processor has not chosen.** 12.6.4.17 says a processor
//! *shall* execute the script on invocation, and 6.3.2.1 is what makes reading it without
//! running it conforming: each processor chooses which subsets of PDF functionality to
//! support and complies for the ones it chose — PDF 2.0 deliberately has no "conforming
//! reader". A document that needs the subset can say so, in `/Requirements` with
//! `EnableJavaScripts` (12.11), and saying so is the honest interface between the two.
//!
//! **Every place an action can hang is walked**, because a screen that missed one would
//! be worse than none: `/OpenAction`, the catalogue's `/AA`, the document-level
//! `/Names /JavaScript` tree, each page's `/AA`, each annotation's `/A` and `/AA`, each
//! form field's `/AA`, and the `/Next` chain hanging off any of them. The name tree is
//! the one that matters most and is the easiest to miss: nothing *points* at it, and its
//! scripts run when the file opens.

use crate::arena::PdfArena;
use crate::document::Document;
use crate::document::entries::{DocumentRequirements, Requirement};
use crate::error::PdfResult;
use crate::handle::Handle;
use crate::object::{Object, PdfName};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// How far a `/Next` chain, a page tree or a field tree is followed before it is assumed
/// to be looping. The same bound the other walks in this crate use, for the same reason.
const MAX_DEPTH: usize = 64;

/// What an action lets a document do.
///
/// The classification is by *consequence*, not by name, because that is the part a
/// caller can act on. Two `/S` values this engine has never seen would still land in the
/// right group if they did the same thing — and one it does not recognise lands in
/// [`Capability::Undefined`] rather than quietly in the harmless group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Capability {
    /// Runs a script: `/JavaScript` (12.6.4.17).
    ///
    /// The clause is titled **ECMAScript actions**: PDF 2.0 renamed the language
    /// throughout and kept `JavaScript` as the keyword for compatibility. The language
    /// itself is normative by reference to ISO/DIS 21757-1 and is not defined in
    /// ISO 32000-2 at all.
    RunsCode,
    /// Starts another program, or opens another file with one: `/Launch` (12.6.4.6).
    LaunchesAnother,
    /// Reaches outside this document — the network, another file, or a server:
    /// `/URI`, `/SubmitForm`, `/ImportData`, `/GoToR`, `/GoToE`.
    ReachesOutside,
    /// Plays or renders media: `/Sound`, `/Movie`, `/Rendition`, `/RichMediaExecute`.
    /// Clause 13.4 deprecates most of it, which does not stop a file carrying it.
    PlaysMedia,
    /// Moves or changes something within this document and nothing else: `/GoTo`,
    /// `/Named`, `/SetOCGState`, `/Hide`, `/ResetForm`, `/Thread`, `/Trans`.
    StaysInside,
    /// An `/S` clause 12.6.4 does not define, or an action carrying none. **Not** folded
    /// into `StaysInside`: an unrecognised action is unknown, and reporting an unknown as
    /// harmless is the failure this whole module exists to avoid.
    Undefined,
}

impl Capability {
    /// The consequence of an action type (Table 196's `/S` values).
    #[must_use]
    pub fn of(kind: &str) -> Self {
        match kind {
            "JavaScript" => Self::RunsCode,
            "Launch" => Self::LaunchesAnother,
            "URI" | "SubmitForm" | "ImportData" | "GoToR" | "GoToE" => Self::ReachesOutside,
            "Sound" | "Movie" | "Rendition" | "RichMediaExecute" => Self::PlaysMedia,
            "GoTo" | "Named" | "SetOCGState" | "Hide" | "ResetForm" | "Thread" | "Trans" => {
                Self::StaysInside
            }
            _ => Self::Undefined,
        }
    }

    /// What to call it in a report.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::RunsCode => "runs code",
            Self::LaunchesAnother => "launches another program",
            Self::ReachesOutside => "reaches outside the document",
            Self::PlaysMedia => "plays media",
            Self::StaysInside => "stays inside the document",
            Self::Undefined => "not a defined action type",
        }
    }
}

/// What has to happen before an action fires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    /// `/OpenAction` — the document is opened and this runs (12.3.2).
    DocumentOpened,
    /// A script in the `/Names /JavaScript` tree, which runs when the document opens
    /// (12.6.4.17). Carries the name it is filed under.
    DocumentScript(String),
    /// The catalogue's `/AA`: before the document is closed, saved or printed.
    DocumentEvent(String),
    /// A page's `/AA`: the page is opened or closed.
    PageEvent {
        /// Zero-based page index.
        page: usize,
        /// The `/AA` key — `/O` when the page opens, `/C` when it closes.
        event: String,
    },
    /// An annotation's `/A`: the reader activated it.
    AnnotationActivated {
        /// Zero-based page index.
        page: usize,
        /// The annotation's `/Subtype`.
        subtype: String,
    },
    /// An annotation's `/AA`: a pointer entering it, a field losing focus.
    AnnotationEvent {
        /// Zero-based page index.
        page: usize,
        /// The `/AA` key — `/E`, `/X`, `/Fo`, `/K`, `/V`, `/C`.
        event: String,
    },
    /// A form field's `/AA`, on a field that is not itself an annotation.
    FieldEvent {
        /// The field's `/T`, when it has one.
        field: Option<String>,
        /// The `/AA` key.
        event: String,
    },
    /// Reached through another action's `/Next` (12.6.1).
    Chained,
}

impl Trigger {
    /// Whether this fires with **no** interaction from the reader.
    ///
    /// The line a screen is really drawn on: a script in `/OpenAction` or the name tree
    /// runs at the moment the file is opened, and one behind a link does not.
    /// `Chained` is *not* here even though it may follow one that is — what fires it is
    /// the action it hangs off, and that one is reported in its own right.
    #[must_use]
    pub const fn without_interaction(&self) -> bool {
        matches!(self, Self::DocumentOpened | Self::DocumentScript(_))
    }
}

/// What an action says, for the kinds that say something.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Says {
    /// `/JS`, from a literal string or a stream — the script itself.
    Script(String),
    /// A file or program this names: a `/Launch`'s `/F`, or the `/Win` dictionary's,
    /// which is where `TextPad.exe` is written in the file that made this worth reading.
    File(String),
    /// `/URI`, or a `/SubmitForm`'s target.
    Url(String),
    /// A `/Named` action's `/N` — `NextPage`, `Print`, `SaveAs`.
    Name(String),
}

/// One action, and what it takes to fire it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReachableAction {
    /// What has to happen first.
    pub trigger: Trigger,
    /// `/S` verbatim, or `(none)` where the action carries none.
    pub kind: String,
    /// What that lets the document do.
    pub capability: Capability,
    /// What the action says, where the kind is one that says something.
    pub says: Option<Says>,
}

/// The requirement names (12.11, Table 275) this processor is known **not** to satisfy.
///
/// A short list on purpose. Claiming to satisfy a requirement is a claim that goes stale;
/// claiming *not* to satisfy one is a decision this project has taken and written down.
///
/// **`EnableJavaScripts` changed meaning on 2026-08-22 and the entry stayed.** It used to
/// say the subset was *not chosen* — 6.3.2.1 lets a processor decline, so a document
/// asking for it was asking for something it would not get. ADR-0026 took the subset, and
/// the reason was not that a corpus asked: `SetFormFieldValue` records a `Violation` of
/// 12.6.3 on every form that declares a calculation order, so form editing was undertaken
/// and cannot be finished without it. The entry therefore now reports **chosen and not
/// yet met** (ROADMAP Phase R), which is a defect rather than a refusal.
///
/// What the caller is told is the same either way, and that is the point of reporting the
/// requirement rather than the reasoning: a document asking for this still will not get
/// it today. The entry leaves this list when the scripts run, not when the decision was
/// taken.
pub const NOT_SATISFIED: &[&str] = &["EnableJavaScripts"];

/// Everything a document can do, and what has to happen first.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionReport {
    /// Every action reachable from anywhere, in the order the walk finds them.
    pub actions: Vec<ReachableAction>,
    /// What the document declares a processor must support to handle it (12.11).
    pub requirements: Vec<Requirement>,
    /// Objects in an action position that would not read as an action dictionary.
    /// Counted rather than skipped: a screen that silently dropped one would report a
    /// document as doing less than it does.
    pub unreadable: usize,
}

impl ActionReport {
    /// Walks every place clause 12.6 lets an action hang.
    ///
    /// # Errors
    /// Fails only when the catalogue cannot be reached; a page or annotation that will
    /// not read contributes nothing rather than failing the report.
    pub fn of(doc: &Document) -> PdfResult<Self> {
        let arena = doc.arena();
        let mut out = Self::default();
        let Some(catalog) = doc.catalog_handle().and_then(|h| arena.get_object(h)) else {
            return Ok(out);
        };
        let Some(catalog) = dict_of(arena, &catalog) else { return Ok(out) };

        out.requirements = crate::document::entries::entry::<DocumentRequirements>(
            arena,
            &Object::Dictionary(arena.alloc_dict(catalog.clone())),
            "Requirements",
        )
        .ok()
        .flatten()
        .map(|declared| declared.required)
        .unwrap_or_default();

        out.take(arena, catalog.get(&arena.name("OpenAction")), &Trigger::DocumentOpened);
        out.take_additional_actions(arena, &catalog, &|event| Trigger::DocumentEvent(event));
        out.take_document_scripts(arena, &catalog);
        out.take_pages(doc, arena);
        out.take_form_fields(arena, &catalog);
        Ok(out)
    }

    /// The requirements this document declares that this processor does not satisfy.
    ///
    /// The honest half of a subset this processor does not deliver: 12.11 is how a
    /// document says it needs one, and where that meets a name in `NOT_SATISFIED`,
    /// somebody has to be told. The reason a name is on that list — declined under
    /// 6.3.2.1, or chosen and not yet built — does not change what the document does not
    /// get, so it is not encoded here.
    #[must_use]
    pub fn unmet_requirements(&self) -> Vec<&Requirement> {
        self.requirements
            .iter()
            .filter(|r| NOT_SATISFIED.contains(&r.requirement.as_str()))
            .collect()
    }

    /// The actions that fire with no interaction at all.
    #[must_use]
    pub fn without_interaction(&self) -> Vec<&ReachableAction> {
        self.actions.iter().filter(|a| a.trigger.without_interaction()).collect()
    }

    /// Each capability the document has, with how many actions grant it, worst first.
    #[must_use]
    pub fn capabilities(&self) -> Vec<(Capability, usize)> {
        let mut tally: BTreeMap<Capability, usize> = BTreeMap::new();
        for action in &self.actions {
            *tally.entry(action.capability).or_default() += 1;
        }
        tally.into_iter().collect()
    }

    /// Reads one object in an action position, following its `/Next` chain.
    fn take(&mut self, arena: &PdfArena, entry: Option<&Object>, trigger: &Trigger) {
        let Some(entry) = entry else { return };
        let Some(action) = dict_of(arena, entry) else {
            // `/OpenAction` may be a destination array rather than an action (12.3.2),
            // which is not a defect and does nothing but move the view.
            if !matches!(entry.resolve(arena), Object::Array(_)) {
                self.unreadable += 1;
            }
            return;
        };
        self.push(arena, &action, trigger.clone());
        let mut next = action.get(&arena.name("Next")).cloned();
        for _ in 0..MAX_DEPTH {
            let Some(entry) = next.take() else { break };
            for chained in chained_actions(arena, &entry) {
                self.push(arena, &chained, Trigger::Chained);
                next = chained.get(&arena.name("Next")).cloned();
            }
        }
    }

    /// Records one action dictionary.
    fn push(&mut self, arena: &PdfArena, action: &Dict, trigger: Trigger) {
        let kind = text_name(arena, action, "S").unwrap_or_else(|| "(none)".to_string());
        let capability = Capability::of(&kind);
        self.actions.push(ReachableAction {
            says: says_of(arena, action, &kind),
            capability,
            kind,
            trigger,
        });
    }

    /// Every entry of a dictionary's `/AA` (12.6.3).
    fn take_additional_actions(
        &mut self,
        arena: &PdfArena,
        dict: &Dict,
        trigger: &dyn Fn(String) -> Trigger,
    ) {
        let Some(aa) = dict.get(&arena.name("AA")).and_then(|a| dict_of(arena, a)) else {
            return;
        };
        for (key, value) in &aa {
            let event = arena.get_name(*key).map_or_else(String::new, |n| n.as_str().to_string());
            self.take(arena, Some(value), &trigger(event));
        }
    }

    /// The `/Names /JavaScript` tree (12.6.4.17), whose scripts run when the file opens.
    ///
    /// Not an inference: the clause says all the actions in this tree shall be executed
    /// when the document is opened. It is the one place an action fires that nothing in
    /// the document points at.
    fn take_document_scripts(&mut self, arena: &PdfArena, catalog: &Dict) {
        let Some(names) = catalog.get(&arena.name("Names")).and_then(|n| dict_of(arena, n)) else {
            return;
        };
        let Some(root) = names.get(&arena.name("JavaScript")).and_then(|t| dict_of(arena, t))
        else {
            return;
        };
        for (name, value) in name_tree_entries(arena, &root) {
            self.take(arena, Some(&value), &Trigger::DocumentScript(name));
        }
    }

    /// Each page's `/AA`, and each of its annotations' `/A` and `/AA`.
    fn take_pages(&mut self, doc: &Document, arena: &PdfArena) {
        let count = doc.page_count().unwrap_or(0);
        for page in 0..count {
            let Some(dict) = doc
                .get_page_handle(page)
                .and_then(|h| arena.get_object(h))
                .as_ref()
                .and_then(|object| dict_of(arena, object))
            else {
                continue;
            };
            self.take_additional_actions(arena, &dict, &|event| Trigger::PageEvent { page, event });
            for annotation in array_of(arena, dict.get(&arena.name("Annots"))).unwrap_or_default() {
                let Some(annotation) = dict_of(arena, &annotation) else { continue };
                let subtype = text_name(arena, &annotation, "Subtype")
                    .unwrap_or_else(|| "(no /Subtype)".to_string());
                self.take(
                    arena,
                    annotation.get(&arena.name("A")),
                    &Trigger::AnnotationActivated { page, subtype },
                );
                self.take_additional_actions(arena, &annotation, &|event| {
                    Trigger::AnnotationEvent { page, event }
                });
            }
        }
    }

    /// Each form field's `/AA` (12.7.5.3) — where a business form keeps its scripts.
    ///
    /// A widget is both a field and an annotation and is therefore reached twice; the
    /// duplicate is left in deliberately, because the two triggers are different facts
    /// and collapsing them would lose the one that says *when*.
    fn take_form_fields(&mut self, arena: &PdfArena, catalog: &Dict) {
        let Some(form) = catalog.get(&arena.name("AcroForm")).and_then(|f| dict_of(arena, f))
        else {
            return;
        };
        let mut stack: Vec<(Object, usize)> = array_of(arena, form.get(&arena.name("Fields")))
            .unwrap_or_default()
            .into_iter()
            .map(|field| (field, 0))
            .collect();
        while let Some((entry, depth)) = stack.pop() {
            let Some(field) = dict_of(arena, &entry) else { continue };
            if depth < MAX_DEPTH {
                for kid in array_of(arena, field.get(&arena.name("Kids"))).unwrap_or_default() {
                    stack.push((kid, depth + 1));
                }
            }
            let name = text_string(arena, &field, "T");
            self.take_additional_actions(arena, &field, &|event| Trigger::FieldEvent {
                field: name.clone(),
                event,
            });
        }
    }
}

/// The action dictionaries a `/Next` entry names — one, or an array of them (12.6.1).
fn chained_actions(arena: &PdfArena, entry: &Object) -> Vec<Dict> {
    match entry.resolve(arena) {
        Object::Array(_) => array_of(arena, Some(entry))
            .unwrap_or_default()
            .iter()
            .filter_map(|item| dict_of(arena, item))
            .collect(),
        _ => dict_of(arena, entry).into_iter().collect(),
    }
}

/// What the action says, for the kinds that say something.
///
/// A `/Launch` is read from `/F` **and** from the platform dictionaries beside it: 12.6.4.6
/// deprecates `/Win`, `/Mac` and `/Unix`, and the one file of the corpus that carries a
/// `/Launch` writes `/Win << /F (TextPad.exe) … >>` and no `/F` of its own. Reading only
/// the undeprecated entry would have reported that document as launching nothing.
fn says_of(arena: &PdfArena, action: &Dict, kind: &str) -> Option<Says> {
    match kind {
        "JavaScript" => script_of(arena, action).map(Says::Script),
        "Launch" => launch_target(arena, action).map(Says::File),
        "URI" => text_string(arena, action, "URI").map(Says::Url),
        "SubmitForm" | "ImportData" | "GoToR" | "GoToE" => {
            file_of(arena, action, "F").map(Says::Url)
        }
        "Named" => text_name(arena, action, "N").map(Says::Name),
        _ => None,
    }
}

/// `/JS`, which 12.6.4.17 allows to be a string or a stream — a long script is a stream.
fn script_of(arena: &PdfArena, action: &Dict) -> Option<String> {
    let entry = action.get(&arena.name("JS"))?;
    match entry.resolve(arena) {
        Object::Stream(dict_handle, ref data) => {
            let dict = arena.get_dict(dict_handle)?;
            let raw = arena.get_stream_bytes(data).ok()?;
            let decoded = crate::filters::process_arena_filters(&raw, &dict, arena).ok()?;
            Some(crate::refine::text::recover_string(&decoded))
        }
        other => text_of(arena, &other),
    }
}

/// What a `/Launch` starts, from `/F` or from the platform dictionary that replaces it.
fn launch_target(arena: &PdfArena, action: &Dict) -> Option<String> {
    if let Some(named) = file_of(arena, action, "F") {
        return Some(named);
    }
    for platform in ["Win", "Mac", "Unix"] {
        let Some(dict) = action.get(&arena.name(platform)).and_then(|d| dict_of(arena, d)) else {
            continue;
        };
        if let Some(named) = text_string(arena, &dict, "F") {
            return Some(named);
        }
    }
    None
}

/// A `/F` that is either a string or a file specification (7.11.3).
fn file_of(arena: &PdfArena, dict: &Dict, key: &str) -> Option<String> {
    let entry = dict.get(&arena.name(key))?;
    if let Some(text) = text_of(arena, &entry.resolve(arena)) {
        return Some(text);
    }
    let spec = dict_of(arena, entry)?;
    text_string(arena, &spec, "UF").or_else(|| text_string(arena, &spec, "F"))
}

/// Every `(name, value)` a name tree holds (7.9.6), following `/Kids` with a bound.
fn name_tree_entries(arena: &PdfArena, root: &Dict) -> Vec<(String, Object)> {
    let mut out = Vec::new();
    let mut stack = vec![(root.clone(), 0_usize)];
    while let Some((node, depth)) = stack.pop() {
        if let Some(names) = array_of(arena, node.get(&arena.name("Names"))) {
            for pair in names.as_chunks::<2>().0 {
                let name = text_of(arena, &pair[0].resolve(arena)).unwrap_or_default();
                out.push((name, pair[1].clone()));
            }
        }
        if depth >= MAX_DEPTH {
            continue;
        }
        for kid in array_of(arena, node.get(&arena.name("Kids"))).unwrap_or_default() {
            if let Some(kid) = dict_of(arena, &kid) {
                stack.push((kid, depth + 1));
            }
        }
    }
    out
}

fn dict_of(arena: &PdfArena, object: &Object) -> Option<Dict> {
    arena.get_dict(object.resolve(arena).as_dict_handle()?)
}

fn array_of(arena: &PdfArena, object: Option<&Object>) -> Option<Vec<Object>> {
    match object?.resolve(arena) {
        Object::Array(handle) => arena.get_array(handle),
        _ => None,
    }
}

fn text_name(arena: &PdfArena, dict: &Dict, key: &str) -> Option<String> {
    let handle = dict.get(&arena.name(key))?.resolve(arena).as_name()?;
    Some(arena.get_name(handle)?.as_str().to_string())
}

fn text_string(arena: &PdfArena, dict: &Dict, key: &str) -> Option<String> {
    text_of(arena, &dict.get(&arena.name(key))?.resolve(arena))
}

fn text_of(arena: &PdfArena, object: &Object) -> Option<String> {
    let _ = arena;
    match object {
        Object::Text(text) => Some(text.clone()),
        Object::String(bytes) | Object::Hex(bytes) => {
            Some(crate::refine::text::recover_string(bytes))
        }
        _ => None,
    }
}
