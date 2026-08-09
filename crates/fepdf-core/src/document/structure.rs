//! PDF Logical Structure Types (ISO 32000-2:2020 Clause 14.7)

use crate::{FromPdfObject, Handle, Object, PdfName};

/// PDF Structure Tree Root (Clause 14.7.2)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "14.7.2")]
pub struct StructTreeRoot {
    #[pdf_key("K")]
    pub kids: Option<Object>,
    #[pdf_key("ParentTree")]
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
    pub parent: Option<Handle<Object>>,
    #[pdf_key("K")]
    pub kids: Option<Object>,
    #[pdf_key("Alt")]
    pub alt: Option<String>,
    #[pdf_key("ActualText")]
    pub actual_text: Option<String>,
}
