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

fn resolve_to_node_handle(arena: &PdfArena, obj: &Object) -> Option<Handle<Object>> {
    match obj {
        Object::Reference(h) => Some(*h),
        Object::Dictionary(dh) => Some(Handle::new(dh.index())),
        _ => {
            let resolved = obj.resolve(arena);
            match resolved {
                Object::Reference(h) => Some(h),
                Object::Dictionary(dh) => Some(Handle::new(dh.index())),
                _ => None,
            }
        }
    }
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
                if let Some(kid_handle) = resolve_to_node_handle(self.arena, kids) {
                    self.stack.push_back(kid_handle);
                } else if let Object::Array(h) = kids.resolve(self.arena)
                    && let Some(array) = self.arena.get_array(h)
                {
                    for kid in array.iter().rev() {
                        if let Some(kid_handle) = resolve_to_node_handle(self.arena, kid) {
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
#[derive(Debug, Serialize, Deserialize)]
pub struct AuditFinding {
    /// The Matterhorn Protocol checkpoint ID (e.g., "01-001").
    pub checkpoint: String,
    /// The severity of the finding (e.g., "Error", "Warning").
    pub severity: String,
    /// A human-readable message describing the issue.
    pub message: String,
    /// The object handle ID associated with this finding, if any.
    pub handle_id: Option<u32>,
}

impl<'a> MatterhornAuditor<'a> {
    /// Creates a new Matterhorn Auditor.
    pub fn new(arena: &'a PdfArena) -> Self {
        Self { arena }
    }

    /// Performs a full UA-2 structural audit.
    pub fn audit(&self, root: Handle<Object>) -> PdfResult<Vec<AuditFinding>> {
        let mut findings = Vec::new();
        let mut visitor = StructureVisitor::new(self.arena, root);

        let mut last_heading_level = 0;

        while let Some(element_handle) = visitor.next_element() {
            let element =
                StructElement::from_pdf_object(Object::Reference(element_handle), self.arena)?;

            // Skip structure elements that lack /S (Subtype) — non-fatal, real-world PDFs may omit it.
            let Some(subtype_handle) = element.subtype else {
                continue;
            };

            let tag_name = self
                .arena
                .get_name(subtype_handle)
                .ok_or_else(|| PdfError::Other("Tag name not found".into()))?;
            let tag_str = tag_name.as_str();

            // 1. Heading Hierarchy Check (Matterhorn Checkpoint 14)
            if tag_str.starts_with('H')
                && tag_str.len() == 2
                && let Ok(level) = tag_str[1..].parse::<i32>()
            {
                if level > last_heading_level + 1 {
                    findings.push(AuditFinding {
                        checkpoint: "14-001".into(),
                        severity: "Error".into(),
                        message: format!(
                            "Heading level skipped: {tag_str} follows {last_heading_level}"
                        ),
                        handle_id: Some(element_handle.index()),
                    });
                }
                last_heading_level = level;
            }

            // 2. Alt-text Check (Matterhorn Checkpoint 13)
            if tag_str == "Figure" && element.alt.is_none() {
                findings.push(AuditFinding {
                    checkpoint: "13-001".into(),
                    severity: "Error".into(),
                    message: "Figure element missing /Alt text".into(),
                    handle_id: Some(element_handle.index()),
                });
            }
        }

        Ok(findings)
    }
}
