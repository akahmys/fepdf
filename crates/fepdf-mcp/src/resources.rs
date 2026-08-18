use bytes::Bytes;
use fepdf::PdfDocument;
use std::fs;

/// Reads the PDF/UA-2 logical structure tree of a local PDF document as JSON.
pub fn read_struct_tree_resource(path: &str) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| format!("Failed to read file '{path}': {e}"))?;
    let doc = PdfDocument::open(Bytes::from(data))
        .map_err(|e| format!("Failed to open PDF '{path}': {e:?}"))?;

    let tree = doc.extract_struct_tree();

    serde_json::to_string_pretty(&tree).map_err(|e| e.to_string())
}

/// Reads document metadata (XMP and Info) as JSON.
pub fn read_metadata_resource(path: &str) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| format!("Failed to read file '{path}': {e}"))?;
    let doc = PdfDocument::open(Bytes::from(data))
        .map_err(|e| format!("Failed to open PDF '{path}': {e:?}"))?;

    let summary = doc.get_summary().map_err(|e| format!("Failed to inspect document: {e:?}"))?;

    serde_json::to_string_pretty(&summary.metadata).map_err(|e| e.to_string())
}

/// Reads the compliance audit report of a local PDF document as JSON.
pub fn read_audit_resource(path: &str) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| format!("Failed to read file '{path}': {e}"))?;
    let doc = PdfDocument::open(Bytes::from(data))
        .map_err(|e| format!("Failed to open PDF '{path}': {e:?}"))?;

    let summary = doc.get_summary().map_err(|e| format!("Failed to inspect document: {e:?}"))?;

    serde_json::to_string_pretty(&summary.compliance).map_err(|e| e.to_string())
}
