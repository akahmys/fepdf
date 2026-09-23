//! PDF Logical Structure Engine (ISO 32000-2:2020 Clause 14.7)
//!
//! (ISO 14289-2 / PDF/UA-2 Compliance Bridge)

use fepdf_model::document::structure::StructElement;
use fepdf_model::{Document, FromPdfObject, Handle, Object, PdfArena, PdfError, PdfResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

/// A visitor for traversing the Logical Structure Tree iteratively (RR-15 compliant).
pub struct StructureVisitor<'a> {
    /// Reference to the PDF arena.
    pub arena: &'a PdfArena,
    /// Stack for iterative DFS traversal.
    pub stack: VecDeque<Handle<Object>>,
    /// Set of visited nodes to prevent infinite loops in cyclic structures.
    pub visited: BTreeSet<Handle<Object>>,
}

impl<'a> StructureVisitor<'a> {
    /// Creates a new visitor starting from the given structure root.
    pub fn new(arena: &'a PdfArena, root: Handle<Object>) -> Self {
        let mut stack = VecDeque::new();
        stack.push_back(root);
        Self { arena, stack, visited: BTreeSet::new() }
    }

    /// Iteratively walks the tree and yields structure elements.
    pub fn next_element(&mut self) -> Option<Handle<Object>> {
        let current = self.stack.pop_back()?;

        if !self.visited.insert(current) {
            // Cycle detected - skip this node to prevent infinite loop
            return self.next_element();
        }

        let obj = self.arena.get_object(current)?;
        if let Some(dh) = obj.as_dict_handle()
            && let Some(dict) = self.arena.get_dict(dh)
        {
            let kids_key = self.arena.name("K");

            if let Some(kids) = dict.get(&kids_key) {
                if let Some(kid_handle) =
                    crate::struct_tree::resolve_to_node_handle(self.arena, kids)
                {
                    self.stack.push_back(kid_handle);
                } else if let Object::Array(h) = kids.resolve(self.arena)
                    && let Some(array) = self.arena.get_array(h)
                {
                    for kid in array.iter().rev() {
                        if let Some(kid_handle) =
                            crate::struct_tree::resolve_to_node_handle(self.arena, kid)
                        {
                            self.stack.push_back(kid_handle);
                        }
                    }
                }
            }
        }

        Some(current)
    }
}

/// Matterhorn-compliant Structural Auditor.
///
/// **It holds the document, not only the arena.** Four of the failure conditions it
/// reports are properties of the catalogue or of the interactive form, and none of those
/// is reachable from a structure element: a document with no structure tree at all still
/// has a `/ViewerPreferences` to be asked about, and reporting nothing but "not a tagged
/// PDF" about it would leave four conditions unexamined that nothing prevents examining.
pub struct MatterhornAuditor<'a> {
    /// The document under audit.
    doc: &'a Document,
    /// Its arena, for looking up objects.
    arena: &'a PdfArena,
}

/// Represents a single finding from a structural audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditFinding {
    /// The Matterhorn Protocol failure condition (e.g., "13-004").
    pub checkpoint: String,
    /// The severity of the finding (e.g., "Error", "Warning").
    ///
    /// **Kept for the callers that read it as a string**, and no longer what decides how
    /// a finding is read: [`Self::outcome`] does. A severity spelt `Error` and one spelt
    /// `error` are two strings and one meaning, which is why the classification is not a
    /// string — `compliance.rs` records the last time stringifying a severity lost it,
    /// when a `Violation` and a `Repaired` arrived at the CLI identically.
    pub severity: String,
    /// What checking this condition came to.
    pub outcome: Outcome,
    /// A human-readable message describing the issue.
    pub message: String,
    /// The object handle ID associated with this finding, if any.
    pub handle_id: Option<u32>,
}

