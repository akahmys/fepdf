use crate::{McpError, McpResult};
use bytes::Bytes;
use fepdf_sdk::PdfDocument;
use schemars::JsonSchema;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

/// Arguments for the rendering tool.
#[derive(Deserialize, JsonSchema)]
pub struct RenderArgs {
    /// Path to the PDF file to render.
    pub path: String,
    /// The page number to render (indexed from 0).
    pub page_number: usize,
}

/// Implementation of the page rendering logic for the MCP tool.
pub async fn render_page_impl(args: RenderArgs) -> Result<String, String> {
    render_page_internal(args).map_err(|e| e.to_string())
}

fn render_page_internal(args: RenderArgs) -> McpResult<String> {
    let data = fs::read(&args.path).map_err(McpError::from)?;
    let doc = PdfDocument::open(Bytes::from(data))
        .map_err(|e: fepdf_sdk::PdfError| McpError::Pdf(e.to_string()))?;

    let output_dir = PathBuf::from("artifacts/screenshots");
    if !output_dir.exists() {
        fs::create_dir_all(&output_dir).map_err(McpError::from)?;
    }

    let filename = format!(
        "render_{}_{}.png",
        PathBuf::from(&args.path).file_stem().unwrap_or_default().to_string_lossy(),
        args.page_number
    );
    let output_path = output_dir.join(filename);

    doc.render_page_to_file(args.page_number, &output_path)
        .map_err(|e: fepdf_sdk::PdfError| McpError::Pdf(e.to_string()))?;

    Ok(output_path.to_string_lossy().to_string())
}
