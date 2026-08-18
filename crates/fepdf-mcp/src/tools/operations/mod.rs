//! PDF document operation tools mapping to fepdf canonical operations.

pub mod advanced;
pub mod decoration;
pub mod metadata;
pub mod page;
pub mod struct_elem;

use bytes::Bytes;
use fepdf::{Operation, PdfDocument};
use schemars::JsonSchema;
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Arguments for applying any raw Operation in JSON format.
#[derive(Deserialize, JsonSchema)]
pub struct ApplyOperationArgs {
    /// Path to the input PDF file.
    pub input_path: String,
    /// Path to save the modified output PDF file.
    pub output_path: String,
    /// The serialized Operation object in JSON.
    pub operation_json: String,
}

/// Applies a generic raw Operation JSON to mutate a PDF document.
pub fn apply_operation_impl(args: ApplyOperationArgs) -> Result<String, String> {
    let op: Operation = serde_json::from_str(&args.operation_json)
        .map_err(|e| format!("Failed to parse Operation JSON: {e}"))?;

    let data = fs::read(&args.input_path)
        .map_err(|e| format!("Failed to read input file '{}': {e}", args.input_path))?;
    let mut doc =
        PdfDocument::open(Bytes::from(data)).map_err(|e| format!("Failed to open PDF: {e:?}"))?;

    doc.apply(op).map_err(|e| format!("Operation application failed: {e:?}"))?;

    let out = Path::new(&args.output_path);
    doc.save_with_options(out, "2.0", &fepdf::SaveOptions::default())
        .map_err(|e| format!("Failed to save output PDF '{}': {e:?}", args.output_path))?;

    Ok(serde_json::json!({
        "status": "SUCCESS",
        "input_path": args.input_path,
        "output_path": args.output_path,
        "message": "Operation applied successfully"
    })
    .to_string())
}
