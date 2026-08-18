//! Plain text extraction tool with page range support.

use crate::{McpError, McpResult};
use bytes::Bytes;
use fepdf::PdfDocument;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;

/// Arguments for the text extraction tool.
#[derive(Deserialize, JsonSchema)]
pub struct ExtractTextArgs {
    /// Path to the PDF file.
    pub path: String,
    /// Optional page range expression (e.g. "0", "0-3", "0,2,4", or omit for all pages).
    pub page_range: Option<String>,
}

/// Extracted page text result.
#[derive(Serialize)]
pub struct PageText {
    /// Zero-based page index.
    pub page: usize,
    /// Extracted plain text for the page.
    pub text: String,
}

/// Response report for text extraction.
#[derive(Serialize)]
pub struct ExtractTextReport {
    /// Path of the source document.
    pub path: String,
    /// Total pages in the document.
    pub total_pages: usize,
    /// Extracted text per requested page.
    pub pages: Vec<PageText>,
}

/// Implementation of the extract_text tool.
pub fn extract_text_impl(args: ExtractTextArgs) -> Result<String, String> {
    extract_text_internal(args).map_err(|e| e.to_string())
}

fn extract_text_internal(args: ExtractTextArgs) -> McpResult<String> {
    let data = fs::read(&args.path).map_err(McpError::from)?;
    let doc = PdfDocument::open(Bytes::from(data))
        .map_err(|e| McpError::Pdf(format!("Failed to open PDF: {e:?}")))?;

    let total_pages = doc.page_count().unwrap_or(0);
    let target_indices = parse_page_indices(args.page_range.as_deref(), total_pages);

    let mut pages = Vec::new();
    for page_idx in target_indices {
        if page_idx < total_pages {
            let text = doc.extract_text(page_idx).unwrap_or_default();
            pages.push(PageText { page: page_idx, text });
        }
    }

    let report = ExtractTextReport { path: args.path, total_pages, pages };

    Ok(serde_json::to_string_pretty(&report)?)
}

fn parse_page_indices(range: Option<&str>, total_pages: usize) -> Vec<usize> {
    let Some(range) = range else {
        return (0..total_pages).collect();
    };

    let mut indices = Vec::new();
    for part in range.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((start, end)) = trimmed.split_once('-') {
            let s: usize = start.trim().parse().unwrap_or(0);
            let e: usize = end.trim().parse().unwrap_or(total_pages.saturating_sub(1));
            for i in s..=e {
                if i < total_pages && !indices.contains(&i) {
                    indices.push(i);
                }
            }
        } else if let Ok(idx) = trimmed.parse::<usize>()
            && idx < total_pages
            && !indices.contains(&idx)
        {
            indices.push(idx);
        }
    }
    indices
}