/// What came of checking one failure condition.
///
/// **A report says what was looked at, not only what was wrong.** A condition examined and
/// found sound is a result a reader is owed; one nobody examined is a different thing
/// entirely, and the two used to be told apart by an empty list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// Checked, and the document breaks it.
    Broken,
    /// Checked, and the document does not break it.
    ///
    /// **Only for a condition this engine actually checks.** "Nothing was found" and
    /// "nothing was looked for" are not the same answer, and a report that showed them
    /// alike would say a document conforms on the strength of work nobody did.
    Sound,
    /// Not decided here. The protocol marks it `H`, or this engine has evidence and no
    /// answer — the finding carries what it found and leaves the judgment to a reader.
    ForAReader,
}

/// What an audit looked at, beside what it found.
///
/// **A clean report from a check that was never run is the worst answer this engine can
/// give**, and until this it was the answer it gave: `audit` returned findings and a
/// caller had no way to tell "nothing is wrong" from "almost nothing was examined".
///
/// The Matterhorn Protocol 1.1 is **31 checkpoints**, whose tables enumerate **137
/// failure conditions** — 87 that can be determined by software, 48 that usually require
/// human judgment, and 2 with no specific test (23-001 and 27-001). Its prose says 136
/// and 47, which is version 1.02's count carried into 1.1 without the `H` condition 1.1
/// added; see [`MatterhornAuditor::IN_PROTOCOL`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditScope {
    /// The failure conditions this auditor looks at, by their protocol number.
    pub checked: Vec<String>,
    /// The ones the protocol leaves to a person, with its own wording.
    ///
    /// **The same 48 for every document**, because what this says is a property of the
    /// protocol and not of the file — which is why they are here and not among the
    /// findings. A reader who is told only what was checked, and a count of what was not,
    /// has no way to know which of the rest are questions someone is expected to answer
    /// and which are simply unimplemented.
    pub left_to_a_person: Vec<crate::matterhorn::LeftToAPerson>,
    /// How many failure conditions the protocol has.
    pub in_protocol: usize,
}

/// What an audit found, and what it looked for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    /// What was found.
    pub findings: Vec<AuditFinding>,
    /// What was looked at.
    pub scope: AuditScope,
}

impl AuditReport {
    /// Whether the audit found nothing a reader must act on — which is not the same as
    /// the document passing.
    ///
    /// **Named so that a caller cannot write `findings.is_empty()` and mean "conforms".**
    /// Fourteen failure conditions out of 137 finding nothing is fourteen failure
    /// conditions finding nothing. Read [`AuditReport::scope`] beside this.
    ///
    /// **This was `findings.is_empty()`, and that could not be true.** Once a checked and
    /// unbroken condition became a `Sound` finding, a clean document carried one row per
    /// condition in `CHECKED` and an empty list became unreachable: the method always
    /// answered `false`, and the test asserting `!found_nothing()` on a broken document
    /// could no longer fail. It asks what it is named for — whether anything is `Broken`
    /// or waiting `ForAReader`.
    #[must_use]
    pub fn found_nothing(&self) -> bool {
        !self.findings.iter().any(|f| matches!(f.outcome, Outcome::Broken | Outcome::ForAReader))
    }
}

/// The failure conditions decided by reading the catalogue (7.7.2).
///
/// Separate from the ones below because a document reaches them by a different route:
/// these are true or false of a file with no structure tree at all.
///
/// **Checkpoint 06 is not among them, and cannot be.** 06-001 is "document does not
/// contain an XMP metadata stream" and 06-003 is "XMP metadata stream does not contain
/// `dc:title`" — and `metadata::settle` writes a packet into the catalogue at ingest, and
/// promotes `/Info`'s `/Title` into it. What this auditor reads is the document as
/// ingested ([ADR-0013](../../../docs/adr/0013-a-document-is-one-normalised-state.md)), so
/// 06-001 would answer sound for every file this engine can open,
/// and 06-003 would answer sound for a file whose only title is in the deprecated
/// dictionary — which is a file that breaks it. Checking either means auditing the bytes
/// before ingestion repairs them, which is machinery this item does not have
/// ([ADR-0094](../../../docs/adr/0094-the-auditor-reads-the-ingested-document-so-ingestion-answers-checkpoint-06.md)).
/// `checkpoint_06_is_left_out_because_ingestion_answers_it` holds the reason to the code.
pub const FROM_CATALOGUE: [&str; 3] = ["01-007", "07-001", "07-002"];

