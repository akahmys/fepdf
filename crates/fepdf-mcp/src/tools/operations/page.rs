//! Page-level mutation tools (rotate, reorder, remove).

use crate::McpError;
use bytes::Bytes;
use fepdf::{Operation, PdfDocument, Quarter, RotateMode};
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
    /// Selection of pages, **counting from 1**: "all", "1", "1-3". Default: "all".
    ///
    /// One-based, where every `page` field on this surface is zero-based. The two
    /// bases are real and only this one used to say nothing, so `page_range: "1"` in
    /// `extract_text` and `pages: "1"` here name different pages.
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
    /// Pages to remove, **counting from 1**: "1", "1-3".
    ///
    /// One-based, like every `pages`/`selection` string and unlike every `page`
    /// integer on this surface.
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

fn int_to_quarter(angle: i32) -> Result<Quarter, McpError> {
    match angle.rem_euclid(360) {
        0 => Ok(Quarter::Q0),
        90 => Ok(Quarter::Q90),
        180 => Ok(Quarter::Q180),
        270 => Ok(Quarter::Q270),
        _ => Err(format!("Angle {angle} is not a multiple of 90 degrees").into()),
    }
}

/// Implementation of the rotate_pages tool.
pub fn rotate_pages_impl(args: RotatePagesArgs) -> Result<String, McpError> {
    let quarter = int_to_quarter(args.angle)?;
    let relative = args.relative.unwrap_or(true);
    let mode = if relative { RotateMode::Relative(quarter) } else { RotateMode::Absolute(quarter) };
    let pages = super::parse_selection(args.selection.as_deref())?;

    let op = Operation::Rotate { pages, mode };
    execute_single_op(&args.input_path, &args.output_path, op, "Pages rotated successfully")
}

/// Implementation of the reorder_pages tool.
pub fn reorder_pages_impl(args: ReorderPagesArgs) -> Result<String, McpError> {
    let op = Operation::Reorder { from: args.from, to: args.to };
    execute_single_op(
        &args.input_path,
        &args.output_path,
        op,
        &format!("Page moved from index {} to {}", args.from, args.to),
    )
}

/// Implementation of the remove_pages tool.
pub fn remove_pages_impl(args: RemovePagesArgs) -> Result<String, McpError> {
    let pages = super::parse_selection(Some(&args.pages))?;
    let op = Operation::RemovePages(pages);
    execute_single_op(&args.input_path, &args.output_path, op, "Pages removed successfully")
}

pub(crate) fn execute_single_op(
    input_path: &str,
    output_path: &str,
    op: Operation,
    msg: &str,
) -> Result<String, McpError> {
    let data = fs::read(input_path).map_err(|e| format!("Failed to read input file: {e}"))?;
    let doc =
        PdfDocument::open(Bytes::from(data)).map_err(|e| McpError::pdf("Failed to open PDF", e))?;

    let handle = super::apply_and_calculate(doc, op)?;

    let out = Path::new(output_path);
    handle
        .with(|d| d.save_with_options(out, "2.0", &fepdf::SaveOptions::default()))
        .map_err(|e| McpError::pdf("Failed to save modified PDF", e))?;

    let res = PageOperationResult {
        status: "SUCCESS".into(),
        input_path: input_path.to_string(),
        output_path: output_path.to_string(),
        details: msg.to_string(),
    };

    serde_json::to_string_pretty(&res).map_err(McpError::from)
}

/// Arguments for `crop_pages`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CropPagesArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Selection of pages, **counting from 1**: "all", "1", "1-3". Default: "all".
    pub pages: Option<String>,
    /// The left edge of what to keep, in points from the page's left edge.
    pub left: f64,
    /// The bottom edge of what to keep, in points from the page's foot.
    pub bottom: f64,
    /// The right edge of what to keep.
    pub right: f64,
    /// The top edge of what to keep.
    pub top: f64,
    /// Whether the content outside is taken out of the file. Left out or false, it stays
    /// and `/CropBox` hides it, which is a view rather than a cut: any reader can move
    /// the box back and see what it hid.
    pub remove_outside: Option<bool>,
}

/// Implementation of the crop_pages tool.
pub fn crop_pages_impl(args: CropPagesArgs) -> Result<String, McpError> {
    let pages = super::parse_selection(args.pages.as_deref())?;
    let outside = if args.remove_outside.unwrap_or(false) {
        fepdf::WhatFallsOutside::Goes
    } else {
        fepdf::WhatFallsOutside::Stays
    };
    let op = Operation::CropPages(
        pages,
        fepdf::CropRegion { keep: (args.left, args.bottom, args.right, args.top), outside },
    );
    execute_single_op(&args.input_path, &args.output_path, op, "Pages cropped")
}

/// Arguments for `split_page`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SplitPageArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Which page to cut up, counting from zero.
    pub page: usize,
    /// How many parts across. With `rows`, the page is cut into an even grid.
    pub columns: Option<usize>,
    /// How many parts down.
    pub rows: Option<usize>,
    /// Regions named outright instead of a grid, each as [left, bottom, right, top] in
    /// points from the page's bottom-left corner, in the order the pages are to come out.
    pub regions: Option<Vec<[f64; 4]>>,
}

