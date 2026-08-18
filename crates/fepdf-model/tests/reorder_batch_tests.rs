//! Tests for batch page reordering in Document.

use fepdf_model::{Document, Handle, Object, PdfArena};
use std::collections::BTreeMap;

fn create_dummy_doc_with_pages(count: usize) -> Document {
    let arena = PdfArena::new();
    let mut root_dict = BTreeMap::new();
    root_dict.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    let root = arena.alloc_object(Object::Dictionary(arena.alloc_dict(root_dict)));

    let mut doc = Document::new(arena, root, None);
    let mut pages: Vec<Handle<Object>> = Vec::with_capacity(count);

    for _ in 0..count {
        let mut dict = BTreeMap::new();
        dict.insert(doc.arena().name("Type"), Object::Name(doc.arena().name("Page")));
        let handle = doc.arena().alloc_object(Object::Dictionary(doc.arena().alloc_dict(dict)));
        pages.push(handle);
    }

    doc.pages = pages;
    doc
}

#[test]
fn test_reorder_pages_batch_move_to_first() {
    let mut doc = create_dummy_doc_with_pages(6);
    let original_handles: Vec<_> = doc.pages.clone();

    // Move pages at index 1 and 3 (P1, P3) to first (0)
    let range = doc.reorder_pages_batch(&[1, 3], 0).expect("reorder batch");
    assert_eq!(range, 0..2);

    assert_eq!(
        doc.pages,
        vec![
            original_handles[1],
            original_handles[3],
            original_handles[0],
            original_handles[2],
            original_handles[4],
            original_handles[5],
        ]
    );
}

#[test]
fn test_reorder_pages_batch_move_to_last() {
    let mut doc = create_dummy_doc_with_pages(6);
    let original_handles: Vec<_> = doc.pages.clone();

    // Move pages at index 1 and 3 (P1, P3) to last (6)
    let range = doc.reorder_pages_batch(&[1, 3], 6).expect("reorder batch");
    assert_eq!(range, 4..6);

    assert_eq!(
        doc.pages,
        vec![
            original_handles[0],
            original_handles[2],
            original_handles[4],
            original_handles[5],
            original_handles[1],
            original_handles[3],
        ]
    );
}

#[test]
fn test_reorder_pages_batch_move_middle() {
    let mut doc = create_dummy_doc_with_pages(6);
    let original_handles: Vec<_> = doc.pages.clone();

    // Move pages at index 1 and 3 (P1, P3) to index 5 (before P5)
    let range = doc.reorder_pages_batch(&[1, 3], 5).expect("reorder batch");
    assert_eq!(range, 3..5);

    assert_eq!(
        doc.pages,
        vec![
            original_handles[0],
            original_handles[2],
            original_handles[4],
            original_handles[1],
            original_handles[3],
            original_handles[5],
        ]
    );
}
