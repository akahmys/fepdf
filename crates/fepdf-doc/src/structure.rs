//! PDF Logical Structure Engine (ISO 32000-2:2020 Clause 14.7)
//!
//! (ISO 14289-2 / PDF/UA-2 Compliance Bridge)

use fepdf_model::document::structure::StructElement;
use fepdf_model::{FromPdfObject, Handle, Object, PdfArena, PdfError, PdfResult};
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
pub struct MatterhornAuditor<'a> {
    /// Reference to the PDF arena for looking up objects.
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
/// The Matterhorn Protocol 1.1 is **31 checkpoints comprised of 136 failure conditions**,
/// of which 87 can be determined by software, 47 usually require human judgment, and 2
/// have no specific test. This reports two of them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditScope {
    /// The checkpoints this auditor looks at, by their protocol number.
    pub checked: Vec<String>,
    /// How many checkpoints the protocol has.
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
    /// Whether the audit found nothing — which is not the same as the document passing.
    ///
    /// **Named so that a caller cannot write `findings.is_empty()` and mean "conforms".**
    /// Three checkpoints of 136 finding nothing is three checkpoints finding nothing.
    #[must_use]
    pub fn found_nothing(&self) -> bool {
        self.findings.is_empty()
    }
}

impl<'a> MatterhornAuditor<'a> {
    /// Creates a new Matterhorn Auditor.
    pub fn new(arena: &'a PdfArena) -> Self {
        Self { arena }
    }

    /// The failure conditions this auditor looks at (Matterhorn Protocol 1.1).
    ///
    /// **All three of these were the wrong number until 2026-09-21.** The protocol was
    /// cited from memory and the numbers named other defects entirely: 14-001 is
    /// "Headings are not tagged" and is `Doc H` — a condition needing human judgment,
    /// which software may not report at all — where this checks that numbered levels are
    /// not skipped, which is 14-003. 13-001 is "Graphics objects … are not tagged with a
    /// `<Figure>` tag"; the missing alternative text is 13-004. Reporting a finding under
    /// the wrong number is not silence, it is testimony about a different defect.
    ///
    /// **Named one by one rather than counted**, so that adding a check and forgetting to
    /// say so is a thing the tests can notice.
    pub const CHECKED: [&'static str; 2] = ["13-004", "14-003"];

    /// How many failure conditions the Matterhorn Protocol 1.1 has, across 31 checkpoints.
    ///
    /// Its own text: "a set of 31 checkpoints comprised of 136 failure conditions
    /// encompassing file format requirements specified in **PDF/UA-1**".
    pub const IN_PROTOCOL: usize = 136;

    /// Performs a UA-2 structural audit, of the checkpoints in [`Self::CHECKED`].
    ///
    /// # Errors
    /// Fails when the structure tree cannot be read.
    pub fn audit_report(&self, root: Handle<Object>) -> PdfResult<AuditReport> {
        let mut findings = self.audit(root)?;
        // **A condition checked and not broken is a result.** One finding per condition,
        // not one per object that was sound: a reader wants to know that 13-004 was
        // examined, not that four hundred figures each have their alternative text.
        let broken: std::collections::BTreeSet<&str> =
            findings.iter().map(|f| f.checkpoint.as_str()).collect();
        let sound: Vec<AuditFinding> = Self::CHECKED
            .iter()
            .filter(|condition| !broken.contains(*condition))
            .map(|condition| AuditFinding {
                checkpoint: (*condition).to_string(),
                severity: "Pass".into(),
                outcome: Outcome::Sound,
                message: format!("{condition} was checked and this document does not break it"),
                handle_id: None,
            })
            .collect();
        findings.extend(sound);
        Ok(AuditReport {
            findings,
            scope: AuditScope {
                checked: Self::CHECKED.iter().map(|c| (*c).to_string()).collect(),
                in_protocol: Self::IN_PROTOCOL,
            },
        })
    }

    /// Performs a full UA-2 structural audit.
    pub fn audit(&self, root: Handle<Object>) -> PdfResult<Vec<AuditFinding>> {
        let mut findings = Vec::new();
        let mut visitor = StructureVisitor::new(self.arena, root);
        let mut last_heading_level = 0;

        while let Some(element_handle) = visitor.next_element() {
            self.audit_element(element_handle, &mut last_heading_level, &mut findings)?;
        }

        Ok(findings)
    }

    fn audit_element(
        &self,
        element_handle: Handle<Object>,
        last_heading: &mut i32,
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
        let tag_str = tag_name.as_str();

        if tag_str.starts_with('H')
            && tag_str.len() == 2
            && let Ok(lvl) = tag_str[1..].parse::<i32>()
        {
            if lvl > *last_heading + 1 {
                findings.push(AuditFinding {
                    checkpoint: "14-003".into(),
                    severity: "Error".into(),
                    outcome: Outcome::Broken,
                    message: format!("Heading level skipped: {tag_str} follows {last_heading}"),
                    handle_id: Some(element_handle.index()),
                });
            }
            *last_heading = lvl;
        }

        if tag_str == "Figure" && element.alt.is_none() {
            findings.push(AuditFinding {
                checkpoint: "13-004".into(),
                severity: "Error".into(),
                outcome: Outcome::Broken,
                message: "Figure element missing /Alt text".into(),
                handle_id: Some(element_handle.index()),
            });
        }

        Ok(())
    }
}
