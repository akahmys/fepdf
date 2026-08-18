//! Page-level mutation tools (rotate, reorder, remove).

use bytes::Bytes;
use fepdf::{Operation, PageSelection, PdfDocument, Quarter, RotateMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Arguments for rotating pages.
#[derive(Deserialize, JsonSchema)]
pub struct RotatePagesArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Selection of pages (e.g. "all", "1", "1-3"). Default: "all".
    pub selection: Option<String>,
    /// Angle to rotate: 90, 180, 270, -90, -180, -270.
    pub angle: i32,
    /// Whether rotation is relative to current angle (default: true).
    pub relative: Option<bool>,
}

/// Arguments for reordering pages.
#[derive(Deserialize, JsonSchema)]
pub struct ReorderPagesArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Source 0-based page index.
    pub from: usize,
    /// Destination 0-based page index.
    pub to: usize,
}

/// Arguments for removing pages.
#[derive(Deserialize, JsonSchema)]
pub struct RemovePagesArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Selection of pages to remove (e.g. "1", "1-3").
    pub pages: String,
}

/// Result report of a page-level operation.
#[derive(Serialize)]
pub struct PageOperationResult {
    /// Status code of the operation ("SUCCESS" or "FAILED").
    pub status: String,
    /// Source document path.
    pub input_path: String,
    /// Output document path.
    pub output_path: String,
    /// Detailed description of the operation outcome.
    pub details: String,
}

fn parse_selection(s: Option<&str>) -> PageSelection {
    match s.map(|v| v.trim().to_lowercase()).as_deref() {
        Some("all") | None => PageSelection::All,
        Some(other) => {
            if let Some((start, end)) = other.split_once('-') {
                let start_idx: usize = start.trim().parse().unwrap_or(1);
                let end_idx: usize = end.trim().parse().unwrap_or(1);
                let indices: Vec<usize> =
                    (start_idx.saturating_sub(1)..=end_idx.saturating_sub(1)).collect();
                PageSelection::Indices(indices)
            } else if let Ok(idx) = other.parse::<usize>() {
                PageSelection::Single(idx.saturating_sub(1))
            } else {
                PageSelection::All
            }
        }
    }
}

fn int_to_quarter(angle: i32) -> Result<Quarter, String> {
    match angle.rem_euclid(360) {
        0 => Ok(Quarter::Q0),
        90 => Ok(Quarter::Q90),
        180 => Ok(Quarter::Q180),
        270 => Ok(Quarter::Q270),
        _ => Err(format!("Angle {angle} is not a multiple of 90 degrees")),
    }
}

/// Implementation of the rotate_pages tool.
pub fn rotate_pages_impl(args: RotatePagesArgs) -> Result<String, String> {
    let quarter = int_to_quarter(args.angle)?;
    let relative = args.relative.unwrap_or(true);
    let mode = if relative { RotateMode::Relative(quarter) } else { RotateMode::Absolute(quarter) };
    let pages = parse_selection(args.selection.as_deref());

    let op = Operation::Rotate { pages, mode };
    execute_single_op(&args.input_path, &args.output_path, op, "Pages rotated successfully")
}

/// Implementation of the reorder_pages tool.
pub fn reorder_pages_impl(args: ReorderPagesArgs) -> Result<String, String> {
    let op = Operation::Reorder { from: args.from, to: args.to };
    execute_single_op(
        &args.input_path,
        &args.output_path,
        op,
        &format!("Page moved from index {} to {}", args.from, args.to),
    )
}

/// Implementation of the remove_pages tool.
pub fn remove_pages_impl(args: RemovePagesArgs) -> Result<String, String> {
    let pages = parse_selection(Some(&args.pages));
    let op = Operation::RemovePages(pages);
    execute_single_op(&args.input_path, &args.output_path, op, "Pages removed successfully")
}

pub(crate) fn execute_single_op(
    input_path: &str,
    output_path: &str,
    op: Operation,
    msg: &str,
) -> Result<String, String> {
    let data = fs::read(input_path).map_err(|e| format!("Failed to read input file: {e}"))?;
    let mut doc =
        PdfDocument::open(Bytes::from(data)).map_err(|e| format!("Failed to open PDF: {e:?}"))?;

    doc.apply(op).map_err(|e| format!("Operation failed: {e:?}"))?;

    let out = Path::new(output_path);
    doc.save_with_options(out, "2.0", &fepdf::SaveOptions::default())
        .map_err(|e| format!("Failed to save modified PDF: {e:?}"))?;

    let res = PageOperationResult {
        status: "SUCCESS".into(),
        input_path: input_path.to_string(),
        output_path: output_path.to_string(),
        details: msg.to_string(),
    };

    serde_json::to_string_pretty(&res).map_err(|e| e.to_string())
}
