//! The six operations Rule D added, as named tools.
//!
//! ARCHITECTURE §4.1: a tool is the serialised form of an `Operation`, and this is the
//! frontend whose whole job is to expose the vocabulary. It named all of it until Rule D
//! turned ten facade methods into six new operations, and those six were never given
//! tools — so `fepdf-mcp` sat at 24 of 30 while being the most complete frontend by some
//! distance.
//!
//! **They were always reachable**, through `apply_operation`, which deserialises any
//! `Operation` from a JSON string. What they lacked is a schema: a caller had to already
//! know the variant existed, and know its shape, to ask for it. That is the difference
//! between a vocabulary and a vocabulary you can look up.
//!
//! Paths and not bytes, in the two places an operation takes bytes. `InsertFrom` carries
//! a whole source document and `AddLtvInfo` a list of certificates; sending either as
//! JSON would mean base64 in a tool argument. Every other tool here names a file, so
//! these do too, and read them on the caller's behalf.

use super::page::execute_single_op;
use fepdf::{Operation, PageSelection, PdfStandard};
use schemars::JsonSchema;
use serde::Deserialize;
use std::fs;

/// Arguments for reordering several pages at once.
#[derive(Deserialize, JsonSchema)]
pub struct ReorderBatchArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Source 0-based page indices, in the order they should end up.
    pub sources: Vec<usize>,
    /// 0-based index to insert them before.
    pub target: usize,
}

/// Arguments for duplicating pages.
#[derive(Deserialize, JsonSchema)]
pub struct DuplicatePagesArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Selection of pages to duplicate (e.g. "all", "2", "1-3").
    pub pages: String,
}

/// Arguments for inserting every page of another document.
#[derive(Deserialize, JsonSchema)]
pub struct InsertFromArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Path to the PDF whose pages are inserted.
    pub source_path: String,
    /// 0-based index to insert them at.
    pub at: usize,
}

/// Arguments for embedding validation material beside a signature.
#[derive(Deserialize, JsonSchema)]
pub struct AddLtvInfoArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Paths to DER-encoded certificates to embed in the DSS.
    pub certificate_paths: Vec<String>,
}

/// Arguments for retagging a document.
#[derive(Deserialize, JsonSchema)]
pub struct RetagArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
}

/// Arguments for declaring conformance with a standard.
#[derive(Deserialize, JsonSchema)]
pub struct UpgradeArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// One of "A4", "X6", "UA2" or "ISO32000-2".
    pub standard: String,
}

/// Parses a page selection in the form the other page tools accept.
fn parse_selection(text: &str) -> PageSelection {
    match text.trim().to_lowercase().as_str() {
        "all" => PageSelection::All,
        other => {
            if let Some((start, end)) = other.split_once('-') {
                let first: usize = start.trim().parse().unwrap_or(1);
                let last: usize = end.trim().parse().unwrap_or(first);
                PageSelection::Indices((first.saturating_sub(1)..=last.saturating_sub(1)).collect())
            } else if let Ok(one) = other.trim().parse::<usize>() {
                PageSelection::Single(one.saturating_sub(1))
            } else {
                PageSelection::All
            }
        }
    }
}

/// Implementation of the reorder_pages_batch tool.
pub fn reorder_batch_impl(args: ReorderBatchArgs) -> Result<String, String> {
    let details = format!("Moved {} pages before index {}", args.sources.len(), args.target);
    let op = Operation::ReorderBatch { sources: args.sources, target: args.target };
    execute_single_op(&args.input_path, &args.output_path, op, &details)
}

/// Implementation of the duplicate_pages tool.
pub fn duplicate_pages_impl(args: DuplicatePagesArgs) -> Result<String, String> {
    let op = Operation::DuplicatePages(parse_selection(&args.pages));
    execute_single_op(&args.input_path, &args.output_path, op, "Pages duplicated successfully")
}

/// Implementation of the insert_from tool.
pub fn insert_from_impl(args: InsertFromArgs) -> Result<String, String> {
    let source = fs::read(&args.source_path)
        .map_err(|e| format!("Failed to read source PDF {}: {e}", args.source_path))?;
    let details = format!("Inserted {} at index {}", args.source_path, args.at);
    let op = Operation::InsertFrom { source, at: args.at };
    execute_single_op(&args.input_path, &args.output_path, op, &details)
}

/// Implementation of the add_ltv_info tool.
pub fn add_ltv_info_impl(args: AddLtvInfoArgs) -> Result<String, String> {
    let mut certificates = Vec::with_capacity(args.certificate_paths.len());
    for path in &args.certificate_paths {
        certificates
            .push(fs::read(path).map_err(|e| format!("Failed to read certificate {path}: {e}"))?);
    }
    let details = format!("Embedded {} certificates for long-term validation", certificates.len());
    let op = Operation::AddLtvInfo { certificates };
    execute_single_op(&args.input_path, &args.output_path, op, &details)
}

/// Implementation of the retag_document tool.
pub fn retag_impl(args: RetagArgs) -> Result<String, String> {
    execute_single_op(
        &args.input_path,
        &args.output_path,
        Operation::Retag,
        "Document structure re-tagged",
    )
}

/// Implementation of the upgrade_standard tool.
pub fn upgrade_impl(args: UpgradeArgs) -> Result<String, String> {
    // Named rather than matched with a wildcard so a standard added to `PdfStandard`
    // fails here loudly instead of silently becoming ISO 32000-2 (RR-15 Rule 5's point,
    // applied where the input is a string and the lint cannot reach).
    let standard = match args.standard.trim().to_uppercase().replace(['-', '_', ' '], "").as_str() {
        "A4" | "PDFA4" => PdfStandard::A4,
        "X6" | "PDFX6" => PdfStandard::X6,
        "UA2" | "PDFUA2" => PdfStandard::UA2,
        "ISO320002" | "PDF20" | "20" => PdfStandard::ISO32000_2,
        other => {
            return Err(format!(
                "Unknown standard {other:?}; expected one of A4, X6, UA2, ISO32000-2"
            ));
        }
    };
    let details = format!("Declared conformance with {standard:?}");
    let op = Operation::Upgrade { standard };
    execute_single_op(&args.input_path, &args.output_path, op, &details)
}
