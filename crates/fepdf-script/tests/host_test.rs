//! What a document script can see, and what it cannot (ISO 32000-2 12.6.4.16).
//!
//! The two scripts the corpus actually carries are here by name. Between 524 files there
//! are exactly two distinct ones — `app.alert("Hello World!")` and Adobe's stock
//! file-attachment boilerplate — so these are not a sample of real scripts; they are all
//! of them. Fixtures are what will make this testable, and they are the next entry.

use fepdf::PdfDocument;
use fepdf_script::{DocumentHandle, ScriptEnvironment, ScriptError, ScriptHost};

/// A one-page document, so `this.numPages` has something true to report.
fn document() -> PdfDocument {
    let content = "0 0 0 rg 0 0 10 10 re f\n";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ];
    let mut out = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let table_at = out.len();
    let size = bodies.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n")
            .as_bytes(),
    );
    PdfDocument::open(out.into()).expect("the fixture opens")
}

fn host(environment: ScriptEnvironment) -> ScriptHost {
    ScriptHost::new(DocumentHandle::new(document()), environment)
}

/// Adobe's stock file-attachment script, as four of the corpus's seven files carry it.
const ATTACHMENT_BOILERPLATE: &str = r"
    var v = app.viewerVersion;
    if (v < 7) {
        var n = 0;
        if (this.dataObjects != null) n = this.dataObjects.length;
        if (v >= 6 && v < 7) {
            if (n == 0) {
                var np = this.numPages;
                syncAnnotScan();
            }
        }
    }
";

#[test]
fn the_corpus_alert_script_completes() {
    let outcome = host(ScriptEnvironment::default())
        .run(r#"app.alert("Hello World!");"#)
        .expect("the script completes");
    assert_eq!(outcome.alerts, vec!["Hello World!".to_string()]);
}

#[test]
fn a_script_reads_the_document_it_was_given() {
    let outcome = host(ScriptEnvironment::default())
        .run("app.alert('pages=' + this.numPages);")
        .expect("completes");
    assert_eq!(outcome.alerts, vec!["pages=1".to_string()], "not a constant: the document said so");
}

#[test]
fn the_injected_viewer_version_decides_which_branch_runs() {
    // The measurement this crate was built on. At 7 the guard is false and the script
    // completes having done nothing; at 6 it reaches an Acrobat global this engine does
    // not provide and fails. Determinism injection is not tidiness — it decides the
    // outcome.
    let quiet = ScriptEnvironment { viewer_version: 7.0, ..ScriptEnvironment::default() };
    assert!(host(quiet).run(ATTACHMENT_BOILERPLATE).is_ok(), "7: the guard is false");

    let deep = ScriptEnvironment { viewer_version: 6.0, ..ScriptEnvironment::default() };
    let error = host(deep).run(ATTACHMENT_BOILERPLATE).expect_err("6: reaches syncAnnotScan");
    let ScriptError::DidNotComplete(message) = error else {
        panic!("a missing Acrobat global is the script failing, not the host");
    };
    assert!(message.contains("syncAnnotScan"), "says what was missing: {message}");
}

#[test]
fn the_same_environment_gives_the_same_answer_twice() {
    // RR-15's determinism rules bind anything that decides output.
    let script = "app.alert('v=' + app.viewerVersion);";
    let once = host(ScriptEnvironment::default()).run(script).expect("completes");
    let twice = host(ScriptEnvironment::default()).run(script).expect("completes");
    assert_eq!(once.alerts, twice.alerts);
}

#[test]
fn a_script_that_throws_is_reported_and_does_not_panic() {
    let error = host(ScriptEnvironment::default())
        .run("throw new Error('deliberate');")
        .expect_err("a throwing script is an error, not a panic");
    assert!(matches!(error, ScriptError::DidNotComplete(_)));
}

#[test]
fn nothing_beyond_app_and_this_is_provided() {
    // 12.6.4.16's object model is large and this engine implements a subset. A script
    // reaching outside it must fail loudly rather than find an empty object that quietly
    // answers undefined to everything.
    for reaching in ["util.printf('%d', 1)", "event.value", "color.red", "Collab.something()"] {
        assert!(
            host(ScriptEnvironment::default()).run(reaching).is_err(),
            "{reaching} must not silently succeed"
        );
    }
}
