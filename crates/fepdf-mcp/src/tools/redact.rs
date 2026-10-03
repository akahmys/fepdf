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
    /// The colour the regions are filled with, as a `/Redact` annotation's `/IC`: `[]`
    /// for no fill, `[gray]`, `[r, g, b]` or `[c, m, y, k]`, each from 0 to 1. Omitted,
    /// they are filled black and the document records that the engine chose it.
    #[serde(default)]
    pub fill: Option<Vec<f64>>,
}

/// Summary report after applying physical redactions.
#[derive(Serialize)]
pub struct RedactionReport {
    /// Path of the source document.
    pub input_path: String,
    /// Destination path of the sanitized document.
    pub output_path: String,
    /// Number of glyphs removed.
    ///
    /// **What was removed, not what was asked for.** This read `args.targets.len()` until
    /// 2026-09-06, so a rectangle covering nothing was reported to the caller as a
    /// redaction that had happened — and the caller is an agent. It is read before the
    /// redaction is applied, by the same test that applies it.
    pub redacted_count: usize,
    /// Pages where a glyph was removed. Every page named is filled, these or not.
    pub affected_pages: Vec<usize>,
}

/// Implementation of the apply_redaction tool.
pub fn apply_redaction_impl(args: RedactDocumentArgs) -> Result<String, McpError> {
    apply_redaction_internal(args)
}

fn apply_redaction_internal(args: RedactDocumentArgs) -> McpResult<String> {
    let data = fs::read(&args.input_path).map_err(McpError::from)?;
    let mut doc =
        PdfDocument::open(Bytes::from(data)).map_err(|e| McpError::pdf("Failed to open PDF", e))?;

    // Group targets by page index
    let mut page_map: std::collections::BTreeMap<usize, Vec<[f32; 4]>> =
        std::collections::BTreeMap::new();
    for target in &args.targets {
        page_map.entry(target.page).or_default().push(target.rect);
    }

    let mut affected_pages = Vec::new();
    let mut scrubbed = 0;
    for (page_idx, rects) in &page_map {
        let redaction = fepdf::Redaction {
            page: *page_idx,
            fill: args.fill.clone(),
            regions: rects
                .iter()
                .map(|r| (f64::from(r[0]), f64::from(r[1]), f64::from(r[2]), f64::from(r[3])))
                .collect(),
        };
        let removed = doc
            .what_redaction_removes(&redaction)
            .map_err(|e| McpError::pdf(format!("redacting page {page_idx}"), e))?
            .glyphs
            .len();
        doc.apply(fepdf::Operation::Redact(redaction))
            .map_err(|e| McpError::pdf(format!("redacting page {page_idx}"), e))?;
        scrubbed += removed;
        if removed > 0 {
            affected_pages.push(*page_idx);
        }
    }

    let out_path = Path::new(&args.output_path);
    doc.save_with_options(out_path, "2.0", &fepdf::SaveOptions::default())
        .map_err(|e| McpError::pdf("Failed to save redacted PDF", e))?;

    let report = RedactionReport {
        input_path: args.input_path,
        output_path: args.output_path,
        redacted_count: scrubbed,
        affected_pages,
    };

    Ok(serde_json::to_string_pretty(&report)?)
}
