//! A new structure element round existing content (14.7.2; WTPDF 8.2.5).
//!
//! **Nothing could make one.** A list item whose label and body are two bare MCIDs needs an
//! `Lbl` and an `LBody`, and a figure's paragraph needs to be its `Caption`; the vocabulary
//! could retag, move and delete elements and not put one between an element and its kids.

use fepdf::{IngestionOptions, Operation, Outcome, PdfDocument, StructElemWrap};
use fepdf_model::Handle;
use fepdf_model::object::Object;

/// A list, element 6, whose item, 7, holds MCIDs 0 and 1 on page 3; and a `Figure`, 8,
/// whose one kid is a paragraph, 9, holding MCID 2.
fn tagged() -> PdfDocument {
    let content = "/P <</MCID 0>> BDC 0 0 5 5 re f EMC /P <</MCID 1>> BDC 10 0 5 5 re f EMC \
                   /P <</MCID 2>> BDC 20 0 5 5 re f EMC\n";
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /Lang (en) \
           /MarkInfo << /Marked true >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
           /StructParents 0 >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /StructTreeRoot /K [6 0 R 8 0 R] \
           /ParentTree << /Nums [0 [7 0 R 7 0 R 9 0 R]] >> >>"
            .to_string(),
        "<< /Type /StructElem /S /L /P 5 0 R /K [7 0 R] >>".to_string(),
        "<< /Type /StructElem /S /LI /P 6 0 R /Pg 3 0 R /K [0 1] >>".to_string(),
        "<< /Type /StructElem /S /Figure /P 5 0 R /Alt (A chart) /K 9 0 R >>".to_string(),
        "<< /Type /StructElem /S /P /P 8 0 R /Pg 3 0 R /K 2 >>".to_string(),
    ]);
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default()).expect("it opens")
}

fn wrap(handle_index: u32, first: usize, count: usize, tag: &str) -> Operation {
    Operation::WrapStructElem(StructElemWrap { handle_index, first, count, tag: tag.into() })
}

/// An entry of an object's dictionary.
fn entry(doc: &PdfDocument, object: Handle<Object>, key: &str) -> Option<Object> {
    let arena = doc.inner().arena();
    let dict = arena.get_object(object).and_then(|o| o.as_dict_handle())?;
    arena.dict_entry(dict, arena.name(key))
}

/// An element's kids, each as written: a reference stays one.
fn kids(doc: &PdfDocument, element: Handle<Object>) -> Vec<Object> {
    let arena = doc.inner().arena();
    match entry(doc, element, "K") {
        Some(Object::Array(a)) => arena.get_array(a).unwrap_or_default(),
        Some(single) => vec![single],
        None => Vec::new(),
    }
}

/// The element named by kid `i` of `element`, and its tag.
fn kid(doc: &PdfDocument, element: Handle<Object>, i: usize) -> (Handle<Object>, String) {
    let arena = doc.inner().arena();
    let handle = kids(doc, element)[i].as_reference().expect("an element, by reference");
    let tag = entry(doc, handle, "S").and_then(|s| s.as_name()).and_then(|n| arena.get_name(n));
    (handle, tag.map(|n| n.as_str().to_string()).unwrap_or_default())
}

/// The elements the parent tree names for MCIDs 0, 1 and 2 on the page.
fn parent_tree(doc: &PdfDocument) -> Vec<Option<Handle<Object>>> {
    let arena = doc.inner().arena();
    let tree = entry(doc, Handle::new(5), "ParentTree").expect("a parent tree").resolve(arena);
    let nums = tree.as_dict_handle().and_then(|d| arena.dict_entry(d, arena.name("Nums")));
    let Some(Object::Array(nums)) = nums.map(|n| n.resolve(arena)) else { panic!("no /Nums") };
    let Some(Object::Array(page)) = arena.get_array(nums).and_then(|n| n.get(1).cloned()) else {
        panic!("no array for the page")
    };
    arena.get_array(page).unwrap_or_default().iter().map(Object::as_reference).collect()
}

