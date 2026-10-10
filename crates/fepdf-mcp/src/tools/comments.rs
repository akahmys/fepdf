//! A page's annotations as a reviewer reads them (ROADMAP AA-2a).
//!
//! **The other half of naming an annotation.** `RemoveAnnotation`, `EditAnnotation`,
//! `ReplyToAnnotation` and `SetAnnotationState` reach a caller through `apply_operation`,
//! and each takes a page and an index (ADR-0115). This is where the index comes from.

use crate::{McpError, McpResult};
use bytes::Bytes;
use fepdf::PdfDocument;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;

/// Arguments for `list_comments`.
#[derive(Deserialize, JsonSchema)]
pub struct ListCommentsArgs {
    /// Path to the PDF file.
    pub path: String,
    /// Zero-based page index.
    pub page: usize,
}

/// What `list_comments` returns.
#[derive(Serialize)]
pub struct ListCommentsReport {
    /// Path of the document read.
    pub path: String,
    /// The page read.
    pub page: usize,
    /// Every annotation on it, in `/Annots` order. Each one's `at` is what an operation
    /// names it by.
    pub comments: Vec<fepdf::comments::Comment>,
}

/// Implementation of the `list_comments` tool.
///
/// # Errors
/// When the file will not read or open, or the page is not there.
pub fn list_comments_impl(args: ListCommentsArgs) -> McpResult<String> {
    let data = fs::read(&args.path).map_err(McpError::from)?;
    let doc =
        PdfDocument::open(Bytes::from(data)).map_err(|e| McpError::pdf("Failed to open PDF", e))?;
    let comments = doc
        .comments(args.page)
        .map_err(|e| McpError::pdf("Failed to read the page's annotations", e))?;
    let report = ListCommentsReport { path: args.path, page: args.page, comments };
    Ok(serde_json::to_string_pretty(&report)?)
}

/// Arguments for `export_fdf`.
#[derive(Deserialize, JsonSchema)]
pub struct ExportFdfArgs {
    /// Path to the PDF file.
    pub input_path: String,
    /// Where to write the FDF file.
    pub output_path: String,
}

/// Implementation of the `export_fdf` tool: every markup annotation, as FDF (12.7.8).
///
/// # Errors
/// When the file will not read or open, or the FDF will not write.
pub fn export_fdf_impl(args: ExportFdfArgs) -> McpResult<String> {
    let data = fs::read(&args.input_path).map_err(McpError::from)?;
    let doc =
        PdfDocument::open(Bytes::from(data)).map_err(|e| McpError::pdf("Failed to open PDF", e))?;
    let fdf = doc.export_fdf().map_err(|e| McpError::pdf("Failed to export the comments", e))?;
    fs::write(&args.output_path, &fdf).map_err(McpError::from)?;
    Ok(format!("Wrote {} bytes of FDF to {}", fdf.len(), args.output_path))
}

/// Implementation of the `export_xfdf` tool: the same annotations as XFDF (ISO 19444-1).
///
/// # Errors
/// When the file will not read or open, or the XFDF will not write.
pub fn export_xfdf_impl(args: ExportFdfArgs) -> McpResult<String> {
    let data = fs::read(&args.input_path).map_err(McpError::from)?;
    let doc =
        PdfDocument::open(Bytes::from(data)).map_err(|e| McpError::pdf("Failed to open PDF", e))?;
    let xfdf = doc.export_xfdf().map_err(|e| McpError::pdf("Failed to export the comments", e))?;
    fs::write(&args.output_path, &xfdf).map_err(McpError::from)?;
    Ok(format!("Wrote {} bytes of XFDF to {}", xfdf.len(), args.output_path))
}

/// Implementation of the `import_xfdf` tool (ADR-0117, ADR-0119).
///
/// # Errors
/// When a file will not read, it is not XFDF, or the result will not save.
pub fn import_xfdf_impl(args: ImportFdfArgs) -> McpResult<String> {
    let xfdf = fs::read(&args.fdf_path).map_err(McpError::from)?;
    crate::tools::operations::page::execute_single_op(
        &args.input_path,
        &args.output_path,
        fepdf::Operation::ImportXfdf { xfdf },
        "XFDF annotations imported",
    )
}

/// Arguments for `import_fdf`.
#[derive(Deserialize, JsonSchema)]
pub struct ImportFdfArgs {
    /// Path to the PDF file.
    pub input_path: String,
    /// Path to the FDF file whose annotations are imported.
    pub fdf_path: String,
    /// Where to write the PDF with them.
    pub output_path: String,
}

/// Implementation of the `import_fdf` tool (ADR-0117).
///
/// **A path rather than `apply_operation`'s JSON**: `ImportFdf` carries the file's bytes,
/// and a caller should not have to write a file out as a JSON array of numbers.
///
/// # Errors
/// When a file will not read, the FDF is not one, or the result will not save.
pub fn import_fdf_impl(args: ImportFdfArgs) -> McpResult<String> {
    let fdf = fs::read(&args.fdf_path).map_err(McpError::from)?;
    crate::tools::operations::page::execute_single_op(
        &args.input_path,
        &args.output_path,
        fepdf::Operation::ImportFdf { fdf },
        "FDF annotations imported",
    )
}
