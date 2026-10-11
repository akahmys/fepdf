//! Documents made from what is not a PDF, as tools: pictures (ROADMAP AA-5, ADR-0123)
//! and plain text (AA-6, ADR-0124).
//!
//! **Paths, not bytes**, as `insert_from` takes a path: a picture sent as JSON would be
//! base64 in a tool argument. Text is the one thing a caller may hand over as it is.

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
    let sheet = sheet_named(args.sheet.as_deref())?;
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

/// The sheet `name` from `PageResize::SHEETS`, in points, or a refusal listing them.
fn sheet_named(name: Option<&str>) -> Result<Option<[f32; 2]>, McpError> {
    let Some(name) = name else { return Ok(None) };
    let (w, h) = fepdf::PageResize::sheet(name).ok_or_else(|| {
        let known: Vec<&str> = fepdf::PageResize::SHEETS.iter().map(|(n, _)| *n).collect();
        format!("no sheet is called {name:?}; the sheets are {}", known.join(", "))
    })?;
    #[allow(clippy::cast_possible_truncation)] // whole points, well inside f32
    let sheet = [w as f32, h as f32];
    Ok(Some(sheet))
}

/// Arguments for `text_to_pdf`.
#[derive(Deserialize, JsonSchema)]
pub struct TextToPdfArgs {
    /// The text, as it is; or leave it empty and name a file in `text_path`.
    #[serde(default)]
    pub text: String,
    /// A plain text file, UTF-8 or UTF-16 with a byte order mark, in place of `text`.
    pub text_path: Option<String>,
    /// Where to write the PDF.
    pub output_path: String,
    /// A PDF to put the pages into; a new document where absent.
    pub input_path: Option<String>,
    /// The 0-based page index to insert them at; the end where absent.
    pub at: Option<usize>,
    /// The sheet: A3, A4, A5, B4, B5, Letter, Legal or Tabloid. A4 where absent.
    pub sheet: Option<String>,
    /// The size the text is set at, in points. 10.5 where absent.
    pub font_size: Option<f32>,
    /// The language the text is in, BCP 47: "ja", "en-GB".
    pub lang: Option<String>,
}

/// Implementation of the `text_to_pdf` tool.
///
/// # Errors
/// When the text or the PDF cannot be read, a sheet has an unknown name, or the text
/// cannot be set.
pub fn text_to_pdf_impl(args: TextToPdfArgs) -> Result<String, McpError> {
    let text = match args.text_path.as_deref() {
        Some(path) => {
            let bytes = std::fs::read(path).map_err(|e| format!("Failed to read {path}: {e}"))?;
            fepdf::PdfDocument::plain_text(&bytes)
                .map_err(|e| McpError::pdf("The file is not plain text this reads", e))?
        }
        None => args.text,
    };
    let mut setting = fepdf::TextSetting { lang: args.lang, ..Default::default() };
    if let Some(sheet) = sheet_named(args.sheet.as_deref())? {
        setting.sheet = sheet;
    }
    if let Some(size) = args.font_size {
        setting.font_size = size;
    }
    let Some(input) = args.input_path.as_deref() else {
        let doc = fepdf::PdfDocument::from_text(&text, setting)
            .map_err(|e| McpError::pdf("Failed to set the text", e))?;
        doc.save_with_options(Path::new(&args.output_path), "2.0", &fepdf::SaveOptions::default())
            .map_err(|e| McpError::pdf("Failed to save the PDF", e))?;
        return Ok(format!("The text was set as a PDF at {}", args.output_path));
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
    let op = fepdf::Operation::InsertText { text, at, setting };
    super::operations::page::execute_single_op(
        input,
        &args.output_path,
        op,
        &format!("Text set as pages at index {at}"),
    )
}
