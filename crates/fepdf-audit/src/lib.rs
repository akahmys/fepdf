//! Judging a document against the Matterhorn Protocol (ISO 14289): which failure
//! conditions it breaks, which it meets, and which are left to a person.
//!
//! **It changes nothing.** It reads what `fepdf-doc` reads — content, the structure tree,
//! the parent tree — and stands above it, so an operation cannot reach into an audit
//! (Rule E, [ADR-0107](../../../docs/adr/0107-an-operation-does-not-reach-into-an-audit.md)).

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
/// Which marked content lies in a `<Formula>` (17-003).
pub mod formula_marks;
/// Which glyph each character code of a font selects, as ISO 32000-1 says.
pub mod glyph_map;
/// What the codes a font's text shows select: 31-011 and 31-030.
pub mod glyph_select;
/// A glyph's width in the font dictionary and in the program (31-016).
pub mod glyph_widths;
/// The Matterhorn Protocol's own text, for the conditions it leaves to a person.
pub mod matterhorn;
/// Whether the language of the text in page content can be determined (11-001).
pub mod page_languages;
/// PDF logical structure auditor and visitor.
pub mod structure;
/// Whether a TrueType font's rendered codes reach a glyph through its cmap (31-018).
pub mod truetype_lookup;
/// Whether each character code shown maps to Unicode (10-001).
pub mod unicode_map;

pub use matterhorn::{LeftToAPerson, MARKED_H, left_to_a_person};
pub use structure::{
    AuditFinding, AuditReport, AuditScope, FROM_CATALOGUE, FROM_CONTENT, FROM_FORM,
    FROM_STRUCTURE_TREE, MatterhornAuditor, NO_STRUCTURE_TREE, Outcome, StructureVisitor,
};