/// The failure conditions decided by reading the interactive form (12.7).
pub const FROM_FORM: [&str; 1] = ["28-005"];

/// The failure conditions decided by reading the pages' content streams (14.7.4.2).
///
/// **A tree says which marks belong to which element; it does not say what is on the
/// page.** PDF/UA-1 7.1 is a requirement about all content, and the three conditions of
/// checkpoint 01 that this engine can decide are about what a `BDC` encloses — which is
/// in the content stream and nowhere else. See [`crate::tagging`].
pub const FROM_CONTENT: [&str; 3] = ["01-003", "01-004", "01-005"];

/// The failure conditions decided by walking the structure tree (14.7).
///
/// **A document with no structure tree has none of these examined**, which is not the
/// same as passing them, and is why the three lists are apart.
pub const FROM_STRUCTURE_TREE: [&str; 7] =
    ["11-002", "13-004", "14-002", "14-003", "14-006", "14-007", "17-002"];

/// What a `/StructTreeRoot` that is not there is reported as.
///
/// **Not a failure-condition number.** It was `00-001`, which is a number the protocol
/// does not have — the same defect W-21e removed from the auditor, surviving one level
/// up because the scope test only ever read a document that had a structure tree. PDF/UA-1
/// requires the tree in 7.1, and the protocol's own Section notation for that clause is
/// this, so what a reader looks up is a clause that exists.
pub const NO_STRUCTURE_TREE: &str = "UA1:7.1";

/// What the numbered headings seen so far come to.
///
/// Three of checkpoint 14's conditions are conclusions about the *document*, not about an
/// element: whether the first numbered heading is `<H1>`, whether a level was skipped,
/// and whether both spellings of a heading are in use. They are gathered while the walk
/// runs and decided when it ends.
#[derive(Default)]
struct Headings {
    /// The level of the last `<Hn>` seen, which 14-003 compares the next one against.
    last_level: i32,
    /// The first `<Hn>`: its level, and the element that carried it (14-002).
    first_numbered: Option<(i32, u32)>,
    /// The first unnumbered `<H>` (14-007).
    first_unnumbered: Option<u32>,
}

impl Headings {
    /// What the walk as a whole says about the headings.
    fn conclude(&self, findings: &mut Vec<AuditFinding>) {
        let Some((level, at)) = self.first_numbered else {
            // 14-002 and 14-007 both begin "the document uses numbered headings". One
            // that uses none breaks neither.
            return;
        };
        if level != 1 {
            findings.push(broken_at(
                "14-002",
                format!("Document uses numbered headings and the first is <H{level}>, not <H1>"),
                at,
            ));
        }
        if let Some(unnumbered) = self.first_unnumbered {
            findings.push(broken_at(
                "14-007",
                "Document uses both <H> and <H#> tags".to_string(),
                unnumbered,
            ));
        }
    }
}

/// `<H1>` through `<Hn>`, as the level it names.
///
/// **Any number of digits, not two characters.** The test was `len() == 2`, which stops
/// at `<H9>` — and 14-005 is about a seventh level and higher, so a document deep enough
/// to need `<H10>` is one this was written for rather than one it may ignore.
fn numbered_heading_level(tag: &str) -> Option<i32> {
    tag.strip_prefix('H').filter(|rest| !rest.is_empty())?.parse().ok()
}

