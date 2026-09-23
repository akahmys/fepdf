//! Document mutation vocabulary, structure tree processing, and PDF/UA-2 remediation.
//!
//! Owns the [`Operation`] vocabulary (ARCHITECTURE.md §4.1) and is its only interpreter:
//! rotate, reorder, remove, portfolio, outlines, layers, annotations, form fields, security.
//! Also provides Matterhorn structural auditing, logical structure tree extraction,
//! and automated structural remediation.

/// Dispatcher and domain modules for applying operations to documents.
pub mod apply;
/// Object graph cloning.
pub mod cloning;
/// Where a page's marked content landed, for the structure tree to read.
pub mod marked_content;
/// The Matterhorn Protocol's own text, for the conditions it leaves to a person.
pub mod matterhorn;
/// Canonical document mutation operations.
pub mod operation;
/// Reading the bookmark tree back out of a document.
pub mod outline_tree;
/// Structural remediation and redaction.
pub mod remediation;
/// Logical structure tree visitor and presentation data.
pub mod struct_tree;
/// PDF logical structure auditor and visitor.
pub mod structure;
/// Whether what a page draws is tagged, marked as an artefact, or neither.
pub mod tagging;

pub use apply::apply_operation;
pub use matterhorn::{LEFT_TO_A_PERSON, LeftToAPerson};
pub use operation::*;
pub use outline_tree::{OutlineReport, read_outlines};
pub use remediation::apply_physical_redaction_to_page;
pub use struct_tree::{Placement, StructureTreeNode, StructureTreeVisitor};
pub use structure::{
    AuditFinding, AuditReport, AuditScope, FROM_CATALOGUE, FROM_CONTENT, FROM_FORM,
    FROM_STRUCTURE_TREE, MatterhornAuditor, NO_STRUCTURE_TREE, Outcome, StructureVisitor,
};
