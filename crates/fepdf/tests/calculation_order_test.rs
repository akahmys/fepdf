//! What setting a field value costs in a form that calculates (ISO 32000-2, 12.6.3).
//!
//! This is the measurement Phase R exists to move. 12.6.3 says a field-related action may
//! "make any other modification to the document" and names the case directly: modifying a
//! field value can trigger calculations for *other* fields. This engine writes the value
//! and records a `Violation` saying it did not run them.
//!
//! **No file in either corpus can test this.** `/AA /C` occurs zero times across 524
//! files. The document below is built here for the same reason
//! `crates/fepdf-model/examples/make_script_fixtures.rs` writes its siblings to
//! `target/scripts/` — that example produces files to inspect by hand; the assertions
//! live here, where they run.

use fepdf::{FormFieldSpec, FormValue, Operation, PdfDocument};

/// A form whose `total` is computed from `a` and `b`, with `/CO` naming the order.
fn calculating_form() -> Vec<u8> {
    let field = |name: &str, value: &str, calc: &str| {
        format!(
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) /V ({value}) \
             /Rect [0 0 100 20] /F 4 /DA (/Helv 9 Tf 0 g) {calc} >>"
        )
    };
    let bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R 7 0 R] \
         /CO [7 0 R] /DA (/Helv 9 Tf 0 g) >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [5 0 R 6 0 R 7 0 R] \
         /Contents 4 0 R >>"
            .to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
        field("a", "2", ""),
        field("b", "3", ""),
        field(
            "total",
            "0",
            r"/AA << /C << /S /JavaScript /JS (event.value = this.getField\('a'\).value;) >> >>",
        ),
    ];
    assemble(&bodies)
}

/// The same form with no `/CO`, so nothing is declared to be calculated.
fn plain_form() -> Vec<u8> {
    let bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] \
         /DA (/Helv 9 Tf 0 g) >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [5 0 R] \
         /Contents 4 0 R >>"
            .to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (a) /V (2) /Rect [0 0 100 20] /F 4 \
         /DA (/Helv 9 Tf 0 g) >>"
            .to_string(),
    ];
    assemble(&bodies)
}

fn assemble(bodies: &[String]) -> Vec<u8> {
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
    out
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
}

#[test]
fn a_form_without_a_calculation_order_reports_nothing() {
    // The other half, and the one that keeps this honest: a `Decision` that fires on
    // every form would be a constant rather than a signal (ARCHITECTURE §5.3).
    let decisions = set_value(plain_form(), "a");
    assert!(
        !decisions.iter().any(|d| d.clause == "12.6.3"),
        "nothing is calculated, so there is nothing stale to report"
    );
}