/// Whether a string is there and says something.
///
/// An entry written as the empty string is present and carries nothing, and 14.9.2.2 says
/// so outright for `/Lang`: the empty string means the language is unknown. Treating it
/// as an answer is how a file passes by writing `()`.
fn stated(value: Option<&String>) -> bool {
    value.is_some_and(|text| !text.trim().is_empty())
}

/// A condition this engine decided the document breaks.
fn broken(condition: &str, message: impl Into<String>) -> AuditFinding {
    AuditFinding {
        checkpoint: condition.to_string(),
        severity: "Error".into(),
        outcome: Outcome::Broken,
        message: message.into(),
        handle_id: None,
    }
}

/// The same, naming the object it was decided about.
fn broken_at(condition: &str, message: impl Into<String>, at: u32) -> AuditFinding {
    AuditFinding { handle_id: Some(at), ..broken(condition, message) }
}

/// A condition this engine will not decide, with what it found for whoever does.
///
/// **The evidence is the message, or this is not worth showing.** "28-005 suspected"
/// tells a reader nothing they can act on; the field's name and what was and was not
/// resolved about it is what lets them agree or disagree.
fn for_a_reader(condition: &str, message: impl Into<String>) -> AuditFinding {
    AuditFinding {
        checkpoint: condition.to_string(),
        severity: "Warning".into(),
        outcome: Outcome::ForAReader,
        message: message.into(),
        handle_id: None,
    }
}

/// A condition that was examined and came out sound.
fn sound_row(condition: &str) -> AuditFinding {
    AuditFinding {
        checkpoint: condition.to_string(),
        severity: "Pass".into(),
        outcome: Outcome::Sound,
        message: format!("{condition} was checked and this document does not break it"),
        handle_id: None,
    }
}

impl<'a> MatterhornAuditor<'a> {
    /// Creates a new Matterhorn Auditor.
    pub fn new(doc: &'a Document) -> Self {
        Self { doc, arena: doc.arena() }
    }

    /// The failure conditions this auditor looks at (Matterhorn Protocol 1.1).
    ///
    /// **All three of the first numbers here were the wrong number until 2026-09-21.** The
    /// protocol was cited from memory and the numbers named other defects entirely:
    /// 14-001 is "Headings are not tagged" and is `Doc H` — a condition needing human
    /// judgment, which software may not report at all — where this checks that numbered
    /// levels are not skipped, which is 14-003. 13-001 is "Graphics objects … are not
    /// tagged with a `<Figure>` tag"; the missing alternative text is 13-004. Reporting a
    /// finding under the wrong number is not silence, it is testimony about a different
    /// defect.
    ///
    /// **Named one by one rather than counted**, so that adding a check and forgetting to
    /// say so is a thing the tests can notice. The three lists above partition this one,
    /// and a test holds them to it.
    pub const CHECKED: [&'static str; 14] = [
        "01-003", "01-004", "01-005", "01-007", "07-001", "07-002", "11-002", "13-004", "14-002",
        "14-003", "14-006", "14-007", "17-002", "28-005",
    ];

    /// How many failure conditions the Matterhorn Protocol 1.1 has, across 31 checkpoints.
    ///
    /// **Its tables enumerate 137 and its prose says 136**, and the tables are what a
    /// number can be looked up in. The prose — "a set of 31 checkpoints comprised of 136
    /// failure conditions … 87 … determined by software alone, 47 … require human
    /// judgment, 2 … have no specific tests" — is version 1.02's, carried into 1.1
    /// unrevised: 1.1's own Document History records "Failure condition 13-008 added", and
    /// 13-008 is marked `H`, which is why counting the `How` column gives 87 `M` and
    /// **48** `H` beside the 2 with no test. `every_failure_condition_in_the_protocol_is_counted`
    /// derives this by reading the Index column of `docs/specs/Matterhorn-Protocol-1-1.pdf`,
    /// and holds the prose to 136 as well, so an edition that corrects the sentence is
    /// noticed rather than silently agreed with
    /// ([ADR-0093](../../../docs/adr/0093-the-protocols-tables-enumerate-137-failure-conditions.md)).
    pub const IN_PROTOCOL: usize = 137;

