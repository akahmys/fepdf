//! Integration tests for PDF Object Representation & PdfArena Handle Invariants

use ferruginous_core::arena::PdfArena;
use ferruginous_core::object::Object;
use std::collections::BTreeMap;

#[test]
fn test_object_reference_resolution() {
    let arena = PdfArena::new();
    let num_obj = arena.alloc_object(Object::Integer(2026));
    
    let resolved = arena.get_object(num_obj);
    assert_eq!(resolved, Some(Object::Integer(2026)));
}

#[test]
fn test_arena_intern_name_deduplication() {
    let arena = PdfArena::new();
    let h1 = arena.name("Helvetica");
    let h2 = arena.name("Helvetica");
    let h3 = arena.name("Times-Roman");

    assert_eq!(h1, h2);
    assert_ne!(h1, h3);
}

#[test]
fn test_dictionary_allocation_and_traversal() {
    let arena = PdfArena::new();
    let mut dict = BTreeMap::new();
    
    let key_type = arena.name("Type");
    let val_type = Object::Name(arena.name("Catalog"));
    dict.insert(key_type, val_type);

    let key_ver = arena.name("Version");
    let val_ver = Object::Real(2.0);
    dict.insert(key_ver, val_ver);

    let dict_handle = arena.alloc_dict(dict);
    let retrieved = arena.get_dict(dict_handle).unwrap();

    assert_eq!(retrieved.len(), 2);
    assert_eq!(retrieved.get(&key_ver), Some(&Object::Real(2.0)));
}
