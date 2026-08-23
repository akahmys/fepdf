//! What a script gets when it asks for a locale this engine does not carry.
//!
//! The defect these pin is not "ECMA-402 is missing" — it is that its absence used to
//! look like success. `(1234567.891).toLocaleString('de-DE')` answered `"1234567.891"`,
//! which is the number a German invoice must not show, returned as though the locale had
//! been applied.

use fepdf::{PdfDocument, Severity};
use fepdf_script::{DocumentHandle, ScriptEnvironment, ScriptError, ScriptHost};

/// A one-page document, so a run has something to record against.
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

/// Runs one expression, alerting its value, and hands back the run and the document.
fn run(expression: &str) -> (Result<Vec<String>, ScriptError>, DocumentHandle) {
    let handle = DocumentHandle::new(document());
    let script = format!("app.alert(String({expression}));");
    let outcome = ScriptHost::new(handle.clone(), ScriptEnvironment::default())
        .run(&script)
        .map(|o| o.alerts);
    (outcome, handle)
}

/// What the run alerted, or the message it threw.
fn said(expression: &str) -> String {
    match run(expression).0 {
        Ok(alerts) => alerts.join("|"),
        Err(error) => format!("THREW: {error}"),
    }
}

#[test]
fn a_named_locale_is_refused_rather_than_answered_without_it() {
    // The whole point. Before this, every one of these returned a string.
    for asked in [
        "(1234567.891).toLocaleString('de-DE')",
        "(1234.5).toLocaleString('de-DE', {style: 'currency', currency: 'EUR'})",
        "(10n).toLocaleString('de-DE')",
        // Not replaced, and covered anyway: Array.prototype.toLocaleString delegates to
        // each element's, so the refusal travels through it.
        "[1234.5, 2].toLocaleString('de-DE')",
    ] {
        let answer = said(asked);
        assert!(answer.starts_with("THREW:"), "{asked} must not answer quietly: {answer}");
        assert!(answer.contains("12.6.4.16"), "the refusal names the clause: {answer}");
        assert!(answer.contains("de-DE"), "the refusal names what was asked: {answer}");
    }
}

#[test]
fn no_locale_answers_with_the_digits_because_that_answer_is_true() {
    // A script that names no locale asked for this host's default, and this host's
    // default really is unlocalised digits. That is the one case where the old answer
    // was not a lie — so it survives, and gets recorded rather than refused.
    assert_eq!(said("(1234567.891).toLocaleString()"), "1234567.891");
    assert_eq!(said("[1234.5, 2].toLocaleString()"), "1234.5, 2");
    // `undefined` is what an omitted argument becomes through a wrapper.
    assert_eq!(said("(1234567.891).toLocaleString(undefined)"), "1234567.891");
}

#[test]
fn both_answers_are_recorded_against_the_document() {
    // Rule 20: a departure the caller cannot see is a defect even when the output is
    // right. `inspect structure` prints these.
    let (_, handle) = run("(1234567.891).toLocaleString('de-DE')");
    let refused = handle.with(PdfDocument::decisions);
    assert_eq!(refused.len(), 1, "one decision: {refused:?}");
    assert_eq!(refused[0].severity, Severity::Violation);
    assert_eq!(refused[0].clause, "12.6.4.16");
    assert!(refused[0].found.contains("de-DE"), "names the locale: {:?}", refused[0]);

    let (_, handle) = run("(1234567.891).toLocaleString()");
    let taken = handle.with(PdfDocument::decisions);
    assert_eq!(taken.len(), 1, "one decision: {taken:?}");
    assert_eq!(taken[0].severity, Severity::Ambiguity, "a permitted reading, not a violation");
}

#[test]
fn a_column_of_numbers_records_one_decision_and_not_a_thousand() {
    // The log is a document-lifetime `Vec` behind a lock and `inspect structure` prints
    // all of it. A per-call record would bury every other decision in the file.
    //
    // The bound is one *script execution*, not one document: `run_calculations` builds a
    // context per field per pass, so a two-pass form with two formatting fields records
    // four. Bounding that would mean scanning the log on every call.
    let handle = DocumentHandle::new(document());
    let script = "var s = ''; for (var i = 0; i < 1000; i++) { s = (i).toLocaleString(); }";
    ScriptHost::new(handle.clone(), ScriptEnvironment::default()).run(script).expect("completes");
    assert_eq!(handle.with(PdfDocument::decisions).len(), 1);
}

#[test]
fn the_date_methods_name_the_clause_instead_of_function_unimplemented() {
    // These threw before too — with boa's `Function Unimplemented`, which tells a caller
    // neither which function nor that a locale was the reason.
    for asked in [
        "new Date().toLocaleDateString('de-DE')",
        "new Date().toLocaleTimeString()",
        "new Date().toLocaleString('de-DE')",
    ] {
        let answer = said(asked);
        assert!(answer.contains("12.6.4.16"), "{asked} names the clause: {answer}");
        assert!(!answer.contains("Function Unimplemented"), "{asked} still bare: {answer}");
    }
}

#[test]
fn what_was_left_alone_is_pinned_rather_than_assumed() {
    // Collation and Turkish casing ignore their locale as silently as the formatters did.
    // They are left because a form's calculate action does neither — and pinned here, so
    // "left" stays a measurement instead of becoming a belief. A viewer with ECMA-402
    // answers -1, "İ" and "ı" to these three.
    assert_eq!(said("'ä'.localeCompare('z', 'de')"), "1", "German sorts ä with a");
    assert_eq!(said("'i'.toLocaleUpperCase('tr')"), "I");
    assert_eq!(said("'I'.toLocaleLowerCase('tr')"), "i");
}