    /// Performs a UA-2 structural audit, of the failure conditions in [`Self::CHECKED`].
    ///
    /// # Errors
    /// Fails when the structure tree cannot be read.
    pub fn audit_report(&self) -> PdfResult<AuditReport> {
        let mut findings = Vec::new();
        let mut examined: BTreeSet<&'static str> = BTreeSet::new();
        self.audit_catalogue(&mut findings, &mut examined);
        self.audit_form(&mut findings, &mut examined);
        self.audit_content(&mut findings, &mut examined);
        match self.doc.get_structure_root()? {
            Some(root) => {
                findings.extend(self.audit(root)?);
                examined.extend(FROM_STRUCTURE_TREE);
            }
            // Not a tagged PDF, which is the one thing this can say without looking at a
            // condition at all — and the conditions the tree would have decided stay out
            // of `examined`, so none of them is called sound.
            None => findings.push(broken(
                NO_STRUCTURE_TREE,
                format!(
                    "Document has no /StructTreeRoot and is not a tagged PDF, so the {} \
                     failure conditions decided by walking the structure tree were not \
                     examined",
                    FROM_STRUCTURE_TREE.len()
                ),
            )),
        }
        // **A condition checked and not broken is a result.** One finding per condition,
        // not one per object that was sound: a reader wants to know that 13-004 was
        // examined, not that four hundred figures each have their alternative text.
        let reported: BTreeSet<&str> = findings.iter().map(|f| f.checkpoint.as_str()).collect();
        let sound: Vec<AuditFinding> = examined
            .iter()
            .filter(|condition| !reported.contains(*condition))
            .map(|condition| sound_row(condition))
            .collect();
        findings.extend(sound);
        Ok(AuditReport { findings, scope: Self::scope() })
    }

    /// What this auditor looks at, for a report to carry beside what it found.
    #[must_use]
    pub fn scope() -> AuditScope {
        AuditScope {
            checked: Self::CHECKED.iter().map(|c| (*c).to_string()).collect(),
            left_to_a_person: crate::matterhorn::LEFT_TO_A_PERSON
                .iter()
                .map(|(condition, wording)| crate::matterhorn::LeftToAPerson {
                    condition: (*condition).to_string(),
                    wording: (*wording).to_string(),
                })
                .collect(),
            in_protocol: Self::IN_PROTOCOL,
        }
    }

    /// The conditions that are properties of the catalogue (7.7.2).
    ///
    /// A catalogue that will not read leaves all three unexamined rather than sound: the
    /// answer to "does `/ViewerPreferences` say `/DisplayDocTitle`" is not "no" when the
    /// dictionary holding it could not be got at.
    fn audit_catalogue(
        &self,
        findings: &mut Vec<AuditFinding>,
        examined: &mut BTreeSet<&'static str>,
    ) {
        let Ok(catalogue) = self.doc.catalog() else {
            return;
        };
        examined.insert("01-007");
        if catalogue.mark_info.as_ref().and_then(|info| info.suspects) == Some(true) {
            findings.push(broken(
                "01-007",
                "/MarkInfo /Suspects is true: the document declares its own tag structure \
                 unreliable",
            ));
        }
        Self::audit_viewer_preferences(catalogue.viewer_preferences.as_ref(), findings, examined);
    }

    /// 07-001 and 07-002, from `/ViewerPreferences` (12.2).
    fn audit_viewer_preferences(
        preferences: Option<&fepdf_model::document::ViewerPreferences>,
        findings: &mut Vec<AuditFinding>,
        examined: &mut BTreeSet<&'static str>,
    ) {
        examined.extend(["07-001", "07-002"]);
        match preferences.and_then(|prefs| prefs.display_doc_title) {
            None => findings.push(broken(
                "07-001",
                "/ViewerPreferences states no /DisplayDocTitle, so a reader is shown the \
                 file's name where its title belongs",
            )),
            Some(false) => {
                findings.push(broken("07-002", "/ViewerPreferences states /DisplayDocTitle false"));
            }
            Some(true) => {}
        }
    }

