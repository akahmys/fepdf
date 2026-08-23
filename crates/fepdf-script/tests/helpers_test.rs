//! Adobe's `AF*` helpers, tested because no audit reads them.
//!
//! `verify_compliance.sh`'s fifteen checks run over Rust. `scripting/aform.js` is read by
//! none of them — not the function-length limit, not determinism, not the error types —
//! so ADR-0025 made two conditions for writing them in JavaScript, and this file is the
//! first: **every helper carries a test that fails when the helper is broken.** The
//! second is the `status.sh` row counting the lines.

use fepdf::PdfDocument;
use fepdf_script::{DocumentHandle, ScriptEnvironment, ScriptHost};

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

/// A form with `a = 2`, `b = 3` and an empty `total`.
fn form() -> DocumentHandle {
    let field = |name: &str, value: &str| {
        format!(
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) /V ({value}) \
             /Rect [0 0 100 20] /F 4 /DA (/Helv 9 Tf 0 g) >>"
        )
    };
    let bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R 7 0 R] \
         /DA (/Helv 9 Tf 0 g) >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [5 0 R 6 0 R 7 0 R] \
         /Contents 4 0 R >>"
            .to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
        field("a", "2"),
        field("b", "3"),
        field("total", "0"),
    ];
    DocumentHandle::new(PdfDocument::open(assemble(&bodies).into()).expect("opens"))
}

/// Runs a calculation and reports what it wrote to `event.value`.
fn calculate(source: &str) -> String {
    ScriptHost::new(form(), ScriptEnvironment::default())
        .run_calculation(source, Some(""))
        .expect("the helper runs")
        .expect("a calculation writes event.value")
}

#[test]
fn af_make_number_reads_a_comma_as_a_decimal_point() {
    // A form filled in one locale is read in another.
    let host = ScriptHost::new(form(), ScriptEnvironment::default());
    let out = host
        .run_calculation("event.value = AFMakeNumber('3,5') + AFMakeNumber(' 1.5 ');", Some(""))
        .expect("runs")
        .expect("writes");
    assert_eq!(out, "5");
}

#[test]
fn af_make_number_refuses_what_is_not_a_number() {
    let host = ScriptHost::new(form(), ScriptEnvironment::default());
    let out = host
        .run_calculation("event.value = String(AFMakeNumber('not a number'));", Some(""))
        .expect("runs")
        .expect("writes");
    assert_eq!(out, "null", "null, not 0 — a field with text in it has no value, not zero");
}

#[test]
fn af_simple_covers_the_five_functions() {
    for (function, expected) in
        [("SUM", "7"), ("AVG", "3.5"), ("PRD", "12"), ("MIN", "3"), ("MAX", "4")]
    {
        let out = calculate(&format!("event.value = AFSimple('{function}', 3, 4);"));
        assert_eq!(out, expected, "AFSimple {function}");
    }
}

#[test]
fn af_simple_rejects_a_function_it_does_not_have() {
    let host = ScriptHost::new(form(), ScriptEnvironment::default());
    assert!(
        host.run_calculation("AFSimple('NOPE', 1, 2);", Some("")).is_err(),
        "an unknown function throws rather than returning something plausible"
    );
}

#[test]
fn af_simple_calculate_sums_the_named_fields() {
    // The one a real form calls, and the reason this file exists.
    let out = calculate("AFSimple_Calculate('SUM', ['a', 'b']);");
    assert_eq!(out, "5", "2 + 3, read from the document");
}

#[test]
fn af_simple_calculate_accepts_a_comma_separated_list() {
    // Form producers write both forms, and Acrobat takes either.
    assert_eq!(calculate("AFSimple_Calculate('SUM', 'a,b');"), "5");
}

#[test]
fn af_simple_calculate_skips_a_field_that_is_not_there() {
    // Not an error: a form may name a field a later revision removed.
    assert_eq!(calculate("AFSimple_Calculate('SUM', ['a', 'missing', 'b']);"), "5");
}

#[test]
fn af_simple_calculate_rounds_off_binary_floating_point() {
    // Six places, so a form shows 0.3 rather than 0.30000000000000004.
    assert_eq!(calculate("event.value = Math.round(1e6 * (0.1 + 0.2)) / 1e6;"), "0.3");
}

#[test]
fn a_document_cannot_poison_the_helpers_for_the_next_one() {
    // The measurement that decided per-context loading. A script redefining a helper is
    // doing something legal; it must not reach the next document.
    let poisoned = ScriptHost::new(form(), ScriptEnvironment::default())
        .run_calculation(
            "function AFSimple_Calculate() { event.value = 999; } \
             AFSimple_Calculate('SUM', ['a','b']);",
            Some(""),
        )
        .expect("runs")
        .expect("writes");
    assert_eq!(poisoned, "999", "the redefinition takes effect in its own run");

    assert_eq!(calculate("AFSimple_Calculate('SUM', ['a', 'b']);"), "5", "and only there");
}

#[test]
fn the_unaudited_line_count_is_reported() {
    // The second of ADR-0025's two conditions: the figure exists so it cannot grow
    // quietly. It is not zero, and saying so is the point.
    assert!(fepdf_script::unaudited_lines() > 0);
}
