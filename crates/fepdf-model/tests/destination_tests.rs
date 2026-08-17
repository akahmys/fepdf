//! Destinations (12.3.2) and the name tree that holds them (7.9.6).

use fepdf_model::destination::{Destination, Lookup, NamedDestinations, Target, View};
use fepdf_model::{Object, PdfArena};
use std::collections::BTreeMap;

/// A page to point at, and the arena to build in.
fn arena_with_page() -> (PdfArena, fepdf_model::Handle<Object>) {
    let arena = PdfArena::new();
    let mut page = BTreeMap::new();
    page.insert(arena.name("Type"), Object::Name(arena.name("Page")));
    let page = arena.alloc_object(Object::Dictionary(arena.alloc_dict(page)));
    (arena, page)
}

/// `[page /Form args...]`, as Table 151 writes it.
fn destination(arena: &PdfArena, page: fepdf_model::Handle<Object>, rest: Vec<Object>) -> Object {
    let mut array = vec![Object::Reference(page)];
    array.extend(rest);
    Object::Array(arena.alloc_array(array))
}

/// All eight forms of Table 151, including the `null` it permits in place of a number.
#[test]
fn every_form_table_151_defines_reads_back() {
    let (arena, page) = arena_with_page();
    let name = |s: &str| Object::Name(arena.name(s));
    let read = |rest: Vec<Object>| {
        Destination::read(&destination(&arena, page, rest), &arena).expect("a destination")
    };

    let d = read(vec![name("XYZ"), Object::Integer(36), Object::Real(594.5), Object::Null]);
    assert_eq!(d.target, Target::Page(page));
    // `null` is not zero and not a default: Table 151 defines it as "keep the current
    // value", which only the viewer knows. `intel_sdm.pdf` writes `/XYZ null null null`.
    assert_eq!(d.view, View::Xyz { left: Some(36.0), top: Some(594.5), zoom: None });

    assert_eq!(read(vec![name("Fit")]).view, View::Fit);
    assert_eq!(
        read(vec![name("FitH"), Object::Integer(720)]).view,
        View::FitH { top: Some(720.0) }
    );
    assert_eq!(read(vec![name("FitH"), Object::Null]).view, View::FitH { top: None });
    assert_eq!(read(vec![name("FitV"), Object::Integer(0)]).view, View::FitV { left: Some(0.0) });
    assert_eq!(
        read(vec![
            name("FitR"),
            Object::Integer(0),
            Object::Integer(1),
            Object::Integer(2),
            Object::Integer(3),
        ])
        .view,
        View::FitR { left: 0.0, bottom: 1.0, right: 2.0, top: 3.0 }
    );
    assert_eq!(read(vec![name("FitB")]).view, View::FitB);
    assert_eq!(read(vec![name("FitBH"), Object::Real(1.5)]).view, View::FitBH { top: Some(1.5) });
    assert_eq!(read(vec![name("FitBV"), Object::Null]).view, View::FitBV { left: None });

    // What a report prints is the name the table gives, not a Rust identifier.
    assert_eq!(View::FitBH { top: None }.as_name(), "FitBH");
}

/// `/FitR` is the one form with no null permitted, because a rectangle with a missing
/// edge names no region. A form the table does not define is not read at all — its name
/// determines how many numbers follow, so keeping the name would keep a label and throw
/// the destination away.
#[test]
fn a_destination_that_cannot_be_read_is_not_guessed_at() {
    let (arena, page) = arena_with_page();
    let name = |s: &str| Object::Name(arena.name(s));
    let read = |rest: Vec<Object>| Destination::read(&destination(&arena, page, rest), &arena);

    assert!(read(vec![name("FitR"), Object::Integer(0), Object::Integer(1)]).is_none());
    assert!(
        read(vec![
            name("FitR"),
            Object::Integer(0),
            Object::Null,
            Object::Integer(2),
            Object::Integer(3)
        ])
        .is_none()
    );
    assert!(read(vec![name("FitEverything")]).is_none(), "not a form Table 151 defines");
    assert!(read(vec![]).is_none(), "no form at all");

    // The first element has to be a page, and a page is an indirect reference here.
    let direct = Object::Array(arena.alloc_array(vec![Object::Null, name("Fit")]));
    assert!(Destination::read(&direct, &arena).is_none());

    // A remote destination names a page by number instead, which is legal and is not
    // this document's page (12.6.4.3).
    let remote = Object::Array(arena.alloc_array(vec![Object::Integer(3), name("Fit")]));
    let d = Destination::read(&remote, &arena).expect("a remote destination");
    assert_eq!(d.target, Target::RemotePage(3));
}