    /// 28-005, from the interactive form (12.7.4).
    ///
    /// **Left for a reader, because half of the condition is not reachable from here.**
    /// It reads "a form field does not have a `TU` entry **and** does not have an
    /// alternative description (in the form of an `Alt` entry in the enclosing structure
    /// element)", and the enclosing structure element is reached through an `/OBJR`, which
    /// `struct_tree.rs` resolves to nothing. A field that *has* a `/TU` is decided — the
    /// conjunction fails on its first half — so the condition comes out sound for a
    /// document whose fields all carry one, and for a document with no form at all.
    fn audit_form(&self, findings: &mut Vec<AuditFinding>, examined: &mut BTreeSet<&'static str>) {
        examined.insert("28-005");
        let form = fepdf_model::interactive::form_of(self.doc);
        for field in &form.terminal {
            if stated(field.tooltip.as_ref()) {
                continue;
            }
            let name = field
                .qualified_name
                .as_deref()
                .or(field.name.as_deref())
                .unwrap_or("a field with no /T");
            findings.push(for_a_reader(
                "28-005",
                format!(
                    "The form field \"{name}\" states no /TU. Whether an /Alt on its \
                     enclosing structure element describes it instead is not resolved \
                     here, because /OBJR is not followed — look at the field"
                ),
            ));
        }
    }

    /// Checkpoint 01's three, from what each page's content stream marks.
    ///
    /// **One finding per page per condition, with a count.** A page of four hundred
    /// untagged glyphs is one thing wrong with one page, and four hundred rows saying so
    /// is the list W-21f replaced with a report. The unit a reader acts on here is the
    /// page.
    ///
    /// **A page whose content will not decode leaves all three unexamined.** Calling them
    /// sound on the strength of the pages that did read would be a claim about the
    /// document, made from part of it.
    fn audit_content(
        &self,
        findings: &mut Vec<AuditFinding>,
        examined: &mut BTreeSet<&'static str>,
    ) {
        let Ok(pages) = self.doc.page_count() else {
            return;
        };
        let mut unreadable = Vec::new();
        for page in 0..pages {
            match crate::tagging::tagging_of_page(self.doc, page) {
                Ok(tagging) => Self::note_page_tagging(page, &tagging, findings),
                Err(_) => unreadable.push((page + 1).to_string()),
            }
        }
        if unreadable.is_empty() {
            examined.extend(FROM_CONTENT);
            return;
        }
        findings.push(for_a_reader(
            "01-005",
            format!(
                "The content of {} of the document's {pages} pages would not decode, so what \
                 they mark is not established here — pages {}",
                unreadable.len(),
                unreadable.join(", ")
            ),
        ));
    }

    /// What one page's marked content came to, as the report says it.
    fn note_page_tagging(
        page: usize,
        tagging: &crate::tagging::PageTagging,
        findings: &mut Vec<AuditFinding>,
    ) {
        let at = page + 1;
        if tagging.artifact_inside_tagged > 0 {
            findings.push(broken(
                "01-003",
                format!(
                    "Page {at}: an /Artifact sequence is inside tagged content ({} on this \
                     page)",
                    tagging.artifact_inside_tagged
                ),
            ));
        }
        if tagging.tagged_inside_artifact > 0 {
            findings.push(broken(
                "01-004",
                format!(
                    "Page {at}: content carrying an /MCID is inside an /Artifact ({} on this \
                     page)",
                    tagging.tagged_inside_artifact
                ),
            ));
        }
        if tagging.untagged_marks > 0 {
            findings.push(broken(
                "01-005",
                format!(
                    "Page {at}: content is marked as neither an /Artifact nor real content \
                     ({} painting operators on this page)",
                    tagging.untagged_marks
                ),
            ));
        }
        if !tagging.forms_outside.is_empty() {
            findings.push(for_a_reader(
                "01-005",
                format!(
                    "Page {at}: the form XObject {} is drawn under neither a tag nor an \
                     /Artifact. Its own content stream may carry the marks and this walk does \
                     not descend into it — look at the form",
                    tagging.forms_outside.join(", ")
                ),
            ));
        }
    }

