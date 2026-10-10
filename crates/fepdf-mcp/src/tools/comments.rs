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
