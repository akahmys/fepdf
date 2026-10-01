//! The three `inspect` reports this window had no counterpart for.
//!
//! **Measured 2026-09-09**: of `fepdf inspect`'s ten reports, `encryption`, `actions` and
//! `coverage` were reachable only from a terminal. `actions` is the one a reader wants
//! first about a PDF someone sent them — what runs before they touch anything — and the
//! security handler was already carried into the app and shown nowhere.
//!
//! What is checked here is that the three answers exist and are the shape the panel
//! reads, which is the part that goes quiet when an engine API moves.

use fepdf::{ActionReport, Coverage, PdfDocument};

fn sample(name: &str) -> Option<Vec<u8>> {
    std::fs::read(format!("../../samples/{name}")).ok()
}

/// A document that does nothing says so, rather than saying nothing.
#[test]
fn a_document_with_no_actions_reports_none() {
    let Some(bytes) = sample("constitution.pdf") else {
        panic!("samples/ is not in the tree, so this has nothing to test")
    };
    let doc = PdfDocument::open(bytes.into()).expect("the sample opens");
    let report = ActionReport::of(doc.inner()).expect("the walk completes");
    assert!(
        report.without_interaction().is_empty(),
        "this sample fires nothing on open, so the panel's first line is the reassuring one"
    );
    assert!(report.capabilities().is_empty(), "and it carries no action at all");
}

/// A document that runs something on open is what the panel exists for.
#[test]
fn a_script_on_open_is_reported_without_interaction() {
    // Built here rather than found: `/AA /O` occurs in no sample, and a panel that only
    // ever shows "nothing" has not been seen working.
    let file = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /OpenAction << /S /JavaScript /JS (app.alert\\('hi'\\);) >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
    ]);
    let doc = PdfDocument::open(file.into()).expect("the fixture opens");
    let report = ActionReport::of(doc.inner()).expect("the walk completes");
    assert!(
        !report.without_interaction().is_empty(),
        "an /OpenAction runs before the reader does anything and has to be named"
    );
    assert!(
        report.capabilities().iter().any(|(c, _)| matches!(c, fepdf::Capability::RunsCode)),
        "and it is code, which is the capability worth showing in a warning colour"
    );
}

/// The panel's third section, and the axes it lists.
#[test]
fn coverage_reports_the_axes_the_panel_lists() {
    let Some(bytes) = sample("constitution.pdf") else {
        panic!("samples/ is not in the tree, so this has nothing to test")
    };
    let coverage = Coverage::of(&bytes).expect("coverage computes");
    let axes = coverage.axes();
    assert!(!axes.is_empty(), "the panel has rows to draw");
    assert!(
        axes.iter().any(|a| a.presented > 0),
        "and at least one axis the file actually presents"
    );
}

/// The security handler the panel puts under Protection, which `handle_open` already had.
#[test]
fn an_encrypted_document_names_its_handler() {
    let locked = "../../target/encrypted/aes256.pdf";
    if !std::path::Path::new(locked).exists() {
        return;
    }
    let doc = PdfDocument::open(std::fs::read(locked).expect("reads").into()).expect("opens");
    let method = doc.security_method();
    assert!(
        method.contains("AES"),
        "the panel shows what the engine names, and it named: {method}"
    );
}
