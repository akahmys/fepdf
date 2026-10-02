//! Comparing two documents, page by page (ROADMAP W-18).

use crate::McpError;
use schemars::JsonSchema;
use serde::Deserialize;

/// Arguments for `compare_documents`.
#[derive(Deserialize, JsonSchema)]
pub struct CompareArgs {
    /// Path to the first PDF file.
    pub path_a: String,
    /// Path to the second PDF file.
    pub path_b: String,
    /// The resolution pages are compared at for where they look different; 72 when not
    /// given. Ignored in a build without rendering, which compares the text alone.
    pub dpi: Option<f64>,
}

/// Implementation of the compare_documents tool.
pub fn compare_documents_impl(args: CompareArgs) -> Result<String, McpError> {
    let open = |path: &str| {
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        fepdf::PdfDocument::open(bytes.into())
            .map_err(|e| McpError::pdf(format!("opening {path}"), e))
    };
    let (a, b) = (open(&args.path_a)?, open(&args.path_b)?);
    #[cfg(feature = "render")]
    let comparison = fepdf::compare::compare(&a, &b, args.dpi.unwrap_or(72.0));
    #[cfg(not(feature = "render"))]
    let comparison = fepdf::compare::compare_text(&a, &b);
    let comparison = comparison.map_err(|e| McpError::pdf("comparing the documents", e))?;
    serde_json::to_string_pretty(&comparison).map_err(McpError::from)
}