/// 12.3.2.3 also allows `<< /D array >>`, which `intel_sdm.pdf` uses for all 279,501 of
/// its named destinations — so this is the form the corpus exercises most.
#[test]
fn the_dictionary_form_reads_the_same_as_the_array() {
    let (arena, page) = arena_with_page();
    let array = destination(&arena, page, vec![Object::Name(arena.name("Fit"))]);
    let mut wrapper = BTreeMap::new();
    wrapper.insert(arena.name("D"), array.clone());
    let wrapped = Object::Dictionary(arena.alloc_dict(wrapper));

    assert_eq!(Destination::read(&wrapped, &arena), Destination::read(&array, &arena));
    assert!(Destination::read(&wrapped, &arena).is_some());
}

/// A catalogue carrying both of 12.3.2.3's forms, so the two lookups can be told apart.
fn both_forms() -> (PdfArena, BTreeMap<fepdf_model::Handle<fepdf_model::PdfName>, Object>) {
    let (arena, page) = arena_with_page();
    let fit = |a: &PdfArena| destination(a, page, vec![Object::Name(a.name("Fit"))]);

    // The 1.1 form: the catalogue's /Dests, keyed by name.
    let mut dests = BTreeMap::new();
    dests.insert(arena.name("chapter.one"), fit(&arena));
    dests.insert(arena.name("broken"), Object::Integer(7)); // not a destination
    let dests = arena.alloc_dict(dests);

    // The 1.2 form: a two-level name tree under /Names, with a literal-string key in one
    // leaf and a hex-string key in the other — `unicode_16.pdf` writes hex.
    let leaf = |keys: Vec<Object>| {
        let mut d = BTreeMap::new();
        d.insert(arena.name("Names"), Object::Array(arena.alloc_array(keys)));
        // /Limits deliberately wrong: it excludes the keys the leaf actually holds.
        d.insert(
            arena.name("Limits"),
            Object::Array(
                arena.alloc_array(vec![Object::String("zzz".into()), Object::String("zzz".into())]),
            ),
        );
        Object::Reference(arena.alloc_object(Object::Dictionary(arena.alloc_dict(d))))
    };
    let first = leaf(vec![Object::String("G100066".into()), fit(&arena)]);
    let second = leaf(vec![Object::Hex("G100229".into()), fit(&arena)]);
    let mut root = BTreeMap::new();
    root.insert(arena.name("Kids"), Object::Array(arena.alloc_array(vec![first, second])));
    let root = arena.alloc_dict(root);
    let mut names = BTreeMap::new();
    names.insert(arena.name("Dests"), Object::Dictionary(root));
    let names = arena.alloc_dict(names);

    let mut catalog = BTreeMap::new();
    catalog.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    catalog.insert(arena.name("Dests"), Object::Dictionary(dests));
    catalog.insert(arena.name("Names"), Object::Dictionary(names));
    (arena, catalog)
}

