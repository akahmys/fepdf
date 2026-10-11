//! Pages made from pictures (ROADMAP AA-5, ADR-0123), as a tool.
//!
//! **Paths, not bytes**, as `insert_from` takes a path: a picture sent as JSON would be
//! base64 in a tool argument.

use crate::McpError;
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::Path;

/// Arguments for `images_to_pdf`.
#[derive(Deserialize, JsonSchema)]
pub struct ImagesToPdfArgs {
    /// The pictures, JPEG, PNG or TIFF, in page order. Every page of a TIFF is a page.
    pub image_paths: Vec<String>,
    /// Where to write the PDF.
    pub output_path: String,
    /// A PDF to put the pages into; a new document where absent.
    pub input_path: Option<String>,
    /// The 0-based page index to insert them at; the end where absent.
    pub at: Option<usize>,
    /// A sheet every page is, the picture fitted inside it: A3, A4, A5, B4, B5, Letter,
    /// Legal or Tabloid. Each page is its picture's size at its resolution where absent.
    pub sheet: Option<String>,
}

/// Implementation of the `images_to_pdf` tool.
///
/// # Errors
/// When a picture or the PDF cannot be read, a sheet has an unknown name, or a picture is
/// not one the engine reads.
pub fn images_to_pdf_impl(args: ImagesToPdfArgs) -> Result<String, McpError> {
    let sheet = match args.sheet.as_deref() {
        Some(name) => {
            let (w, h) = fepdf::PageResize::sheet(name).ok_or_else(|| {
                let known: Vec<&str> = fepdf::PageResize::SHEETS.iter().map(|(n, _)| *n).collect();
                format!("no sheet is called {name:?}; the sheets are {}", known.join(", "))
            })?;
            #[allow(clippy::cast_possible_truncation)] // whole points, well inside f32
            let sheet = [w as f32, h as f32];
            Some(sheet)
        }
        None => None,
    };
    let mut images = Vec::with_capacity(args.image_paths.len());
    for path in &args.image_paths {
        images.push(std::fs::read(path).map_err(|e| format!("Failed to read {path}: {e}"))?);
    }
    let count = images.len();
    let Some(input) = args.input_path.as_deref() else {
        let doc = fepdf::PdfDocument::from_images(images, sheet)
            .map_err(|e| McpError::pdf("Failed to make pages of the pictures", e))?;
        doc.save_with_options(Path::new(&args.output_path), "2.0", &fepdf::SaveOptions::default())
            .map_err(|e| McpError::pdf("Failed to save the PDF", e))?;
        return Ok(format!("{count} picture(s) made into a PDF at {}", args.output_path));
    };
    let at = match args.at {
        Some(at) => at,
        None => {
            let data = std::fs::read(input).map_err(|e| format!("Failed to read {input}: {e}"))?;
            fepdf::PdfDocument::open(data.into())
                .and_then(|d| d.page_count())
                .map_err(|e| McpError::pdf("Failed to open PDF", e))?
        }
    };
    let op = fepdf::Operation::InsertImages { images, at, sheet };
    let details = format!("{count} picture(s) made into pages at index {at}");
    super::operations::page::execute_single_op(input, &args.output_path, op, &details)
}
