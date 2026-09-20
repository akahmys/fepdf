//! Reading and changing a page's runs, one at a time.
//!
//! **A run is named, not searched for.** Which runs on a page belong together is a
//! question about meaning, and a content stream does not answer it: characters drawn next
//! to each other may be a word, a label and its value, or two columns set in one stream.
//! So nothing here groups runs; a caller lists them, sees what each one reads, and names
//! the one it means
//! ([ADR-0091](../../../../docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md)).
//!
//! That is why `list_runs` is here beside the three edits rather than with the other
//! readers. Naming a run without having seen the listing is guessing at a number, and
//! `edit_run` was reachable for a day with no way to get one.

use crate::tools::operations::page::execute_single_op;
use crate::{McpError, McpResult};
use bytes::Bytes;
use fepdf::Operation;
use fepdf::PdfDocument;
use fepdf::text::runs_of_page;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;

/// Arguments for `list_runs`.
#[derive(Deserialize, JsonSchema)]
pub struct ListRunsArgs {
    /// Path to the PDF file.
    pub path: String,
    /// Zero-based page index.
    pub page: usize,
}

/// One run of a page, as a caller choosing between them sees it.
#[derive(Serialize)]
pub struct ListedRun {
    /// Its position among the page's show-text operators, counting from zero — the number
    /// `edit_run`, `split_run` and `delete_run` take.
    pub run: usize,
    /// What it reads, through the font in force where it is drawn.
    pub text: String,
    /// What each of its codes reads, in order — `text` run apart again. An empty one is
    /// a glyph this engine cannot name, drawn where it says.
    pub pieces: Vec<String>,
    /// The resource name of that font, as the content stream names it.
    pub font: String,
    /// Where it draws from, on the page, in points from the bottom-left corner.
    pub origin: (f64, f64),
    /// How far it advances from there, as a vector — a run set at an angle advances along
    /// that angle. With `origin` and `height` this is the box a reader clicks.
    pub advance: (f64, f64),
    /// The box's other edge, as a vector from `origin`: the size the run is set at, in
    /// the direction its text matrix puts "up".
    pub rise: (f64, f64),
}

/// What `list_runs` returns.
#[derive(Serialize)]
pub struct ListRunsReport {
    /// Path of the document read.
    pub path: String,
    /// The page the runs are on.
    pub page: usize,
    /// Every run on it, in the order the content stream draws them.
    pub runs: Vec<ListedRun>,
}

/// Implementation of the list_runs tool.
pub fn list_runs_impl(args: ListRunsArgs) -> Result<String, String> {
    list_runs_internal(args).map_err(|e| e.to_string())
}

fn list_runs_internal(args: ListRunsArgs) -> McpResult<String> {
    let data = fs::read(&args.path).map_err(McpError::from)?;
    let doc = PdfDocument::open(Bytes::from(data))
        .map_err(|e| McpError::Pdf(format!("Failed to open PDF: {e:?}")))?;
    let listed = runs_of_page(doc.inner(), args.page)
        .map_err(|e| McpError::Pdf(format!("Failed to read the page's runs: {e:?}")))?;

    let report = ListRunsReport {
        path: args.path,
        page: args.page,
        runs: listed
            .into_iter()
            .map(|run| ListedRun {
                run: run.index,
                text: run.text,
                pieces: run.pieces,
                font: run.font,
                origin: run.origin,
                advance: run.advance,
                rise: run.rise,
            })
            .collect(),
    };
    Ok(serde_json::to_string_pretty(&report)?)
}

/// Arguments for `edit_run`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditRunArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Target 0-based page index.
    pub page: usize,
    /// Which run to change, as `list_runs` numbers it.
    pub run: usize,
    /// What that run reads afterwards.
    pub text: String,
}

/// Implementation of the edit_run tool.
pub fn edit_run_impl(args: EditRunArgs) -> Result<String, String> {
    let op = Operation::EditRun { page: args.page, run: args.run, text: args.text };
    execute_single_op(&args.input_path, &args.output_path, op, "Run replaced")
}

/// Arguments for `split_run`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SplitRunArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Target 0-based page index.
    pub page: usize,
    /// Which run to cut, as `list_runs` numbers it.
    pub run: usize,
    /// How many of its codes stay in the first half. `list_runs` reports what each code
    /// reads, which is how a place in the text becomes a place among the codes.
    pub after: usize,
}

/// Implementation of the split_run tool.
pub fn split_run_impl(args: SplitRunArgs) -> Result<String, String> {
    let op = Operation::SplitRun { page: args.page, run: args.run, after: args.after };
    execute_single_op(&args.input_path, &args.output_path, op, "Run split in two")
}

/// Arguments for `delete_run`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteRunArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Target 0-based page index.
    pub page: usize,
    /// Which run to take off the page, as `list_runs` numbers it.
    pub run: usize,
}

/// Implementation of the delete_run tool.
pub fn delete_run_impl(args: DeleteRunArgs) -> Result<String, String> {
    let op = Operation::DeleteRun { page: args.page, run: args.run };
    execute_single_op(&args.input_path, &args.output_path, op, "Run deleted")
}

/// Arguments for `merge_runs`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MergeRunsArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Target 0-based page index.
    pub page: usize,
    /// The first of the two runs to join, as `list_runs` numbers it. The second is the
    /// one after it.
    pub run: usize,
}

/// Implementation of the merge_runs tool.
pub fn merge_runs_impl(args: MergeRunsArgs) -> Result<String, String> {
    let op = Operation::MergeRuns { page: args.page, run: args.run };
    execute_single_op(&args.input_path, &args.output_path, op, "Runs joined into one")
}

/// Arguments for `move_run`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveRunArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Target 0-based page index.
    pub page: usize,
    /// Which run to move, as `list_runs` numbers it.
    pub run: usize,
    /// Where it draws from afterwards, in points from the left edge of the page.
    pub x: f64,
    /// Where it draws from afterwards, in points from the bottom edge of the page.
    pub y: f64,
}

/// Implementation of the move_run tool.
pub fn move_run_impl(args: MoveRunArgs) -> Result<String, String> {
    let op = Operation::MoveRun { page: args.page, run: args.run, to: (args.x, args.y) };
    execute_single_op(&args.input_path, &args.output_path, op, "Run moved")
}
