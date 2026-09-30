//! What setting a field value costs in a form that calculates (ISO 32000-2, 12.6.3).
//!
//! This is the measurement Phase R exists to move. 12.6.3 says a field-related action may
//! "make any other modification to the document" and names the case directly: modifying a
//! field value can trigger calculations for *other* fields.
//!
//! **Whether they run is the frontend's to say.** `fepdf-script` executes them and sits
//! above the facade, so an `Operation` reaches `fepdf-doc` before anything on that side
//! can know ([ADR-0032](../../../docs/adr/0032-running-scripts-is-a-frontend-verb-not-an-operation.md)).
//! A run that will follow with a script run says so through `declare_script_processor`
//! and this stays quiet; one that does not gets the `Violation` it always got. Both
//! halves are here, because a flag that is never checked and a flag that suppresses
//! everything look the same from one side.
//!
//! **No file in either corpus can test this.** `/AA /C` occurs zero times across 524
//! files. The document below is built here for the same reason
//! `crates/fepdf-model/examples/make_script_fixtures.rs` writes its siblings to
//! `target/scripts/` — that example produces files to inspect by hand; the assertions
//! live here, where they run.

use fepdf::{FormFieldSpec, FormValue, Operation, PdfDocument};

use fepdf_fixtures::{FormField, acroform};

/// A form whose `total` is computed from `a` and `b`, with `/CO` naming the order.
fn calculating_form() -> Vec<u8> {
    acroform(
        &[
            FormField::new("a", "2"),
            FormField::new("b", "3"),
            FormField::new("total", "0").calculating(r"event.value = this.getField\('a'\).value;"),
        ],
        &[2],
    )
}

/// The same form with no `/CO`, so nothing is declared to be calculated.
fn plain_form() -> Vec<u8> {
    acroform(&[FormField::new("a", "2")], &[])
}

fn set_value(file: Vec<u8>, field: &str) -> Vec<fepdf::Decision> {
    let mut doc = PdfDocument::open(file.into()).expect("the fixture opens");
    doc.apply(Operation::SetFormFieldValue(FormFieldSpec {
        name: field.to_string(),
        value: FormValue::Text("9".to_string()),
    }))
    .expect("the value is written");
    doc.decisions()
}

#[test]
fn setting_a_value_in_a_calculating_form_reports_the_scripts_it_did_not_run() {
    let decisions = set_value(calculating_form(), "a");
    let found = decisions.iter().find(|d| d.clause == "12.6.3");
    let found = found.expect("12.6.3 must be recorded: the form declares a calculation order");
    assert!(
        found.action.contains("did not run"),
        "it has to say the value was written and the scripts were not: {}",
        found.action
    );
    // One sentence, not two joined by the indentation of the line the literal broke on.
    assert!(
        found.found.contains("and setting a would have run"),
        "the finding does not read as a sentence: {:?}",
        found.found
    );
}

/// **A run that will execute the scripts does not report skipping them.**
///
/// Until the MCP server called `run_calculations`, nothing did, and this `Violation` was
/// true of every write. It is now true only of a caller that has not been wired, which is
/// what the flag distinguishes.
#[test]
fn a_run_that_declares_a_script_processor_reports_nothing() {
    let mut doc = PdfDocument::open(calculating_form().into()).expect("the fixture opens");
    doc.inner().declare_script_processor();
    doc.apply(Operation::SetFormFieldValue(FormFieldSpec {
        name: "a".to_string(),
        value: FormValue::Text("9".to_string()),
    }))
    .expect("the value is written");
    assert!(
        !doc.decisions().iter().any(|d| d.clause == "12.6.3"),
        "the scripts are run by the caller, so nothing here reports otherwise"
    );
}

#[test]
fn a_form_without_a_calculation_order_reports_nothing() {
    // The other half, and the one that keeps this honest: a `Decision` that fires on
    // every form would be a constant rather than a signal (ARCHITECTURE §4.3).
    let decisions = set_value(plain_form(), "a");
    assert!(
        !decisions.iter().any(|d| d.clause == "12.6.3"),
        "nothing is calculated, so there is nothing stale to report"
    );
}
