//! Reading one entry out of a dictionary, once.
//!
//! **Every accessor here resolves.** 7.3.10 lets any object in a file be written as an
//! indirect reference, so `/V 4 0 R` and `/V 4` say the same thing, and a reader that
//! only understands the second answers `None` about a conforming file.
//!
//! This module exists because that had happened. `decrypt.rs` and `encryption.rs` read
//! the same `/Encrypt` dictionary through two sets of private helpers that disagreed:
//! six behaviours across twelve functions, of which `integer` followed one level of
//! indirection in one file and none in the other. Measured on 2026-09-05, against a
//! file whose entries were written as references and which the engine decrypts
//! perfectly:
//!
//! | Written as a reference | Reported |
//! | :--- | :--- |
//! | `/Filter` | `(absent)` |
//! | `/V`, `/R`, `/Length` | `—`, and the verdict `UNSUPPORTED` |
//! | `/EncryptMetadata false` | **`true`** — the opposite, through `.unwrap_or(true)` |
//!
//! **Text is decoded, not assumed to be UTF-8.** `text_at` uses the 7.9.2.2 reader; the
//! copy it replaces used `String::from_utf8_lossy`, so a `/Desc` written as UTF-16BE —
//! which is what a producer writing Japanese must do — read back as
//! `��f�S�S0U0�0_m�N� . p d f`.
//!
//! **The other crates read through it too.** [`entry`], [`name_in`] and [`items`] take the
//! object a dictionary is written as — a reference or a dictionary — because that is what
//! `fepdf-audit`'s audits hold, and they kept a copy of each until ROADMAP Y-3.

use crate::arena::PdfArena;
use crate::handle::Handle;
use crate::object::{Object, PdfName};
use std::collections::BTreeMap;

/// What the arena calls a dictionary.
pub type Dict = BTreeMap<Handle<PdfName>, Object>;

/// The value at `key`, with any chain of references followed.
///
/// Every accessor below is this plus a type test. Widened for `function` and `mesh`, which
/// read an entry before knowing its type and each carried a copy of this.
pub(crate) fn entry_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<Object> {
    Some(dict.get(&arena.name(key))?.resolve(arena))
}

/// The dictionary `object` is or refers to.
///
/// A stream is accepted, because a stream *is* a dictionary with data attached (7.3.8)
/// and every caller of this wants the dictionary.
pub fn dict_of(arena: &PdfArena, object: &Object) -> Option<Dict> {
    arena.get_dict(object.resolve(arena).as_dict_handle()?)
}

/// The value at `key` in the dictionary `object` is or refers to, with references followed.
///
/// Read through the arena in place: the audits ask this of every font and element in a
/// file, and [`dict_of`] would copy the whole dictionary to answer one key.
pub fn entry(arena: &PdfArena, object: &Object, key: &str) -> Option<Object> {
    let handle = object.resolve(arena).as_dict_handle()?;
    arena.dict_entry(handle, arena.name(key)).map(|value| value.resolve(arena))
}

/// The name at `key` in the dictionary `object` is or refers to, without its solidus.
pub fn name_in(arena: &PdfArena, object: &Object, key: &str) -> Option<String> {
    name_of(arena, &entry(arena, object, key)?)
}

/// The items of the array at `key`, each resolved; nothing when it is not an array.
pub fn items(arena: &PdfArena, object: &Object, key: &str) -> Vec<Object> {
    match entry(arena, object, key) {
        Some(Object::Array(array)) => {
            arena.get_array(array).unwrap_or_default().iter().map(|i| i.resolve(arena)).collect()
        }
        _ => Vec::new(),
    }
}

/// The entries of one category of a resource dictionary — `/Font`, `/XObject`,
/// `/Properties` — by resource name, each with the object that holds it.
///
/// An entry written directly has no object and is left out: loading gives a direct
/// `/Font` resource an object of its own (ROADMAP W-E2c), and the audits and the
/// artifact marking that call this compare objects. Moved here from `fepdf-doc`'s font
/// audit in Y-4, so that an operation does not import from an audit.
pub fn names_in(
    arena: &PdfArena,
    resources: Handle<Dict>,
    category: &str,
) -> BTreeMap<String, Handle<Object>> {
    arena
        .dict_entry(resources, arena.name(category))
        .and_then(|f| f.resolve(arena).as_dict_handle())
        .and_then(|f| arena.get_dict(f))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(key, value)| Some((arena.get_name_str(key)?, value.as_reference()?)))
        .collect()
}

