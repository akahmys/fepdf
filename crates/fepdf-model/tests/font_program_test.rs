//! Reaching the program a font draws with.
//!
//! **An earlier entry in the roadmap said this was not reachable at all**, on a
//! measurement that counted 335 font dictionaries with no program. Two things were wrong
//! with it: 36 of those are Type 3 fonts, which have no program by definition, and the
//! field it read is the one `initialize_lifecycle` deliberately releases once the engine
//! has patched the program into `reconstructed_data`. Read through `program`, 291 of the
//! 299 fonts that are not Type 3 answer, and the eight that do not are the ones their
//! documents never embedded.

use fepdf_model::{Handle, Object, document::Document, ingest::IngestionOptions};

/// Every font dictionary in `doc`, by object handle.
fn font_objects(doc: &Document) -> Vec<Handle<Object>> {
    let arena = doc.arena();
    (0..arena.object_count())
        .map(Handle::new)
        .filter(|h| {
            let Some(Object::Dictionary(dh)) = arena.get_object(*h) else { return false };
            let Some(dict) = arena.get_dict(dh) else { return false };
            dict.iter().any(|(k, v)| {
                arena.get_name(*k).is_some_and(|n| n.as_str() == "Type")
                    && v.resolve(arena)
                        .as_name()
                        .and_then(|n| arena.get_name(n))
                        .is_some_and(|n| n.as_str() == "Font")
            })
        })
        .collect()
}

fn sample(name: &str) -> Option<Document> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples").join(name);
    Document::open(std::fs::read(path).ok()?.into(), &IngestionOptions::default()).ok()
}

/// **A font a document embeds has a program a caller can reach.**
#[test]
fn an_embedded_font_answers_with_its_program() {
    let doc = sample("constitution.pdf").expect("the sample opens");
    let with_program = font_objects(&doc)
        .into_iter()
        .filter_map(|h| doc.get_font(h).ok())
        .filter(|font| font.program().is_some_and(|p| p.len() > 4))
        .count();
    assert!(
        with_program >= 9,
        "the document embeds nine fonts and {with_program} answered with a program"
    );
}

/// A Type 3 font draws with content streams and has no program to answer with (9.6.4).
#[test]
fn a_type_three_font_answers_with_nothing() {
    let doc = sample("fugaku.pdf").expect("the sample opens");
    let fonts: Vec<_> =
        font_objects(&doc).into_iter().filter_map(|h| doc.get_font(h).ok()).collect();
    assert!(!fonts.is_empty(), "the sample carries fonts, or this asks nothing");
    assert!(
        fonts.iter().all(|font| font.program().is_none()),
        "a Type 3 font answered with a program it cannot have"
    );
}