/// Both forms are collected, and `/Limits` is not believed over the entries themselves.
#[test]
fn both_forms_are_collected_and_limits_are_not_trusted() {
    let (arena, catalog) = both_forms();
    let named = NamedDestinations::collect(&arena, &catalog);

    assert_eq!(named.by_name.len(), 1, "the 1.1 dictionary");
    assert!(named.by_name.contains_key("chapter.one"));
    // `/Limits` on both leaves says the keys are "zzz". They are not, and a reader that
    // believed the index over the data would find nothing.
    assert_eq!(named.by_string.len(), 2, "the 1.2 name tree, through /Kids");
    assert!(named.by_string.contains_key(b"G100066".as_slice()), "a literal-string key");
    assert!(named.by_string.contains_key(b"G100229".as_slice()), "a hex-string key");
    assert_eq!(named.len(), 3);
    assert_eq!(named.unreadable, 1, "the /Dests entry that is not a destination");
}

/// A name is answered from the dictionary and a string from the tree, and neither
/// answers the other. 12.3.2.3 puts them in different places; the corpus supplies one
/// file of each kind and no file that mixes them.
#[test]
fn a_name_and_a_string_are_looked_up_in_different_places() {
    let (arena, catalog) = both_forms();
    let named = NamedDestinations::collect(&arena, &catalog);

    let by_name = Object::Name(arena.name("chapter.one"));
    assert!(matches!(named.resolve(&by_name, &arena), Lookup::Named(_)));

    let by_string = Object::String("G100066".into());
    assert!(matches!(named.resolve(&by_string, &arena), Lookup::Named(_)));

    // The same text, looked up the other way round, is not found — and is reported as a
    // link that goes nowhere rather than quietly succeeding.
    let crossed = Object::String("chapter.one".into());
    assert_eq!(
        named.resolve(&crossed, &arena),
        Lookup::Dangling("chapter.one".into()),
        "a string must not be answered by the 1.1 name dictionary"
    );
    let crossed = Object::Name(arena.name("G100066"));
    assert_eq!(named.resolve(&crossed, &arena), Lookup::Dangling("G100066".into()));
}

/// The report's reason for existing: a reference nothing declares, named.
#[test]
fn a_reference_to_nothing_is_reported_with_its_name() {
    let (arena, catalog) = both_forms();
    let named = NamedDestinations::collect(&arena, &catalog);

    // `intel_sdm.pdf` references `(G3.7717)` three times and declares it nowhere, in a
    // 5,000-page manual. This is that, in miniature.
    let missing = Object::String("G3.7717".into());
    assert_eq!(named.resolve(&missing, &arena), Lookup::Dangling("G3.7717".into()));

    // An array needs no lookup at all.
    let page = arena.alloc_object(Object::Null);
    let inline = Object::Array(
        arena.alloc_array(vec![Object::Reference(page), Object::Name(arena.name("Fit"))]),
    );
    assert!(matches!(named.resolve(&inline, &arena), Lookup::Inline(_)));

    // And something that is not a destination at all is neither resolved nor dangling.
    assert_eq!(named.resolve(&Object::Integer(4), &arena), Lookup::Unreadable);
}

/// A `/Kids` cycle terminates. Nothing in 7.9.6 forbids one, and a name tree is read by
/// following references, so the bound is the only thing that stops it.
#[test]
fn a_cyclic_name_tree_terminates() {
    let arena = PdfArena::new();
    let placeholder = arena.alloc_object(Object::Null);
    let mut node = BTreeMap::new();
    node.insert(
        arena.name("Kids"),
        Object::Array(arena.alloc_array(vec![Object::Reference(placeholder)])),
    );
    let node = arena.alloc_dict(node);
    // The kid is the node itself.
    arena.set_object(placeholder, Object::Dictionary(node));

    let mut names = BTreeMap::new();
    names.insert(arena.name("Dests"), Object::Dictionary(node));
    let mut catalog = BTreeMap::new();
    catalog.insert(arena.name("Names"), Object::Dictionary(arena.alloc_dict(names)));

    let named = NamedDestinations::collect(&arena, &catalog);
    assert!(named.by_string.is_empty(), "a cycle holds no destinations, and does not hang");
}
