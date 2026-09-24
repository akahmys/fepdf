//! What a browser can learn from a document through this crate.
//!
//! **It could learn how long a PDF was and nothing else.** This crate exposed three
//! functions — open, page count, and a `render_page` that refuses — while the engine
//! behind it reads 96% of what a corpus presents. Everything added here needs no GPU,
//! which is the line between what this crate can do and what it says it cannot.
//!
//! Tested through the plain functions rather than the `wasm_bindgen` methods, because
//! `JsValue` cannot be constructed off a WebAssembly target — the same reason
//! `refusal_test.rs` gives.

use fepdf::PdfDocument;
use fepdf_fixtures::assemble;
use fepdf_wasm::{decisions_of, struct_tree_of, text_of};

fn sample(name: &str) -> Option<PdfDocument> {
    let bytes = std::fs::read(format!("../../samples/{name}")).ok()?;
    PdfDocument::open(bytes.into()).ok()
}

#[test]
fn a_page_yields_its_text() {
    let Some(doc) = sample("constitution.pdf") else { return };
    let text = text_of(&doc, 0).expect("page 1 extracts");
    assert!(!text.trim().is_empty(), "a page with text on it came back empty");
}

/// A page that does not exist is an error, not an empty string — the two are different
/// answers and a caller acting on the first cannot tell.
#[test]
fn a_page_that_is_not_there_is_an_error() {
    let Some(doc) = sample("constitution.pdf") else { return };
    assert!(text_of(&doc, 9_999).is_err(), "a page past the end reported success");
}

/// The decisions are what this engine has instead of a log, so a caller that cannot see
/// them has the logging problem back.
///
/// **The document is one that departs from the standard**, and the assertion is that a
/// decision arrives — not merely that the JSON is an array. This used to open
/// `fy05.pdf`, which costs 5.8 seconds and whose layout departs from nothing: an empty
/// array satisfied `is_array()`, so a `decisions_of` returning `[]` for every document
/// would have passed it.
#[test]
fn the_decisions_cross_as_json() {
    let doc = PdfDocument::open(wrong_stream_length().into()).expect("the fixture opens");
    let json = decisions_of(&doc).expect("the decisions serialise");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("it is JSON");
    let Some(entries) = parsed.as_array() else {
        panic!("the decisions cross as an array: {json:.80}");
    };
    assert!(!entries.is_empty(), "a document that departs from 7.3.8.2 recorded nothing");
}

/// A document with no structure tree says `null`, which is an answer rather than a
/// failure to read one.
#[test]
fn a_document_without_a_structure_tree_says_null() {
    let doc = PdfDocument::create_empty().expect("an empty document");
    assert_eq!(struct_tree_of(&doc).expect("it serialises"), "null");
}

/// And one that has a tree hands it over.
///
/// **This used to open `fy05.pdf`, which has no structure tree**, and accept
/// `is_object() || is_null()` — so it ran the absent branch and passed, under a name
/// saying it ran the other one. It cost 5.8 seconds to do that. The tagged document is
/// four objects, and `is_object()` alone is now the assertion.
#[test]
fn a_tagged_document_yields_its_tree() {
    let doc = PdfDocument::open(tagged().into()).expect("the fixture opens");
    let json = struct_tree_of(&doc).expect("it serialises");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("it is JSON");
    assert!(parsed.is_object(), "a document carrying /StructTreeRoot yielded {json:.80}");
}

/// A document whose content stream declares a `/Length` the data does not have, which
/// 7.3.8.2 makes a departure the reader repairs and records.
///
/// A *reading* departure and not a painting one, because `decisions_of` is asked of a
/// document that has been opened and not yet drawn — a content stream's own decisions
/// are recorded when the interpreter runs, which is later than this.
fn wrong_stream_length() -> Vec<u8> {
    let content = "0 0 10 10 re f\n";
    assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 50 50] /Contents 4 0 R >>".to_string(),
        // 3, where the data is fifteen bytes: far enough out that the reader cannot read
        // it as the ambiguity 7.3.8.2 lets a trailing CR create.
        format!("<< /Length 3 >>\nstream\n{content}endstream"),
    ])
}

/// A one-page document carrying a `/StructTreeRoot` with a single `/P` under it.
fn tagged() -> Vec<u8> {
    assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 4 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 50 50] >>",
        "<< /Type /StructTreeRoot /K [5 0 R] >>",
        "<< /Type /StructElem /S /P /P 4 0 R /Pg 3 0 R >>",
    ])
}