/// The text `object` is or refers to, decoded as 7.9.2.2 defines.
///
/// Four copies of this existed — `actions::text_of`, `signature::text_of`,
/// `interactive::string_of` and the body of `text_at` — and one of them did **not**
/// resolve: `actions::text_of` took `arena` and wrote `let _ = arena;`, requiring every
/// caller to resolve first. All four of its callers did, so nothing was wrong today; a
/// fifth that did not would have read `None` from a string that was there. That is
/// [ADR-0063](../../../docs/adr/0063-one-set-of-accessors-because-two-disagreed-about-one-dictionary.md)'s
/// finding, and this is the file that record created.
pub(crate) fn text_of(arena: &PdfArena, object: &Object) -> Option<String> {
    match object.resolve(arena) {
        Object::Text(t) => Some(t),
        Object::String(b) | Object::Hex(b) => Some(crate::refine::text::recover_string(&b)),
        _ => None,
    }
}

/// The name `object` is or refers to, without its leading solidus.
pub(crate) fn name_of(arena: &PdfArena, object: &Object) -> Option<String> {
    match object.resolve(arena) {
        Object::Name(h) => arena.get_name_str(h),
        _ => None,
    }
}

/// Every number in the array at `key`, or `None` if any entry is not one.
///
/// All-or-nothing on purpose: the callers are `/Domain`, `/Range`, `/C0`, `/Decode` and
/// the mesh dictionaries, where a partial list is not a shorter list but a different
/// function. Two byte-identical copies of this lived in `function/mod.rs` and
/// `graphics/mesh.rs`.
pub(crate) fn numbers_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<Vec<f64>> {
    let items = array_of(arena, dict.get(&arena.name(key)))?;
    items.iter().map(|item| item.resolve(arena).as_f64()).collect()
}

/// The dictionary at `key`.
pub(crate) fn dict_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<Dict> {
    dict_of(arena, dict.get(&arena.name(key))?)
}

/// The array `object` is or refers to.
pub(crate) fn array_of(arena: &PdfArena, object: Option<&Object>) -> Option<Vec<Object>> {
    arena.get_array(object?.resolve(arena).as_array()?)
}

/// The array at `key`.
pub(crate) fn array_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<Vec<Object>> {
    array_of(arena, dict.get(&arena.name(key)))
}

/// The integer at `key`.
pub(crate) fn integer_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<i64> {
    entry_at(arena, dict, key)?.as_integer()
}

/// The boolean at `key`.
///
/// Callers of this default when it is `None`, which is why it must not answer `None` for
/// a value that is there: `/EncryptMetadata 113 0 R` pointing at `false` reported `true`.
pub(crate) fn boolean_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<bool> {
    match entry_at(arena, dict, key)? {
        Object::Boolean(v) => Some(v),
        _ => None,
    }
}

/// The name at `key`, without its leading solidus.
pub(crate) fn name_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<String> {
    arena.get_name_str(entry_at(arena, dict, key)?.as_name()?)
}

/// The raw bytes of the string at `key`, in either string syntax.
///
/// Bytes and not text: `/O` and `/U` are the arguments to a hash, not something to read.
pub(crate) fn bytes_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<Vec<u8>> {
    match entry_at(arena, dict, key)? {
        Object::String(b) | Object::Hex(b) => Some(b.to_vec()),
        _ => None,
    }
}

/// The text string at `key`, decoded as 7.9.2.2 defines one.
pub(crate) fn text_at(arena: &PdfArena, dict: &Dict, key: &str) -> Option<String> {
    text_of(arena, &entry_at(arena, dict, key)?)
}

#[cfg(test)]
mod chains {
    //! 7.3.10 lets any value be written as a reference, so every accessor follows one.

    use super::*;

    /// An arena holding `key` -> a chain of `depth` references -> the value `make` builds.
    ///
    /// `make` takes the arena because a handle belongs to the one that issued it. The
    /// first version of this built a `Object::Name` in a second arena, and the two
    /// handles collided: `name_at` answered `"F"`, the key's own name, with no error
    /// anywhere. Worth a sentence, because it is the failure mode `Handle` has.
    fn chained(key: &str, make: impl Fn(&PdfArena) -> Object, depth: usize) -> (PdfArena, Dict) {
        let arena = PdfArena::new();
        let mut current = make(&arena);
        for _ in 0..depth {
            current = Object::Reference(arena.alloc_object(current));
        }
        let mut dict = Dict::new();
        dict.insert(arena.name(key), current);
        (arena, dict)
    }

    /// Every typed accessor reads through a chain, not just past one reference.
    ///
    /// **One hop is the case the defect had half-right.** `decrypt.rs` followed exactly
    /// one level and `encryption.rs` followed none, so the two disagreed about the same
    /// `/Encrypt` dictionary. Two hops is what tells a real chain-follower from a
    /// one-level one, and neither of the originals passed it.
    #[test]
    fn every_accessor_follows_a_chain_and_not_merely_one_reference() {
        for depth in [0_usize, 1, 2, 5] {
            let (a, d) = chained("N", |_| Object::Integer(42), depth);
            assert_eq!(integer_at(&a, &d, "N"), Some(42), "integer at depth {depth}");

            let (a, d) = chained("B", |_| Object::Boolean(false), depth);
            assert_eq!(boolean_at(&a, &d, "B"), Some(false), "boolean at depth {depth}");

            let (a, d) = chained("F", |a| Object::Name(a.name("Standard")), depth);
            assert_eq!(name_at(&a, &d, "F").as_deref(), Some("Standard"), "name at {depth}");

            let (a, d) = chained("S", |_| Object::String(bytes::Bytes::from_static(b"xy")), depth);
            assert_eq!(bytes_at(&a, &d, "S"), Some(b"xy".to_vec()), "bytes at depth {depth}");
        }
    }

