//! PDF document operation tools mapping to fepdf canonical operations.

pub mod advanced;
pub mod decoration;
pub mod metadata;
pub mod page;
pub mod struct_elem;
pub mod vocabulary;

use crate::McpError;
use bytes::Bytes;
use fepdf::{Operation, PageSelection, PdfDocument};
use fepdf_script::{DocumentHandle, ScriptEnvironment, run_calculations};
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

/// Applies `op`, and runs the form's calculation order when `op` changed a field value.
///
/// **12.6.3's cascade is the frontend's to run.** Setting a field value in a form that
/// declares `/CO` is the start of a cascade — 12.6.3 names the example directly — and
/// `apply` cannot run it: `fepdf-script` sits above the facade, so an `Operation` reaches
/// `fepdf-doc` before anything can say whether the scripts will follow
/// ([ADR-0032](../../../../../docs/adr/0032-running-scripts-is-a-frontend-verb-not-an-operation.md)).
/// Until this, nothing called `run_calculations` but its own tests, and every field write
/// through this server recorded a `Violation` of 12.6.3 saying the computed fields were
/// now stale. They were.
///
/// **The order ran after every operation until 2026-09-09**, on the argument that a form
/// with no `/CO` returns from `run_calculations` before building a context — which is true
/// about the cost and says nothing about the effect on a form that has one. Measured:
/// `rotate_pages` on a form whose `/CO` computes `total` from `a` moved `total` from `0`
/// to `2`, and on a form whose `/CO` writes `new Date().getFullYear()` it overwrote
/// `2026-09-09` with **`2020`** — the fixed instant `ScriptEnvironment::default()` uses so
/// that two runs of the same document agree. A page rotation rewriting a date with a
/// constant is not a stale field being refreshed.
///
/// 12.6.3's trigger is a field value changing, and `SetFormFieldValue` is the only
/// `Operation` that changes one. `tests/calculation_scope_test.rs` holds both halves.
///
/// **A run that does not complete puts the warning back.** Declaring a script processor
/// and then not running one is worse than never declaring it — the engine stops naming a
/// staleness that is now real — so the failure is recorded where `apply` would have
/// recorded it.
fn apply_and_calculate(doc: PdfDocument, op: Operation) -> Result<DocumentHandle, McpError> {
    let changes_a_field = matches!(op, Operation::SetFormFieldValue { .. });
    doc.inner().declare_script_processor();
    let handle = DocumentHandle::new(doc);
    handle.with_mut(|d| d.apply(op)).map_err(|e| McpError::pdf("Operation failed", e))?;
    if !changes_a_field {
        return Ok(handle);
    }
    if let Err(why) = run_calculations(&handle, &ScriptEnvironment::default()) {
        handle.with(|d| {
            d.inner().record(fepdf::Decision::violation(
                "12.6.3",
                format!("the form's calculation order did not complete: {why:?}"),
                "wrote the value and could not finish the scripts; fields computed from it \
                 may be stale",
            ));
        });
    }
    Ok(handle)
}

/// Applies a generic raw Operation JSON to mutate a PDF document.
pub fn apply_operation_impl(args: ApplyOperationArgs) -> Result<String, McpError> {
    let op: Operation = serde_json::from_str(&args.operation_json)
        .map_err(|e| format!("Failed to parse Operation JSON: {e}"))?;

    let data = fs::read(&args.input_path)
        .map_err(|e| format!("Failed to read input file '{}': {e}", args.input_path))?;
    let doc =
        PdfDocument::open(Bytes::from(data)).map_err(|e| McpError::pdf("Failed to open PDF", e))?;

    let handle = apply_and_calculate(doc, op)?;

    let out = Path::new(&args.output_path);
    handle.with(|d| d.save_with_options(out, "2.0", &fepdf::SaveOptions::default())).map_err(
        |e| McpError::pdf(format!("Failed to save output PDF '{}'", args.output_path), e),
    )?;

    Ok(serde_json::json!({
        "status": "SUCCESS",
        "input_path": args.input_path,
        "output_path": args.output_path,
        "message": "Operation applied successfully"
    })
    .to_string())
}

/// Parses the page-selection string every page tool on this surface accepts.
///
/// The accepted forms are the ones the schemas document, **counting from 1**: `all`,
/// a single page (`2`), or an inclusive range (`1-3`). `None` means `all`, for the tools
/// whose field is optional. Anything else is an error naming what was not understood.
///
/// **Refusing is the whole point.** Three copies of this used to live in `page.rs`,
/// `vocabulary.rs` and `decoration.rs`, all three ending `_ => PageSelection::All`, so
/// every string they could not read selected the entire document:
///
/// | input | was | now |
/// | --- | --- | --- |
/// | `"foo"`, `""` | every page | refused |
/// | `"2,3"` — the first thing a caller reaches for | every page | refused |
/// | `"0"`, `"-1"` | page 1 | refused, the field counts from 1 |
/// | `"3-"`, `"5-x"` | page 3 in one copy, nothing in the other | refused |
///
/// On `remove_pages` the first row deleted the file's every page and reported SUCCESS.
/// A selection nobody can parse is not a selection, and guessing `All` for it is the
/// most destructive guess available (RR-15 Rule 13).
///
/// A fourth site, `apply_bates_numbering`, did not parse its `pages` field at all — it
/// opened `let pages = PageSelection::All;` and never read the argument.
pub fn parse_selection(text: Option<&str>) -> Result<PageSelection, McpError> {
    let Some(raw) = text.map(str::trim) else { return Ok(PageSelection::All) };
    if raw.eq_ignore_ascii_case("all") {
        return Ok(PageSelection::All);
    }
    if let Some((first, last)) = raw.split_once('-') {
        let first = one_based(first)?;
        let last = one_based(last)?;
        if last < first {
            return Err(format!("page range \"{raw}\" ends before it begins").into());
        }
        return Ok(PageSelection::Indices(((first - 1)..last).collect()));
    }
    Ok(PageSelection::Single(one_based(raw)? - 1))
}

/// One page number as the schemas define it: an integer, counting from 1.
fn one_based(text: &str) -> Result<usize, McpError> {
    match text.trim().parse::<usize>() {
        Ok(0) => Err("page numbers count from 1, so 0 is not a page".to_string().into()),
        Ok(n) => Ok(n),
        Err(_) => Err(format!(
            "\"{}\" is not a page selection: use \"all\", a page number such as \"2\", \
             or a range such as \"1-3\"",
            text.trim()
        )
        .into()),
    }
}
