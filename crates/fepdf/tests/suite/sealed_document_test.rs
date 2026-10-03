//! **A document changes only inside `apply`, and all of an operation or none of it lands**
//! (ROADMAP Y-11 and Y-F12).
//!
//! The arena writes through `&self`, so a method that reads could write and nothing in its
//! signature would say so: measured 2026-10-03, rendering a page with no `/Resources`, a
//! dashed line or an annotation appearance with no resources of its own, and reporting a
//! document's actions, each wrote into the document it read. The facade seals the arena
//! once a document is opened; a write while sealed panics in a debug build.

use fepdf::{Document, IngestionOptions, PdfDocument};
use fepdf_model::interpretation::Decision;
use fepdf_model::{Object, PdfError};

/// One page with nothing on it and no `/Resources`, which Table 31 requires.
fn page_without_resources() -> bytes::Bytes {
    fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ])
    .into()
}

/// **A write round `apply` fails a test.** This is the probe the gate holds: a `&self`
/// path that writes into an opened document panics, so every test that takes one fails.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "a sealed arena was written")]
fn a_write_outside_apply_panics() {
    let doc =
        PdfDocument::open_with_options(page_without_resources(), &IngestionOptions::default())
            .expect("the fixture opens");
    let _ = doc.inner().arena().alloc_object(Object::Null);
}

/// **A page with no `/Resources` is given one when it is read, and says so**, so that
/// drawing it writes nothing: it allocated an empty dictionary on every call.
#[test]
fn a_page_with_no_resources_is_given_them_at_load() {
    let doc =
        PdfDocument::open_with_options(page_without_resources(), &IngestionOptions::default())
            .expect("the fixture opens");
    let given = doc.decisions().into_iter().filter(|d| d.clause == "7.7.3.3").count();
    assert_eq!(given, 1, "the repair was not recorded: {:?}", doc.decisions());
    let objects = doc.inner().arena().object_count();
    let _ = doc.extract_text(0).expect("the page reads");
    assert_eq!(doc.inner().arena().object_count(), objects, "reading the page wrote objects");
}

/// **A change that fails is put back whole** (Y-F12): what it allocated, the dictionary it
/// rewrote, the page list it emptied and the decision it recorded.
#[test]
fn a_change_that_fails_leaves_the_document_as_it_was() {
    let mut doc = Document::open(page_without_resources(), &IngestionOptions::default())
        .expect("the fixture opens");
    let arena = doc.arena().clone();
    let page = arena.get_object(doc.pages[0]).and_then(|p| p.as_dict_handle()).expect("a page");
    let (objects, before, decided) =
        (arena.object_count(), arena.get_dict(page), doc.decisions.len());

    let failed: Result<(), PdfError> = doc.change(|doc| {
        let arena = doc.arena();
        let _ = arena.alloc_object(Object::Null);
        let mut dict = arena.get_dict(page).expect("it reads");
        dict.insert(arena.name("Rotate"), Object::Integer(90));
        arena.set_dict(page, dict);
        doc.pages.clear();
        doc.decisions.push(Decision::repaired("0", "a change", "made it"));
        Err(PdfError::internal("failed after writing"))
    });

    assert!(failed.is_err());
    assert_eq!(arena.object_count(), objects, "what the change allocated is still there");
    assert_eq!(arena.get_dict(page), before, "the page keeps what the change wrote into it");
    assert_eq!(doc.pages.len(), 1, "the page list was not put back");
    assert_eq!(doc.decisions.len(), decided, "the change's decision is still recorded");
}

/// **A change that succeeds is kept**, which is the other half of the contract.
#[test]
fn a_change_that_succeeds_is_kept() {
    let mut doc = Document::open(page_without_resources(), &IngestionOptions::default())
        .expect("the fixture opens");
    let objects = doc.arena().object_count();
    doc.change(|doc| {
        let _ = doc.arena().alloc_object(Object::Null);
        Ok(())
    })
    .expect("it succeeds");
    assert_eq!(doc.arena().object_count(), objects + 1);
}
