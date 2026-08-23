//! Running a form's calculation order (ISO 32000-2, 12.6.3).
//!
//! The fixtures are `target/scripts/`, built by
//! `cargo run --example make_script_fixtures -p fepdf-model`, because `/AA /C` occurs
//! **zero** times across both corpora. The shapes are rebuilt here so the assertions run
//! without them.

use fepdf::{FormFieldSpec, FormValue, Operation, PdfDocument};
use fepdf_script::{DocumentHandle, ScriptEnvironment, run_calculations};

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

fn field(name: &str, value: &str, calculate: &str) -> String {
    format!(
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) /V ({value}) \
         /Rect [0 0 100 20] /F 4 /DA (/Helv 9 Tf 0 g) {calculate} >>"
    )
}

fn calc(js: &str) -> String {
    format!("/AA << /C << /S /JavaScript /JS ({js}) >> >>")
}

/// `total` = `a` + `b`, with `/CO` naming `total`.
fn sum_form() -> Vec<u8> {
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
            &calc(
                r"event.value = Number\(this.getField\('a'\).value\) \
                  + Number\(this.getField\('b'\).value\);",
            ),
        ),
    ];
    assemble(&bodies)
}

/// Two fields each computed from the other, which 12.6.3 permits.
fn cyclic_form() -> Vec<u8> {
    let bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R] \
         /CO [5 0 R 6 0 R] /DA (/Helv 9 Tf 0 g) >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [5 0 R 6 0 R] \
         /Contents 4 0 R >>"
            .to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
        field("a", "1", &calc(r"event.value = Number\(this.getField\('b'\).value\) + 1;")),
        field("b", "1", &calc(r"event.value = Number\(this.getField\('a'\).value\) + 1;")),
    ];
    assemble(&bodies)
}

fn handle(file: Vec<u8>) -> DocumentHandle {
    DocumentHandle::new(PdfDocument::open(file.into()).expect("the fixture opens"))
}

#[test]
fn the_calculation_order_is_read_from_the_form() {
    let handle = handle(sum_form());
    let order = handle.with(|doc| fepdf::calculation_order(doc.inner()));
    assert_eq!(order, vec!["total".to_string()], "/CO says which fields calculate");
}

#[test]
fn running_the_order_computes_the_field() {
    let handle = handle(sum_form());
    let report = run_calculations(&handle, &ScriptEnvironment::default()).expect("runs");
    assert_eq!(report.calculated, vec!["total".to_string()]);
    assert!(!report.stopped_early);

    let total = handle.with(|doc| fepdf::field_value(doc.inner(), "total"));
    assert_eq!(total.as_deref(), Some("5"), "2 + 3, read back from the document");
}

#[test]
fn a_form_with_no_calculation_order_runs_nothing() {
    // The half that keeps this honest: a runner that reports work on every form would be
    // a constant rather than a signal.
    let handle = handle(assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
    ]));
    let report = run_calculations(&handle, &ScriptEnvironment::default()).expect("runs");
    assert!(report.calculated.is_empty());
    assert_eq!(report.passes, 0, "nothing declared, so no pass was made");
}

#[test]
fn a_cycle_is_bounded_rather_than_forbidden() {
    // 12.6.3 permits A -> B -> A, so the guard cannot be "do not calculate a field
    // twice". It is a bounded pass count, and reaching it is recorded.
    let handle = handle(cyclic_form());
    let report = run_calculations(&handle, &ScriptEnvironment::default()).expect("runs");
    assert!(report.stopped_early, "values were still changing at the bound");
    assert!(report.passes > 1, "a cycle takes more than one pass to show itself");

    let recorded = handle.with(|doc| doc.decisions());
    let stop = recorded
        .iter()
        .find(|d| d.clause == "12.6.3" && d.found.contains("still changed"))
        .expect("stopping has to be recorded, not silent");
    assert!(stop.action.contains("stale"), "and say what it costs: {}", stop.action);
}

#[test]
fn setting_a_value_then_calculating_leaves_nothing_stale() {
    // The phase's *Done when*, as a test. Setting `a` records the 12.6.3 Violation
    // saying the scripts were not run; running them is what answers it.
    let handle = handle(sum_form());
    handle
        .with_mut(|doc| {
            doc.apply(Operation::SetFormFieldValue(FormFieldSpec {
                name: "a".to_string(),
                value: FormValue::Text("10".to_string()),
            }))
        })
        .expect("the value is written");

    run_calculations(&handle, &ScriptEnvironment::default()).expect("runs");
    let total = handle.with(|doc| fepdf::field_value(doc.inner(), "total"));
    assert_eq!(total.as_deref(), Some("13"), "10 + 3: the calculation saw the new value");
}
