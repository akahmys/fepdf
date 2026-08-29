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
/// Canonical document mutation operations.
pub mod operation;
/// Structural remediation and redaction.
pub mod remediation;
/// Logical structure tree visitor and presentation data.
pub mod struct_tree;
/// PDF logical structure auditor and visitor.
pub mod structure;

pub use apply::apply_operation;
pub use operation::*;
pub use remediation::apply_physical_redaction_to_page;
pub use struct_tree::{StructureTreeNode, StructureTreeVisitor};
pub use structure::{AuditFinding, MatterhornAuditor, StructureVisitor};
