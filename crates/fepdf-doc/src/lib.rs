//! Document mutation vocabulary, structure tree processing, and PDF/UA-2 remediation.
//!
//! Owns the [`Operation`] vocabulary (ARCHITECTURE.md §4.1) and is its only interpreter:
//! rotate, reorder, remove, portfolio, outlines, layers, annotations, form fields, security.
//! Also provides logical structure tree extraction, reading order, measurement and
//! automated structural remediation. Judging a document is `fepdf-audit`'s, which stands
//! above this crate.

/// Dispatcher and domain modules for applying operations to documents.
pub mod apply;
/// New documents built from existing ones: merged, extracted, or copied for a writer.
pub mod assembly;
/// Object graph cloning.
pub mod cloning;
pub mod comments;
/// Where a page's marked content landed, for the structure tree to read.
pub mod marked_content;
/// The scale a drawing declares for measuring on it (12.9).
pub mod measure;
/// Canonical document mutation operations.
pub mod operation;
/// Reading the bookmark tree back out of a document.
pub mod outline_tree;
/// What a page taken out of a document leaves behind, taken out with it.
pub mod page_removal;
/// The structure tree's parent tree, as the elements annotations belong to.
pub mod parent_tree;
/// The text of a tagged document in reading order, for a synthesiser.
pub mod reading;
/// Structural remediation and redaction.
pub mod remediation;
/// Logical structure tree visitor and presentation data.
pub mod struct_tree;
/// The replacement text of structure elements a redaction touched, made a marker.
mod struct_tree_marking;
/// Structure content on pages taken out, taken out of the tree.
mod struct_tree_pruning;
/// Whether what a page draws is tagged, marked as an artefact, or neither.
pub mod tagging;

pub use apply::apply_operation;
pub use operation::*;
pub use outline_tree::{OutlineReport, read_outlines};
pub use struct_tree::{Placement, StructureTreeNode, StructureTreeVisitor};
