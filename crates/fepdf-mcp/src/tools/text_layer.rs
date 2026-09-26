//! The page an OCR engine reads, and the text it hands back (ROADMAP W-O1, ADR-0086).
//!
//! **No OCR engine is built; a window is opened for one.** `page_for_ocr` hands out the
//! page as a picture with what is needed to read it back — its size, the transform from
//! the picture's pixels to the page's points, and whatever text the page already has —
//! and `add_text_layer` takes the words the engine found, in either space, and lays them
//! over the page invisibly.

use crate::tools::operations::page::execute_single_op;
use fepdf::{Operation, TextLayerItem};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One piece of text an OCR engine read.
#[derive(Deserialize, JsonSchema)]
pub struct ReadText {
    /// What it says, on one line.
    pub text: String,
    /// The box it was read from: left, bottom, right, top in page points — or, when
    /// `pixel_to_page` is given, left, top, right, bottom in the picture's pixels.
    pub rect: [f64; 4],
}

/// Arguments for `add_text_layer`.
#[derive(Deserialize, JsonSchema)]
pub struct AddTextLayerArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Zero-based page index.
    pub page: usize,
    /// What was read, and where.
    pub items: Vec<ReadText>,
    /// The `pixel_to_page` `page_for_ocr` answered, when the boxes are in its pixels.
    pub pixel_to_page: Option<[f64; 6]>,
}

/// Implementation of the add_text_layer tool.
pub fn add_text_layer_impl(args: AddTextLayerArgs) -> Result<String, String> {
    let items = args
        .items
        .into_iter()
        .map(|read| TextLayerItem {
            rect: args
                .pixel_to_page
                .map_or(read.rect, |to| fepdf::pixel_box_on_page(read.rect, to)),
            text: read.text,
        })
        .collect();
    let op = Operation::AddTextLayer { page: args.page, items };
    execute_single_op(&args.input_path, &args.output_path, op, "Text layer added")
}

/// Arguments for `page_for_ocr`.
#[derive(Deserialize, JsonSchema)]
pub struct PageForOcrArgs {
    /// Path to the PDF file.
    pub path: String,
    /// Zero-based page index.
    pub page: usize,
    /// Where to write the picture, as a PNG.
    pub image_path: String,
    /// Its resolution; 300 when not given, which is what OCR engines are tuned for.
    pub dpi: Option<f64>,
}

/// Text the page already has, so a caller can tell what an engine found from what was
/// there.
#[derive(Serialize)]
struct Existing {
    text: String,
    /// Where it starts, on its baseline, in page points.
    x: f64,
    y: f64,
    width: f64,
    size: f64,
}

/// What `page_for_ocr` answers.
#[derive(Serialize)]
struct PageForOcr {
    image_path: String,
    width_px: u32,
    height_px: u32,
    dpi: f64,
    /// The transform from the picture's pixels — origin top left — to the page's points,
    /// as the six numbers `a b c d e f` of a PDF matrix.
    pixel_to_page: [f64; 6],
    existing_text: Vec<Existing>,
}

/// Implementation of the page_for_ocr tool. It rasterises, so it needs `render`.
#[cfg(feature = "render")]
pub fn page_for_ocr_impl(args: PageForOcrArgs) -> Result<String, String> {
    let bytes = std::fs::read(&args.path).map_err(|e| format!("{}: {e}", args.path))?;
    let doc = fepdf::PdfDocument::open(bytes.into()).map_err(|e| e.to_string())?;
    let dpi = args.dpi.unwrap_or(300.0);
    let (to_pixels, width_px, height_px) =
        doc.page_to_pixels(args.page, dpi).map_err(|e| e.to_string())?;
    let path = std::path::Path::new(&args.image_path);
    doc.render_page_to_file_at(args.page, path, dpi, fepdf::Rasteriser::Cpu)
        .map_err(|e| e.to_string())?;
    let existing_text = doc
        .extract_spans(args.page)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|s| Existing { text: s.text, x: s.x, y: s.y, width: s.width, size: s.font_size })
        .collect();
    let answer = PageForOcr {
        image_path: args.image_path,
        width_px,
        height_px,
        dpi,
        pixel_to_page: to_pixels.inverse().as_coeffs(),
        existing_text,
    };
    serde_json::to_string_pretty(&answer).map_err(|e| e.to_string())
}
