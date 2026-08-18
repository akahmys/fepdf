//! Physical stream redaction tool for irreversible sanitization.

use crate::{McpError, McpResult};
use bytes::Bytes;
use fepdf::PdfDocument;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Specification of a rectangular area on a specific page to physically redact.
#[derive(Deserialize, Serialize, Debug, Clone, JsonSchema)]
pub struct RedactionTarget {
    /// Zero-based page index.
    pub page: usize,
    /// Bounding rectangle in PDF points: [x0, y0, x1, y1] (lower-left to upper-right).
    pub rect: [f32; 4],
}

/// Arguments for the physical redaction tool.
#[derive(Deserialize, JsonSchema)]
pub struct RedactDocumentArgs {
    /// Path to the input PDF document.
    pub input_path: String,
    /// Path where the redacted PDF document will be saved.
    pub output_path: String,
    /// List of target regions to physically scrub from content streams.
    pub targets: Vec<RedactionTarget>,
}

/// Summary report after applying physical redactions.
#[derive(Serialize)]
pub struct RedactionReport {
    /// Path of the source document.
    pub input_path: String,
    /// Destination path of the sanitized document.
    pub output_path: String,
    /// Number of redactions successfully scrubbed.
    pub redacted_count: usize,
    /// Distinct pages modified.
    pub affected_pages: Vec<usize>,
}

/// Implementation of the apply_redaction tool.
pub fn apply_redaction_impl(args: RedactDocumentArgs) -> Result<String, String> {
    apply_redaction_internal(args).map_err(|e| e.to_string())
}

fn apply_redaction_internal(args: RedactDocumentArgs) -> McpResult<String> {
    let data = fs::read(&args.input_path).map_err(McpError::from)?;
    let doc = PdfDocument::open(Bytes::from(data))
        .map_err(|e| McpError::Pdf(format!("Failed to open PDF: {e:?}")))?;

    // Group targets by page index
    let mut page_map: std::collections::BTreeMap<usize, Vec<[f32; 4]>> =
        std::collections::BTreeMap::new();
    for target in &args.targets {
        page_map.entry(target.page).or_default().push(target.rect);
    }

    let mut affected_pages = Vec::new();
    for (page_idx, rects) in &page_map {
        fepdf::apply_physical_redaction_to_page(doc.inner(), *page_idx, rects)
            .map_err(|e| McpError::Pdf(format!("Redaction on page {page_idx} failed: {e:?}")))?;
        affected_pages.push(*page_idx);
    }

    let out_path = Path::new(&args.output_path);
    doc.save_with_options(out_path, "2.0", &fepdf::SaveOptions::default())
        .map_err(|e| McpError::Pdf(format!("Failed to save redacted PDF: {e:?}")))?;

    let report = RedactionReport {
        input_path: args.input_path,
        output_path: args.output_path,
        redacted_count: args.targets.len(),
        affected_pages,
    };

    Ok(serde_json::to_string_pretty(&report)?)
}