    /// Walks the structure tree, for the conditions that are properties of it.
    ///
    /// # Errors
    /// Fails when an element of the tree cannot be read.
    pub fn audit(&self, root: Handle<Object>) -> PdfResult<Vec<AuditFinding>> {
        let mut findings = Vec::new();
        let mut headings = Headings::default();
        let document_language = self.document_language();
        let mut visitor = StructureVisitor::new(self.arena, root);

        while let Some(element_handle) = visitor.next_element() {
            self.audit_element(
                element_handle,
                &mut headings,
                document_language.as_deref(),
                &mut findings,
            )?;
        }
        headings.conclude(&mut findings);
        Ok(findings)
    }

    /// The catalogue's `/Lang` (14.9.2.1), which every element inherits that states none.
    fn document_language(&self) -> Option<String> {
        self.doc.catalog().ok()?.lang.filter(|tag| !tag.trim().is_empty())
    }

    fn audit_element(
        &self,
        element_handle: Handle<Object>,
        headings: &mut Headings,
        document_language: Option<&str>,
        findings: &mut Vec<AuditFinding>,
    ) -> PdfResult<()> {
        let element =
            StructElement::from_pdf_object(Object::Reference(element_handle), self.arena)?;
        let Some(subtype_handle) = element.subtype else {
            return Ok(());
        };
        let tag_name = self
            .arena
            .get_name(subtype_handle)
            .ok_or_else(|| PdfError::Other("Tag name not found".into()))?;
        let tag = tag_name.as_str();
        let at = element_handle.index();

        Self::audit_heading(tag, at, headings, findings);
        self.audit_one_heading_per_node(&element, at, findings);
        Self::audit_alternative_text(tag, &element, at, findings);
        self.audit_language(&element, document_language, at, findings);
        Ok(())
    }

    /// 14-003 as the walk passes, and what 14-002 and 14-007 will be decided from.
    fn audit_heading(
        tag: &str,
        at: u32,
        headings: &mut Headings,
        findings: &mut Vec<AuditFinding>,
    ) {
        if tag == "H" {
            headings.first_unnumbered.get_or_insert(at);
            return;
        }
        let Some(level) = numbered_heading_level(tag) else {
            return;
        };
        if level > headings.last_level + 1 {
            findings.push(broken_at(
                "14-003",
                format!("Heading level skipped: {tag} follows {}", headings.last_level),
                at,
            ));
        }
        headings.first_numbered.get_or_insert((level, at));
        headings.last_level = level;
    }

    /// 14-006: a node with more than one `<H>` among its children.
    ///
    /// **The children of one node, not every `<H>` beneath it.** A `<Sect>` holding two
    /// `<Sect>`s, each with a heading of its own, is two nodes with one heading each and
    /// breaks nothing — which is what makes this a question about a node rather than
    /// about the document, and why 14-007 (both spellings in use) is the one that is
    /// document-wide.
    fn audit_one_heading_per_node(
        &self,
        element: &StructElement,
        at: u32,
        findings: &mut Vec<AuditFinding>,
    ) {
        let headings = self
            .child_elements(element)
            .into_iter()
            .filter(|kid| self.tag_of(*kid) == Some("H".into()))
            .count();
        if headings > 1 {
            findings.push(broken_at(
                "14-006",
                format!("Node holds {headings} <H> children, where 14-006 allows one"),
                at,
            ));
        }
    }

