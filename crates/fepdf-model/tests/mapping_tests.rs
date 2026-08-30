//! Mapping integration tests for fepdf-model.

use fepdf_model::arena::PdfArena;
use fepdf_model::font::{FontMetrics, FontResource, cmap::CMap};
use fepdf_model::object::PdfName;
use fepdf_model::{Handle, Object};
use std::collections::BTreeMap;
use std::sync::Arc;

#[test]
fn test_code_to_cid_mismatch_reproduction() {
    let arena = PdfArena::new();

    let mut enc_cmap = CMap::default();
    let mut mappings_cid = BTreeMap::new();
    mappings_cid.insert(vec![0x65], 100);
    enc_cmap.mappings_cid = Arc::new(mappings_cid);

    let mut tu_cmap = CMap::default();
    let mut mappings = BTreeMap::new();
    mappings.insert(vec![0x65], "e".to_string());
    tu_cmap.mappings = Arc::new(mappings);

    let mut res = FontResource {
        subtype: PdfName::new("Type0"),
        base_font: PdfName::new("TestFont"),
        is_cid_keyed: true,
        encoding: Some(enc_cmap),
        to_unicode: Some(tu_cmap),
        ..FontResource::new_initial(
            PdfName::new("Type0"),
            PdfName::new("TestFont"),
            FontMetrics::default(),
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            &BTreeMap::new(),
            &arena,
            false,
            None,
            None,
        )
    };

    res.build_unified_map();

    let cid = res.unified_map.get("e").copied();
    assert_eq!(
        cid,
        Some(100),
        "Unicode 'e' should map to CID 100 (via Encoding), not character code 0x65"
    );
}

// ---------------------------------------------------------------------------
// `/CIDSystemInfo` — 9.7.3, Table 114 and Table 115.
//
// **These exist because the engine read the wrong object type for four phases and
// nothing went red.** `/Registry` and `/Ordering` are strings; the code asked for names,
// so every conforming file answered `None` — 116 of 116 Type0 fonts across both corpora —
// and the character collection was decided from substrings of `/BaseFont` instead. What
// that cost is in ADR-0041: `fy05.pdf`'s title page, set in Ryumin and declared
// `Adobe-Japan1`, extracted as nothing, while nineteen fonts declaring `Adobe-Korea1` were
// read through the *Japanese* table because their names contain `Gothic`.
//
// Each of these was verified by putting the defect back — `as_string` returned to
// `as_name`, and the CIDFont branch returned to reading no `/CIDSystemInfo` at all.
// ---------------------------------------------------------------------------

/// A CIDFont dictionary declaring `registry`/`ordering` in the object form `wrap` builds.
fn cid_font(
    arena: &PdfArena,
    base_font: &str,
    wrap: fn(&[u8]) -> Object,
    registry: &str,
    ordering: &str,
) -> BTreeMap<Handle<PdfName>, Object> {
    let mut csi = BTreeMap::new();
    csi.insert(arena.name("Registry"), wrap(registry.as_bytes()));
    csi.insert(arena.name("Ordering"), wrap(ordering.as_bytes()));
    csi.insert(arena.name("Supplement"), Object::Integer(6));

    let mut dict = BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Font")));
    dict.insert(arena.name("Subtype"), Object::Name(arena.name("CIDFontType0")));
    dict.insert(arena.name("BaseFont"), Object::Name(arena.name(base_font)));
    dict.insert(arena.name("CIDSystemInfo"), Object::Dictionary(arena.alloc_dict(csi)));
    dict
}

/// Loads `dict` as a font, through a document with nothing else in it.
fn load(arena: PdfArena, dict: &BTreeMap<Handle<PdfName>, Object>) -> FontResource {
    let root = arena.alloc_object(Object::Null);
    let doc = fepdf_model::document::Document::new(arena, root, None);
    FontResource::load(dict, &doc).expect("the font dictionary loads")
}

/// The literal string form, `(Adobe) (Japan1)`, which `samples/fy05.pdf` writes.
#[test]
fn a_cid_font_reads_the_collection_its_own_dictionary_declares() {
    let arena = PdfArena::new();
    let dict = cid_font(
        &arena,
        "RyuminPr6N-Heavy",
        |b| Object::String(b.to_vec().into()),
        "Adobe",
        "Japan1",
    );
    let res = load(arena, &dict);
    assert_eq!(res.cid_registry.as_deref(), Some("Adobe"), "/Registry is a string (Table 114)");
    assert_eq!(res.cid_ordering.as_deref(), Some("Japan1"), "/Ordering is a string (Table 114)");
}

/// The hexadecimal form is the same object type (7.3.4.3), and `intel_sdm.pdf` writes it.
#[test]
fn a_hexadecimal_string_declares_the_collection_just_as_a_literal_one_does() {
    let arena = PdfArena::new();
    let dict =
        cid_font(&arena, "RyuminPr6N-Heavy", |b| Object::Hex(b.to_vec().into()), "Adobe", "Japan1");
    let res = load(arena, &dict);
    assert_eq!(res.cid_ordering.as_deref(), Some("Japan1"));
}

