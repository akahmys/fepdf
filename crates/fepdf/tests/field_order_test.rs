//! The order a reader's Tab key takes through a page, and the order a form recalculates
//! its fields in (ROADMAP W-F2-b).
//!
//! Measured 2026-09-26 with `cargo run --release --example tab_and_calc`: of the 3,731
//! pages carrying annotations in the samples and the external corpus, 3,719 have no
//! `/Tabs`, which 12.5.1 leaves to the reader; three files calculate, and in two of them
//! `/CO` is missing although Table 224 requires it.

use fepdf::operation::{Operation, TabOrder};
use fepdf::{IngestionOptions, PageSelection, PdfDocument};

fn sample(name: &str) -> PdfDocument {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples").join(name);
    let bytes = std::fs::read(path).expect("the sample is in this working copy");
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens")
}

/// The fields that calculate, and the order the form says it recalculates them in.
fn calculation(doc: &PdfDocument) -> (Vec<String>, Vec<String>) {
    let form = fepdf::form_of(doc.inner());
    let mut calculating: Vec<String> = form
        .terminal
        .iter()
        .filter(|field| field.calculates)
        .filter_map(|field| field.qualified_name.clone())
        .collect();
    calculating.sort();
    calculating.dedup();
    (calculating, form.calculation_order)
}

/// **The sample's order is read, and every field in it calculates.** This is the reading
/// the window lists and reorders from, so it has to name the same fields the write takes.
#[test]
fn the_calculation_order_is_read_by_name() {
    let (calculating, order) = calculation(&sample("sample_02c.pdf"));
    assert_eq!(order.len(), 7, "sample_02c.pdf's /CO holds seven fields: {order:?}");
    let mut named = order;
    named.sort();
    assert_eq!(named, calculating, "/CO and the fields that calculate disagree");
}

#[test]
fn the_calculation_order_is_written_in_the_order_given() {
    let mut doc = sample("sample_02c.pdf");
    let (_, order) = calculation(&doc);
    let reversed: Vec<String> = order.iter().rev().cloned().collect();
    doc.apply(Operation::SetCalculationOrder(reversed.clone())).expect("the order applies");
    assert_eq!(calculation(&doc).1, reversed, "the order read back is not the one written");
}

/// **Every field that calculates, once each, and nothing else** — refused by name.
#[test]
fn an_order_that_is_not_every_calculating_field_once_is_refused() {
    let doc = sample("sample_02c.pdf");
    let (_, order) = calculation(&doc);
    let first = order[0].clone();

    let mut twice = order.clone();
    twice.push(first.clone());
    let mut unknown = order.clone();
    unknown.push("nothing.by.this.name".to_string());
    let left_out = order[1..].to_vec();

    for (case, given, said) in [
        ("twice", twice, "twice"),
        ("unknown", unknown, "nothing.by.this.name"),
        ("left out", left_out, first.as_str()),
    ] {
        let mut doc = sample("sample_02c.pdf");
        let error = doc.apply(Operation::SetCalculationOrder(given)).expect_err(case);
        assert!(
            error.to_string().contains(said),
            "{case}: the refusal does not say which: {error}"
        );
        assert_eq!(calculation(&doc).1, order, "{case}: a refused order changed the form");
    }
}

/// `/Tabs` is written on the pages named and on no other.
#[test]
fn the_tab_order_is_written_on_the_pages_named() {
    let mut doc = sample("constitution.pdf");
    doc.apply(Operation::SetTabOrder {
        pages: PageSelection::Indices(vec![1]),
        order: TabOrder::Structure,
    })
    .expect("the order applies");

    let tabs = |page: usize| {
        let inner = doc.inner();
        let arena = inner.arena();
        let dict = inner.get_page(page).expect("a page").obj_handle();
        let dict = inner.resolve_to_dict(dict).expect("a dictionary");
        arena
            .dict_entry(dict, arena.name("Tabs"))
            .and_then(|tabs| tabs.as_name())
            .and_then(|name| arena.get_name_str(name))
    };
    assert_eq!(tabs(1).as_deref(), Some("S"), "page 1 was not given structure order");
    assert_eq!(tabs(0), None, "page 0 was not named and was changed");
}

/// **And it survives a save.** `/CO` holds references, so an order that pointed at objects
/// the writer renumbered would read back as nothing.
#[test]
fn the_calculation_order_survives_a_save() {
    let mut doc = sample("sample_02c.pdf");
    let (_, order) = calculation(&doc);
    let reversed: Vec<String> = order.iter().rev().cloned().collect();
    doc.apply(Operation::SetCalculationOrder(reversed.clone())).expect("the order applies");

    let path = std::env::temp_dir().join(format!("fepdf_field_order_{}.pdf", std::process::id()));
    doc.save_with_options(&path, "2.0", &fepdf::SaveOptions::default()).expect("it writes");
    let written = std::fs::read(&path).expect("the output is there");
    let _ = std::fs::remove_file(&path);
    let reopened = PdfDocument::open_with_options(written.into(), &IngestionOptions::default())
        .expect("what this engine wrote, this engine opens");
    assert_eq!(calculation(&reopened).1, reversed, "the order did not survive the save");
}