    /// The structure elements a node's `/K` names, skipping marks and `/OBJR`s.
    fn child_elements(&self, element: &StructElement) -> Vec<Handle<Object>> {
        let mut out = Vec::new();
        let Some(kids) = &element.kids else {
            return out;
        };
        if let Some(one) = crate::struct_tree::resolve_to_node_handle(self.arena, kids) {
            out.push(one);
            return out;
        }
        if let Object::Array(handle) = kids.resolve(self.arena)
            && let Some(array) = self.arena.get_array(handle)
        {
            out.extend(
                array
                    .iter()
                    .filter_map(|kid| crate::struct_tree::resolve_to_node_handle(self.arena, kid)),
            );
        }
        out
    }

    /// One element's `/S`, as the name it is written with.
    fn tag_of(&self, handle: Handle<Object>) -> Option<String> {
        let element = StructElement::from_pdf_object(Object::Reference(handle), self.arena).ok()?;
        Some(self.arena.get_name(element.subtype?)?.as_str().to_string())
    }

    /// 13-004 and 17-002, which are two different requirements about one word.
    ///
    /// **13-004 is "alternative *or replacement* text missing"**, and this read `/Alt`
    /// alone: a `<Figure>` carrying `/ActualText` and no `/Alt` was reported as breaking a
    /// condition it does not break, which is 7.3 paragraph 3's own "Alt or ActualText".
    /// 17-002 really is `/Alt` alone — "`<Formula>` tag is missing an Alt attribute" — so
    /// the two are not the same test spelt twice.
    fn audit_alternative_text(
        tag: &str,
        element: &StructElement,
        at: u32,
        findings: &mut Vec<AuditFinding>,
    ) {
        if tag == "Figure" && !stated(element.alt.as_ref()) && !stated(element.actual_text.as_ref())
        {
            findings.push(broken_at(
                "13-004",
                "Figure element states neither /Alt nor /ActualText",
                at,
            ));
        }
        if tag == "Formula" && !stated(element.alt.as_ref()) {
            findings.push(broken_at("17-002", "Formula element states no /Alt", at));
        }
    }

    /// 11-002: the natural language of `/Alt`, `/ActualText` and `/E` (14.9.2.2).
    ///
    /// An element carrying one of the three and reached by no `/Lang` — its own, an
    /// ancestor's, or the catalogue's — has text whose language a reader cannot determine.
    fn audit_language(
        &self,
        element: &StructElement,
        document_language: Option<&str>,
        at: u32,
        findings: &mut Vec<AuditFinding>,
    ) {
        let carries = stated(element.alt.as_ref())
            || stated(element.actual_text.as_ref())
            || stated(element.expanded.as_ref());
        if !carries || self.language_in_force(element, document_language).is_some() {
            return;
        }
        findings.push(broken_at(
            "11-002",
            "Element states /Alt, /ActualText or /E and no /Lang reaches it, so the \
             natural language of that text cannot be determined",
            at,
        ));
    }

    /// The `/Lang` in force for an element: its own, else the nearest ancestor's through
    /// `/P`, else the catalogue's (14.9.2.2).
    ///
    /// Walked with a stack and a visited set rather than by recursion (RR-15 Rule 6), and
    /// the visited set is not only about depth: `/P` in a damaged file can point back into
    /// the subtree it came from, and the chain would not end.
    fn language_in_force(
        &self,
        element: &StructElement,
        document_language: Option<&str>,
    ) -> Option<String> {
        if stated(element.lang.as_ref()) {
            return element.lang.clone();
        }
        let mut seen = BTreeSet::new();
        let mut at = element.parent;
        while let Some(handle) = at {
            if !seen.insert(handle) {
                break;
            }
            let Ok(ancestor) =
                StructElement::from_pdf_object(Object::Reference(handle), self.arena)
            else {
                break;
            };
            if stated(ancestor.lang.as_ref()) {
                return ancestor.lang;
            }
            at = ancestor.parent;
        }
        document_language.map(str::to_owned)
    }
}
