//! Document mutation vocabulary, structure tree processing, and PDF/UA-2 remediation.
//!
//! Owns the [`Operation`] vocabulary (ARCHITECTURE.md §4.1) and is its only interpreter:
//! rotate, reorder, remove, portfolio, outlines, layers, annotations, form fields, security.
//! Also provides Matterhorn structural auditing, logical structure tree extraction,
//! and automated structural remediation.

/// Dispatcher and domain modules for applying operations to documents.
pub mod apply;
/// New documents built from existing ones: merged, extracted, or copied for a writer.
pub mod assembly;
mod audit_cmaps;
/// Matterhorn conditions about embedded files, XFA, encryption, media and shared forms.
pub mod audit_files;
/// Matterhorn conditions about font dictionaries and embedded programs.
pub mod audit_fonts;
/// Matterhorn conditions decided by objects outside the structure tree.
pub mod audit_objects;
/// Matterhorn conditions marked `H`, decided where the document gives the answer.
pub mod audit_presence;
/// Matterhorn conditions about a font descriptor's `/CharSet` and `/CIDSet`.
pub mod audit_subsets;
/// Matterhorn conditions about role mapping, notes and table headers.
pub mod audit_tree;
/// Object graph cloning.
pub mod cloning;
/// Which marked content lies in a `<Formula>` (17-003).
pub mod formula_marks;
/// Which glyph each character code of a font selects, as ISO 32000-1 says.
pub mod glyph_map;
/// What the codes a font's text shows select: 31-011 and 31-030.
pub mod glyph_select;
/// A glyph's width in the font dictionary and in the program (31-016).
pub mod glyph_widths;
/// Where a page's marked content landed, for the structure tree to read.
pub mod marked_content;
/// The Matterhorn Protocol's own text, for the conditions it leaves to a person.
pub mod matterhorn;
/// The scale a drawing declares for measuring on it (12.9).
pub mod measure;
/// Canonical document mutation operations.
pub mod operation;
/// Reading the bookmark tree back out of a document.
pub mod outline_tree;
/// Whether the language of the text in page content can be determined (11-001).
pub mod page_languages;
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
/// PDF logical structure auditor and visitor.
pub mod structure;
/// Whether what a page draws is tagged, marked as an artefact, or neither.
pub mod tagging;
/// Whether a TrueType font's rendered codes reach a glyph through its cmap (31-018).
pub mod truetype_lookup;
/// Whether each character code shown maps to Unicode (10-001).
pub mod unicode_map;

pub use apply::apply_operation;
pub use matterhorn::{LeftToAPerson, MARKED_H, left_to_a_person};
pub use operation::*;
pub use outline_tree::{OutlineReport, read_outlines};
pub use struct_tree::{Placement, StructureTreeNode, StructureTreeVisitor};
pub use structure::{
    AuditFinding, AuditReport, AuditScope, FROM_CATALOGUE, FROM_CONTENT, FROM_FORM,
    FROM_STRUCTURE_TREE, MatterhornAuditor, NO_STRUCTURE_TREE, Outcome, StructureVisitor,
};
