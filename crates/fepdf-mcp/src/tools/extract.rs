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
    /// Why this page carries no text, when that is the reason.
    ///
    /// **A page that failed to read and a page that is genuinely blank both carry an
    /// empty `text`.** This is the only thing that tells them apart. Extraction reached
    /// `unwrap_or_default`, so a failure was reported to the client as a blank page —
    /// silently, and `discarded_results.py` does not see `unwrap_or_default`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
pub fn extract_text_impl(args: ExtractTextArgs) -> Result<String, McpError> {
    extract_text_internal(args)
}

fn extract_text_internal(args: ExtractTextArgs) -> McpResult<String> {
    let data = fs::read(&args.path).map_err(McpError::from)?;
    let doc =
        PdfDocument::open(Bytes::from(data)).map_err(|e| McpError::pdf("Failed to open PDF", e))?;

    let total_pages = doc.page_count().unwrap_or(0);
    let target_indices = parse_page_indices(args.page_range.as_deref(), total_pages);

    let mut pages = Vec::new();
    for page_idx in target_indices {
        if page_idx < total_pages {
            let (text, error) = match doc.extract_text(page_idx) {
                Ok(text) => (text, None),
                Err(why) => (String::new(), Some(format!("{why:?}"))),
            };
            pages.push(PageText { page: page_idx, text, error });
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

#[cfg(test)]
mod blank_or_broken {
    use super::PageText;

    /// **A blank page and an unreadable one must not serialise alike.**
    ///
    /// They did: extraction reached `unwrap_or_default`, so a page that failed to read
    /// reached the client as `{"page": n, "text": ""}` — the same object a genuinely
    /// blank page produces. A caller had no way to tell it had been told nothing.
    #[test]
    fn a_page_that_failed_to_read_does_not_look_blank() {
        let blank = PageText { page: 3, text: String::new(), error: None };
        let broken =
            PageText { page: 3, text: String::new(), error: Some("Filter(Unsupported)".into()) };
        let blank = serde_json::to_value(&blank).expect("serialises");
        let broken = serde_json::to_value(&broken).expect("serialises");
        assert_ne!(blank, broken, "the two are indistinguishable to a client");
        assert!(blank.get("error").is_none(), "a blank page carries no error key");
        assert_eq!(broken["error"], "Filter(Unsupported)");
    }
}
