//! Reading and changing the objects a page draws with `Do` — its images and form XObjects
//! (ROADMAP W-E5).
//!
//! **An object is named by its place among the page's `Do` operators**, which is the
//! number `list_objects` gives and `edit_object` takes; naming one without the listing
//! would be guessing at a number, which is why the two are served together.

use crate::tools::operations::page::execute_single_op;
use crate::{McpError, McpResult};
use bytes::Bytes;
use fepdf::xobject::objects_of_page;
use fepdf::{Operation, PdfDocument, XObjectEdit};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;

/// Arguments for `list_objects`.
#[derive(Deserialize, JsonSchema)]
pub struct ListObjectsArgs {
    /// Path to the PDF file.
    pub path: String,
    /// Zero-based page index.
    pub page: usize,
}

/// One object a page draws, as a caller choosing between them sees it.
#[derive(Serialize)]
pub struct ListedObject {
    /// Its place among the page's `Do` operators — the number `edit_object` takes.
    pub object: usize,
    /// The resource name the page draws it by.
    pub name: String,
    /// `image` or `form`.
    pub kind: &'static str,
    /// Its four corners on the page, in points from the bottom-left, going round it.
    pub corners: [(f64, f64); 4],
}

/// Implementation of the list_objects tool.
pub fn list_objects_impl(args: ListObjectsArgs) -> Result<String, String> {
    list_objects_internal(args).map_err(|e| e.to_string())
}

fn list_objects_internal(args: ListObjectsArgs) -> McpResult<String> {
    let data = fs::read(&args.path).map_err(McpError::from)?;
    let doc = PdfDocument::open(Bytes::from(data))
        .map_err(|e| McpError::Pdf(format!("Failed to open PDF: {e:?}")))?;
    let listed: Vec<ListedObject> = objects_of_page(doc.inner(), args.page)
        .map_err(|e| McpError::Pdf(format!("Failed to read the page's objects: {e:?}")))?
        .into_iter()
        .map(|o| ListedObject {
            object: o.index,
            name: o.name,
            kind: if o.image { "image" } else { "form" },
            corners: o.corners,
        })
        .collect();
    Ok(serde_json::to_string_pretty(&listed)?)
}

/// Arguments for `edit_object`. Exactly one of `move_to`, `scale`, `rotate_degrees` and
/// `replace_with` is given.
#[derive(Deserialize, JsonSchema)]
pub struct EditObjectArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Zero-based page index.
    pub page: usize,
    /// The object, by the number `list_objects` gives it.
    pub object: usize,
    /// Move it so its lower left corner is here, in points from the page's.
    pub move_to: Option<(f64, f64)>,
    /// Scale it about its centre by this much.
    pub scale: Option<f64>,
    /// Turn it about its centre by this many degrees, anticlockwise.
    pub rotate_degrees: Option<f64>,
    /// Draw this JPEG file in its place. An image only.
    pub replace_with: Option<String>,
}

/// Implementation of the edit_object tool.
pub fn edit_object_impl(args: EditObjectArgs) -> Result<String, String> {
    let mut edits = Vec::new();
    if let Some(to) = args.move_to {
        edits.push(XObjectEdit::Move { to });
    }
    if let Some(by) = args.scale {
        edits.push(XObjectEdit::Scale { by });
    }
    if let Some(degrees) = args.rotate_degrees {
        edits.push(XObjectEdit::Rotate { degrees });
    }
    if let Some(path) = &args.replace_with {
        let jpeg = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        edits.push(XObjectEdit::Replace { jpeg });
    }
    let [edit] = <[XObjectEdit; 1]>::try_from(edits).map_err(|given| {
        format!(
            "give exactly one of move_to, scale, rotate_degrees and replace_with; {} were given",
            given.len()
        )
    })?;
    let op = Operation::EditXObject { page: args.page, object: args.object, edit };
    execute_single_op(&args.input_path, &args.output_path, op, "Object edited")
}
