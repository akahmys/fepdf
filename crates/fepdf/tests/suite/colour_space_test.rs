//! A colour space the engine cannot reach is recorded, and draws what it drew before.
//!
//! **Measured 2026-09-06, before this existed.** `/Frobnicate cs 1 0 0 sc` filled red,
//! byte for byte the same image as `/DeviceRGB cs 1 0 0 sc`, and `/Frobnicate cs 0 1 1 0
//! sc` filled the CMYK red — the *operand count* decided the colour model and nothing
//! anywhere said so. `CODING.md`'s Rule 5 section names that exact failure as the one a
//! catch-all must not produce: "a catch-all turns 'unsupported colour space' into
//! 'silently renders black'".
//!
//! `silent_branches.py` reads 0 and cannot see this: it finds wildcard arms over a
//! numeric value read from a file, and a colour space that resolves to nothing is not an
//! arm at all. Rule 20 is the ground, and the ground is wider than the counter.

use fepdf::PdfDocument;

use fepdf_fixtures::assemble;

fn page_drawing(content: &str) -> PdfDocument {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    PdfDocument::open(bytes::Bytes::from(assemble(&objects))).expect("the fixture opens")
}

/// Interpreting the page is what raises the decision, so the page is interpreted.
fn decisions_for(content: &str) -> Vec<fepdf::Decision> {
    let doc = page_drawing(content);
    let _ = doc.extract_text(0);
    doc.decisions()
}

fn unknown_space_decisions(content: &str) -> Vec<fepdf::Decision> {
    decisions_for(content).into_iter().filter(|d| d.clause == "8.6.3").collect()
}

/// A name that is neither a device family nor a `/ColorSpace` entry is recorded.
#[test]
fn a_colour_space_that_names_nothing_is_recorded() {
    let found = unknown_space_decisions("/Frobnicate cs 1 0 0 sc 10 10 100 100 re f");
    assert_eq!(found.len(), 1, "one operand, one record: {found:?}");
    assert!(found[0].found.contains("Frobnicate"), "naming what it was given: {}", found[0].found);
    assert!(
        found[0].action.contains("how many operands"),
        "and what it did instead: {}",
        found[0].action
    );
}

/// A device family is not recorded, whichever one it is.
///
/// Without this, a recorder that fired on every `cs` would pass the test above and bury
/// the case it exists for under one line per colour change.
#[test]
fn a_device_family_is_not_recorded() {
    for space in ["DeviceGray", "DeviceRGB", "DeviceCMYK", "G", "RGB", "CMYK"] {
        let found = unknown_space_decisions(&format!("/{space} cs 0 sc 10 10 100 100 re f"));
        assert!(found.is_empty(), "/{space} is a device family: {found:?}");
    }
}

/// The stroking form records too, and says which operator it was.
#[test]
fn the_stroking_operator_records_under_its_own_name() {
    let found = unknown_space_decisions("/Frobnicate CS 1 0 0 SC 10 10 m 100 100 l S");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].found.contains("operator CS"), "{}", found[0].found);
}

/// **What is drawn does not change.** This records; it does not refuse.
///
/// The operand count often produces the right colour — `/Frobnicate cs 1 0 0 sc` is the
/// same red `/DeviceRGB` gives — and often right is exactly what a silent acceptance
/// looks like. Changing the colour is a separate decision from saying that a guess was
/// made, and only the second is taken here.
#[test]
fn recording_does_not_change_the_colour() {
    let known = decisions_for("/DeviceRGB cs 1 0 0 sc 10 10 100 100 re f");
    let unknown = decisions_for("/Frobnicate cs 1 0 0 sc 10 10 100 100 re f");

    assert!(known.iter().all(|d| d.clause != "8.6.3"));
    assert_eq!(unknown.iter().filter(|d| d.clause == "8.6.3").count(), 1);

    // The pages differ by one decision and by nothing else: same operators, same operand
    // counts, so the same fill reaches the backend. The image comparison that establishes
    // this is in the commit; here the claim is that the record is the only difference.
    assert_eq!(
        known.iter().filter(|d| d.clause != "8.6.3").count(),
        unknown.iter().filter(|d| d.clause != "8.6.3").count(),
        "nothing else about the page changed"
    );
}