/// Implementation of the split_page tool.
pub fn split_page_impl(args: SplitPageArgs) -> Result<String, McpError> {
    let into = match args.regions {
        Some(named) => fepdf::PageDivision::Regions(
            named.into_iter().map(|r| (r[0], r[1], r[2], r[3])).collect(),
        ),
        None => fepdf::PageDivision::Grid {
            columns: args.columns.unwrap_or(1),
            rows: args.rows.unwrap_or(1),
        },
    };
    let op = Operation::SplitPage { page: args.page, into };
    execute_single_op(&args.input_path, &args.output_path, op, "Page split")
}

/// Arguments for `combine_pages`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CombinePagesArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Selection of pages, **counting from 1**: "all", "1", "1-3". Default: "all".
    pub pages: Option<String>,
    /// How many cells across.
    pub columns: usize,
    /// How many cells down.
    pub rows: usize,
    /// The width of the sheet they go onto, in points. Left out, the first page's own.
    pub sheet_width: Option<f64>,
    /// The height of that sheet.
    pub sheet_height: Option<f64>,
}

/// Implementation of the combine_pages tool.
pub fn combine_pages_impl(args: CombinePagesArgs) -> Result<String, McpError> {
    let pages = super::parse_selection(args.pages.as_deref())?;
    let sheet = match (args.sheet_width, args.sheet_height) {
        (Some(width), Some(height)) => Some((width, height)),
        _ => None,
    };
    let op = Operation::CombinePages(
        pages,
        fepdf::PageArrangement { sheet, columns: args.columns, rows: args.rows },
    );
    execute_single_op(&args.input_path, &args.output_path, op, "Pages combined")
}

/// Arguments for `set_tab_order`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetTabOrderArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Selection of pages, **counting from 1**: "all", "1", "1-3". Default: "all".
    pub pages: Option<String>,
    /// One of `row`, `column`, `structure`, `annotations` or `widgets`.
    pub order: String,
}

/// Implementation of the set_tab_order tool.
pub fn set_tab_order_impl(args: SetTabOrderArgs) -> Result<String, McpError> {
    let pages = super::parse_selection(args.pages.as_deref())?;
    let order = match args.order.as_str() {
        "row" => fepdf::TabOrder::Row,
        "column" => fepdf::TabOrder::Column,
        "structure" => fepdf::TabOrder::Structure,
        "annotations" => fepdf::TabOrder::Annotations,
        "widgets" => fepdf::TabOrder::Widgets,
        other => return Err(format!("no tab order is called {other:?}").into()),
    };
    let op = Operation::SetTabOrder { pages, order };
    execute_single_op(&args.input_path, &args.output_path, op, "Tab order set")
}

/// Arguments for `resize_pages`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ResizePagesArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Selection of pages, **counting from 1**: "all", "1", "1-3". Default: "all".
    pub pages: Option<String>,
    /// The new sheet's width in points. With `sheet_height`; both left out keep the sheet
    /// each page is already on.
    pub sheet_width: Option<f64>,
    /// The new sheet's height in points.
    pub sheet_height: Option<f64>,
    /// How what is drawn is resized: "keep", "fit" (all of it on the sheet), "fill" (the
    /// sheet covered), or a factor such as "0.9". Default: "fit".
    pub scale: Option<String>,
    /// How far right of the sheet's middle the drawing sits, in points. Default: 0.
    pub offset_right: Option<f64>,
    /// How far up from the sheet's middle the drawing sits, in points. Default: 0.
    pub offset_up: Option<f64>,
}

/// Implementation of the resize_pages tool.
pub fn resize_pages_impl(args: ResizePagesArgs) -> Result<String, McpError> {
    let pages = super::parse_selection(args.pages.as_deref())?;
    let sheet = match (args.sheet_width, args.sheet_height) {
        (Some(w), Some(h)) => Some((w, h)),
        (None, None) => None,
        _ => {
            return Err(McpError::Other(
                "give sheet_width and sheet_height together, or neither".to_string(),
            ));
        }
    };
    let scale = match args.scale.as_deref().map_or("fit", str::trim) {
        "keep" => fepdf::ContentScale::Keep,
        "fit" => fepdf::ContentScale::Fit,
        "fill" => fepdf::ContentScale::Fill,
        factor => fepdf::ContentScale::By(factor.parse().map_err(|_| {
            McpError::Other(format!("scale is keep, fit, fill or a factor, not {factor:?}"))
        })?),
    };
    let offset = (args.offset_right.unwrap_or(0.0), args.offset_up.unwrap_or(0.0));
    let op = Operation::ResizePages(pages, fepdf::PageResize { sheet, scale, offset });
    execute_single_op(&args.input_path, &args.output_path, op, "Pages resized")
}

/// Arguments for `remove_outside`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RemoveOutsideArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Which page, counting from zero.
    pub page: usize,
    /// The left edge of what to keep, in points from the page's left edge.
    pub left: f64,
    /// The bottom edge of what to keep, in points from the page's foot.
    pub bottom: f64,
    /// The right edge of what to keep.
    pub right: f64,
    /// The top edge of what to keep.
    pub top: f64,
}

/// Implementation of the remove_outside tool.
pub fn remove_outside_impl(args: RemoveOutsideArgs) -> Result<String, McpError> {
    let op = Operation::RemoveOutside {
        page: args.page,
        keep: (args.left, args.bottom, args.right, args.top),
    };
    execute_single_op(&args.input_path, &args.output_path, op, "Text outside removed")
}