/// **An item's label and body each get an element, and everything they held follows**: the
/// item holds `Lbl` and `LBody` in place of its MCIDs, each on the item's page, and the
/// parent tree names them — not the item — for MCIDs 0 and 1. Checkpoint 01 stays sound.
#[test]
fn a_list_item_gets_its_label_and_body() {
    let mut doc = tagged();
    doc.apply(wrap(7, 0, 1, "Lbl")).expect("the label is wrapped");
    doc.apply(wrap(7, 1, 1, "LBody")).expect("the body is wrapped");

    let item = Handle::new(7);
    assert_eq!(kids(&doc, item).len(), 2);
    let (label, tag) = kid(&doc, item, 0);
    assert_eq!(tag, "Lbl");
    let (body, tag) = kid(&doc, item, 1);
    assert_eq!(tag, "LBody");
    for element in [label, body] {
        assert_eq!(entry(&doc, element, "P").and_then(|p| p.as_reference()), Some(item));
        assert_eq!(entry(&doc, element, "Pg").and_then(|p| p.as_reference()), Some(Handle::new(3)));
    }
    assert!(matches!(kids(&doc, label)[..], [Object::Integer(0)]));
    assert!(matches!(kids(&doc, body)[..], [Object::Integer(1)]));
    assert_eq!(parent_tree(&doc), vec![Some(label), Some(body), Some(Handle::new(9))]);

    let report = doc.audit_ua2_report().expect("it audits");
    for condition in ["01-003", "01-004", "01-005"] {
        let outcome: Vec<Outcome> = report
            .findings
            .iter()
            .filter(|f| f.checkpoint == condition)
            .map(|f| f.outcome)
            .collect();
        assert_eq!(outcome, vec![Outcome::Sound], "{condition}");
    }
}

/// **A figure's paragraph becomes its `Caption`**, still the paragraph it was, by reference:
/// the figure's `/K` was that one reference, not an array, and read resolved it would have
/// been written back as the paragraph's dictionary, inline. The new element survives a file.
#[test]
fn a_figure_gets_its_caption() {
    let mut doc = tagged();
    doc.apply(wrap(8, 0, 1, "Caption")).expect("the caption is wrapped");
    let figure = Handle::new(8);
    let (caption, tag) = kid(&doc, figure, 0);
    assert_eq!(tag, "Caption");
    assert_eq!(kids(&doc, caption).first().and_then(Object::as_reference), Some(Handle::new(9)));
    assert_eq!(entry(&doc, Handle::new(9), "P").and_then(|p| p.as_reference()), Some(caption));
    assert_eq!(parent_tree(&doc)[2], Some(Handle::new(9)), "the paragraph's MCID moved");

    let path = std::env::temp_dir().join("fepdf-wrap-caption.pdf");
    doc.save_as_version(&path, "2.0").expect("it writes");
    let bytes = std::fs::read(&path).expect("it is on disk");
    let reopened =
        PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default()).expect("opens");
    let arena = reopened.inner().arena();
    let tag = |dict| {
        let tag = arena.dict_entry(dict, arena.name("S")).and_then(|s| s.as_name());
        tag.and_then(|n| arena.get_name(n)).map(|n| n.as_str().to_string()).unwrap_or_default()
    };
    let caption = arena.all_dict_handles().into_iter().find(|d| tag(*d) == "Caption");
    let caption = caption.expect("the caption did not survive the file");
    let parent = arena.dict_entry(caption, arena.name("P")).map(|p| p.resolve(arena));
    assert_eq!(parent.and_then(|p| p.as_dict_handle()).map(tag).as_deref(), Some("Figure"));
}

/// **What cannot be wrapped is refused, and nothing is written**: no kids, kids that are not
/// there, an object that is no element, and an MCID with no page to be found on.
#[test]
fn what_cannot_be_wrapped_is_refused() {
    let mut doc = tagged();
    assert!(doc.apply(wrap(7, 0, 0, "Lbl")).is_err(), "no kids were wrapped");
    assert!(doc.apply(wrap(7, 1, 2, "Lbl")).is_err(), "a kid past the end was wrapped");
    assert!(doc.apply(wrap(3, 0, 1, "Lbl")).is_err(), "a page's kids were wrapped");
    assert!(doc.apply(wrap(9999, 0, 1, "Lbl")).is_err(), "nothing was wrapped");
    assert_eq!(kids(&doc, Handle::new(7)).len(), 2, "a refused wrap changed the item");

    doc.apply(wrap(5, 0, 2, "Div")).expect("the root's kids are wrapped");
    let (div, tag) = kid(&doc, Handle::new(5), 0);
    assert_eq!((kids(&doc, Handle::new(5)).len(), tag.as_str()), (1, "Div"));
    assert_eq!(entry(&doc, Handle::new(6), "P").and_then(|p| p.as_reference()), Some(div));

    let mut pageless = tagged();
    let arena = pageless.inner().arena();
    let item = arena.get_object(Handle::new(7)).and_then(|o| o.as_dict_handle()).expect("item");
    let mut dict = arena.get_dict(item).expect("a dictionary");
    dict.remove(&arena.name("Pg"));
    arena.set_dict(item, dict);
    assert!(pageless.apply(wrap(7, 0, 1, "Lbl")).is_err(), "an MCID with no page was wrapped");
    assert_eq!(kids(&pageless, Handle::new(7)).len(), 2, "a refused wrap changed the item");
}