    /// A boolean that is there reads as itself, because the callers of this default.
    ///
    /// `encrypt_metadata` is `boolean_at(..).unwrap_or(true)`, so an accessor that
    /// answers `None` for a value it could not read reports the **opposite** of what the
    /// document says. Measured before this module existed: `/EncryptMetadata 113 0 R`
    /// pointing at `false` was reported as `true`.
    #[test]
    fn a_boolean_behind_a_reference_is_not_left_to_a_default() {
        let (a, d) = chained("EncryptMetadata", |_| Object::Boolean(false), 1);
        assert!(
            !boolean_at(&a, &d, "EncryptMetadata").unwrap_or(true),
            "the default must not stand in for a value that is there"
        );
    }

    /// A text string is decoded as 7.9.2.2 defines one, not assumed to be UTF-8.
    ///
    /// The copy this replaces used `String::from_utf8_lossy`, so a `/Desc` written as
    /// UTF-16BE — which is what a producer writing Japanese must do — came back as
    /// `��f�S�S0U0...`.
    #[test]
    fn text_is_decoded_rather_than_assumed_to_be_utf8() {
        let mut raw = vec![0xFE, 0xFF];
        for unit in "暗号化された添付.pdf".encode_utf16() {
            raw.extend_from_slice(&unit.to_be_bytes());
        }
        let (a, d) = chained("UF", |_| Object::String(bytes::Bytes::from(raw.clone())), 1);
        assert_eq!(text_at(&a, &d, "UF").as_deref(), Some("暗号化された添付.pdf"));
    }

    /// A stream is a dictionary with data attached (7.3.8), and callers want the
    /// dictionary. The three `as_dict` copies this replaces refused one.
    #[test]
    fn a_stream_is_read_as_the_dictionary_it_is() {
        let arena = PdfArena::new();
        let mut inner = Dict::new();
        inner.insert(arena.name("Type"), Object::Name(arena.name("EmbeddedFile")));
        let data =
            std::sync::Arc::new(crate::object::SublimatedData::Commands { items: Vec::new() });
        let stream = Object::Stream(arena.alloc_dict(inner), data);

        let mut outer = Dict::new();
        outer.insert(arena.name("EF"), Object::Reference(arena.alloc_object(stream)));

        let read = dict_at(&arena, &outer, "EF").expect("a stream resolves to its dictionary");
        assert_eq!(name_at(&arena, &read, "Type").as_deref(), Some("EmbeddedFile"));
    }

    /// A chain that loops answers `None` rather than running forever.
    ///
    /// `Object::resolve` caps at 64 and returns `Null`; this records that the accessors
    /// inherit that rather than needing a guard of their own.
    #[test]
    fn a_reference_that_loops_is_not_followed_forever() {
        let arena = PdfArena::new();
        let looped = arena.alloc_object(Object::Null);
        arena.set_object(looped, Object::Reference(looped));
        let mut dict = Dict::new();
        dict.insert(arena.name("V"), Object::Reference(looped));

        assert_eq!(integer_at(&arena, &dict, "V"), None);
    }

    /// `text_of` follows a reference, which one of the four copies it replaces did not.
    #[test]
    fn text_is_read_through_a_reference() {
        let arena = PdfArena::new();
        let target = arena.alloc_object(Object::Text("Chapter One".to_string()));

        assert_eq!(
            text_of(&arena, &Object::Reference(target)).as_deref(),
            Some("Chapter One"),
            "an indirect string is the string it names"
        );
        assert_eq!(
            text_of(&arena, &Object::Text("direct".to_string())).as_deref(),
            Some("direct"),
            "and a direct one is still itself"
        );
    }

    /// A hex string reaches 7.9.2.2's decoding, not `from_utf8_lossy`.
    #[test]
    fn a_utf16_string_is_recovered_rather_than_mangled() {
        let arena = PdfArena::new();
        // UTF-16BE with the byte-order mark 7.9.2.2 requires.
        let utf16 = bytes::Bytes::from(vec![0xFE, 0xFF, 0x00, 0x41, 0x00, 0x42]);
        assert_eq!(text_of(&arena, &Object::Hex(utf16)).as_deref(), Some("AB"));
    }
}
