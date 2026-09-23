//! Every walk an operation takes is bounded, and a file or a caller can make it prove it.
//!
//! [ADR-0060](../../../docs/adr/0060-a-reference-chain-is-bounded-by-what-it-has-seen.md)
//! bounded `catalog::describe` after a four-object file aborted the process, and named
//! the walks the same sweep found and did not fix. These are those walks, each driven
//! from the side a caller reaches it from: an `Operation`.
//!
//! **A stack overflow aborts the test binary rather than failing one test**, so a case
//! that regresses here stops the run instead of reddening it. That is the reason these
//! are worth having and not a reason to expect them to read like ordinary tests.

use fepdf_doc::apply::apply_operation;
use fepdf_doc::operation::{Operation, PageSelection};
use fepdf_model::Document;
use fepdf_model::document::extensions::{FormFieldSpec, FormValue, OutlineNode, OutlineTree};
use fepdf_model::ingest::IngestionOptions;

use fepdf_fixtures::assemble;

fn open(objects: &[&str]) -> Document {
    Document::open(bytes::Bytes::from(assemble(objects)), &IngestionOptions::default())
        .expect("the fixture reads")
}

/// A structure tree whose `/K` leads back to an ancestor does not delete forever.
///
/// Object 5's `/K` names object 4, which is its own parent, so following `/K` from the
/// root never runs out of children. `delete_struct_node` followed it with no visited set
/// and no depth, and `DeleteStructElem` is how a frontend reaches it.
#[test]
fn deleting_a_structure_element_terminates_on_a_cyclic_k() {
    let mut doc = open(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 4 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
        "<< /Type /StructTreeRoot /K 5 0 R >>",
        "<< /Type /StructElem /S /Document /K 4 0 R >>",
    ]);

    // The handle named is one no element carries, so the walk has to exhaust the tree
    // rather than stop at the first match — which is the case that loops.
    apply_operation(&mut doc, Operation::DeleteStructElem { handle_index: 9999 })
        .expect("a cyclic structure tree is refused or exhausted, never followed forever");
}

/// A form field whose `/Kids` leads back to an ancestor does not search forever.
///
/// Object 5 is a field whose `/Kids` names object 4, its own parent. `SetFormFieldValue`
/// searches by name, so a name no field carries makes the search visit everything.
#[test]
fn setting_a_form_field_terminates_on_a_cyclic_kids() {
    let mut doc = open(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] >> >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
        "<< /T (parent) /Kids [5 0 R] >>",
        "<< /T (child) /Kids [4 0 R] >>",
    ]);

    let spec = FormFieldSpec {
        name: "no such field".to_string(),
        value: FormValue::Text("x".to_string()),
    };
    let _ = apply_operation(&mut doc, Operation::SetFormFieldValue(spec));
}

/// An outline a caller nests past the bound is refused, not built.
///
/// **Two hundred levels and not ten thousand**, which the first version of this test
/// used. At ten thousand the process does abort — in `OutlineNode`'s derived `Drop`,
/// releasing the value, not in the walk under test. A test that cannot say which of two
/// defects it caught has not caught either, and this one is about the walk.
///
/// Two hundred is also past what `fepdf-mcp` can deliver: `serde_json` refuses an
/// `OutlineNode` past 62 levels. This is the Rust-caller path, which has no such gate.
#[test]
fn an_outline_nested_deeper_than_any_document_is_refused() {
    let mut doc = open(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
    ]);

    let mut node =
        OutlineNode { title: "leaf".to_string(), destination_page: 0, children: Vec::new() };
    for _ in 0..200 {
        node =
            OutlineNode { title: "level".to_string(), destination_page: 0, children: vec![node] };
    }

    let result =
        apply_operation(&mut doc, Operation::UpdateOutlines(OutlineTree { items: vec![node] }));
    assert!(result.is_err(), "an outline past the bound is refused rather than built");
}

/// An outline nested as deep as the bound allows is still built.
///
/// The bound exists to stop a caller from reaching the stack, not to cap a document's
/// table of contents. Without this, tightening it to something small passes.
#[test]
fn an_outline_nested_within_the_bound_is_built() {
    let mut doc = open(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
    ]);

    let mut node =
        OutlineNode { title: "leaf".to_string(), destination_page: 0, children: Vec::new() };
    for _ in 0..60 {
        node =
            OutlineNode { title: "level".to_string(), destination_page: 0, children: vec![node] };
    }

    apply_operation(&mut doc, Operation::UpdateOutlines(OutlineTree { items: vec![node] }))
        .expect("sixty levels is a table of contents, not an attack");
}

/// Cloning a page recurses once per level of *direct* nesting, and the parser is what
/// stops that being unbounded — so the invariant is written down and exercised here.
///
/// `ObjectCloner::walk` queues references rather than following them, so a cyclic
/// reference graph is already safe. What it does recurse on is a dictionary or an array
/// found directly inside another, and it carries no bound of its own. It needs none,
/// because nothing can put such a value in the arena: `Parser` refuses past 512 levels
/// and records a `[VIOLATION] ISO 7.5.4`, and the arena is filled by the parser or by
/// this cloner copying what the parser produced.
///
/// **That was an invariant nothing stated**, which is the shape this sweep exists to
/// remove. Five hundred levels is just under the parser's limit and is what a clone
/// therefore has to survive; a bound inside `walk` would be a second guard for a job the
/// parser already does, and would have to answer what a truncated clone should be.
#[test]
fn a_clone_survives_the_deepest_nesting_the_parser_will_admit() {
    let deep = format!("{}1{}", "[".repeat(500), "]".repeat(500));
    let page = format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Deep {deep} >>");
    let mut doc = open(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        &page,
    ]);

    apply_operation(&mut doc, Operation::DuplicatePages(PageSelection::Single(0)))
        .expect("a page nested to just under the parser's limit clones");
    assert_eq!(doc.page_count().expect("pages"), 2, "the clone is there");
}

/// And the limit that makes the above enough is the parser's, not the cloner's.
///
/// Six hundred levels never enters the arena at all, so `walk` is never asked to go
/// there. If this ever starts reading as a document with a page, the invariant above has
/// gone and `ObjectCloner::walk` needs a bound of its own.
#[test]
fn nesting_past_the_parsers_limit_never_reaches_the_arena() {
    let deep = format!("{}1{}", "[".repeat(600), "]".repeat(600));
    let catalogue = format!("<< /Type /Catalog /Pages 2 0 R /Deep {deep} >>");
    // **`catalogue.as_str()`, not `&catalogue`.** `assemble` takes `&[B: AsRef<[u8]>]`
    // and this array mixes a `&String` with two `&str`s: rustc 1.97 picks `&str` and
    // coerces, and **1.94 — the minimum this project states — picks `&String` from the
    // first element and rejects the literals.** It was the one thing in the workspace
    // that did not compile under the stated minimum (W-T3).
    let bytes = assemble(&[
        catalogue.as_str(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
    ]);

    let opened = Document::open(bytes::Bytes::from(bytes), &IngestionOptions::default());
    assert!(opened.is_err(), "the object holding it does not parse, so there is no catalogue");
}