/// **A collection with no table is reported, not approximated.** `Adobe-Japan2` is a real
/// registered collection and one Adobe publishes no CID-to-Unicode file for, so it stands
/// here for anything outside the five that are fetched. Reading such a font through
/// Adobe-Japan1 — which is what the `/BaseFont` guess did to nineteen Korean fonts of the
/// external corpus — would give a Japanese character for every CID with nothing to say it
/// was a guess.
#[test]
fn a_collection_this_engine_carries_no_table_for_is_declined_and_recorded() {
    let arena = PdfArena::new();
    let dict =
        cid_font(&arena, "HeiseiMin-W3", |b| Object::String(b.to_vec().into()), "Adobe", "Japan2");
    let res = load(arena, &dict);

    assert_eq!(res.cid_ordering.as_deref(), Some("Japan2"));
    assert!(
        res.collection_map.is_none(),
        "a table was loaded for Adobe-Japan2, which Adobe publishes no UCS2 file for"
    );
    let declined = res
        .decisions
        .iter()
        .find(|d| d.clause == "9.7.3")
        .expect("declining a collection is a decision, not a silence");
    assert!(
        declined.found.contains("Adobe-Japan2"),
        "the decision must name the collection, or it says only that something failed: {}",
        declined.found
    );
}

/// `Identity` is a statement about indexing, not about characters (9.7.4.2), so it leaves
/// the name heuristic as the only thing to go on — which is the case it was written for.
/// Seventy-five fonts of the two corpora are in it.
#[test]
fn identity_ordering_leaves_the_name_heuristic_in_place() {
    if fepdf_model::resources::locate(fepdf_model::resources::Resource::CidToUnicode).is_none() {
        eprintln!("skipping: Adobe's CID-to-Unicode tables are not present");
        return;
    }
    let arena = PdfArena::new();
    let dict = cid_font(
        &arena,
        "KozMinPr6N-Regular",
        |b| Object::String(b.to_vec().into()),
        "Adobe",
        "Identity",
    );
    let res = load(arena, &dict);
    assert_eq!(res.cid_ordering.as_deref(), Some("Identity"));
    assert!(
        res.collection_map.is_some(),
        "a font declaring Identity and named Koz should still reach Adobe-Japan1"
    );
    assert!(
        res.decisions.iter().all(|d| d.clause != "9.7.3"),
        "declaring Identity is not declaring a collection this engine cannot read"
    );
}

/// **The declared collection decides which table is read, and CID 16128 proves which.**
///
/// Adobe-Japan1 puts `\u{30d5}` there and Adobe-Korea1 puts `\u{cdce}`, so a font that
/// declares Korea1 and comes back with the katakana is one that had the Japanese table
/// applied to it — which is exactly what happened to nineteen fonts of the external
/// corpus before [ADR-0041], and what "no table at all" replaced until the other four
/// collections were read. Asserting on a CID whose two readings differ tests the *wiring*
/// rather than the table's contents: a check that only asked whether some character came
/// back would pass on the wrong table.
///
/// Skips without Adobe's CID-to-Unicode tables, which are fetched rather than vendored.
///
/// [ADR-0041]: ../../../docs/adr/0041-a-character-collection-is-declared-not-guessed.md
#[test]
fn the_collection_a_font_declares_is_the_table_that_names_its_cids() {
    if fepdf_model::resources::locate(fepdf_model::resources::Resource::CidToUnicode).is_none() {
        eprintln!("skipping: Adobe's CID-to-Unicode tables are not present");
        return;
    }
    // CID 16128, as the two bytes an Identity-H font would draw it with.
    let cid = [0x3F, 0x00];
    // Read out of Adobe's own files rather than recalled: a first draft of this test put
    // GB1's at U+9ECF from memory and the assertion caught it.
    for (ordering, expected) in [
        ("Japan1", "\u{30d5}"),
        ("Korea1", "\u{cdce}"),
        ("GB1", "\u{76e6}"),
        ("CNS1", "\u{9b39}"),
        ("KR", "\u{6a38}"),
    ] {
        let arena = PdfArena::new();
        let dict = cid_font(
            &arena,
            "AdobeGothicStd-Bold",
            |b| Object::String(b.to_vec().into()),
            "Adobe",
            ordering,
        );
        let res = load(arena, &dict);
        let table = res.collection_map.as_ref().unwrap_or_else(|| {
            panic!("no table loaded for Adobe-{ordering}, which is one of the five fetched")
        });
        assert_eq!(
            table.map(&cid).as_deref(),
            Some(expected),
            "Adobe-{ordering} named CID 16128 with the wrong table"
        );
    }
}
