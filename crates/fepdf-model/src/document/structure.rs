//! PDF Logical Structure Types (ISO 32000-2:2020 Clause 14.7)

use crate::{FromPdfObject, Handle, Object, PdfName};

/// PDF Structure Tree Root (Clause 14.7.2)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "14.7.2")]
pub struct StructTreeRoot {
    #[pdf_key("K")]
    /// `/K`: the root's child structure elements.
    pub kids: Option<Object>,
    #[pdf_key("ParentTree")]
    /// `/ParentTree`: number tree linking content back to structure.
    pub parent_tree: Option<Handle<Object>>,
}

/// PDF Structure Element (Clause 14.7.3)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "14.7.3")]
pub struct StructElement {
    /// The structure type (/S key). Optional here because malformed real-world PDFs
    /// may omit /S or /P — the auditor skips such elements gracefully.
    #[pdf_key("S")]
    pub subtype: Option<Handle<PdfName>>,
    #[pdf_key("P")]
    /// `/P`: the enclosing structure element.
    pub parent: Option<Handle<Object>>,
    #[pdf_key("K")]
    /// `/K`: this element's children.
    pub kids: Option<Object>,
    #[pdf_key("Alt")]
    /// `/Alt`: alternative description for assistive technology.
    pub alt: Option<String>,
    #[pdf_key("ActualText")]
    /// `/ActualText`: the text this element actually represents.
    pub actual_text: Option<String>,
    #[pdf_key("E")]
    /// `/E`: the expanded form of an abbreviation or acronym.
    ///
    /// Read beside `/Alt` and `/ActualText` because 14.9.2.2 gives the three of them one
    /// natural language — the element's own `/Lang`, or the nearest ancestor's, or the
    /// catalogue's — and Matterhorn 11-002 fails a document where that language cannot
    /// be determined for any of them.
    pub expanded: Option<String>,
    #[pdf_key("Lang")]
    /// `/Lang`: the natural language of this element's content (14.9.2.2), as a BCP 47
    /// tag, overriding the catalogue's `/Lang` for this subtree.
    pub lang: Option<String>,
}
