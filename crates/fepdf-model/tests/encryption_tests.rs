//! What a written string has to survive: every byte value, not just the printable ones.
//!
//! The interesting property is not that encrypt-then-decrypt round-trips — the engine
//! agreeing with itself is the weakest statement available, and it held throughout the
//! defect these tests exist for. It is that a string of *arbitrary bytes* survives being
//! written and read back, which is what encryption turns every string into.
//!
//! The defect: `write_string_literal` escaped `(`, `)` and `\` and wrote every other
//! byte raw, while the lexer returned a raw carriage return unchanged. 7.3.4.2 says an
//! unescaped end-of-line in a literal string is one line feed, so the written `\r` was
//! a different byte on the way back — but only for a reader that follows the clause,
//! and this engine did not. The two mistakes cancelled. PDFKit found it, on one page of
//! `fy05.pdf`, once every string in the file became random bytes.

use bytes::Bytes;
use fepdf_model::{Object, PdfArena};
use std::collections::BTreeMap;

/// Writes a one-object document carrying `value` as a string, and reads it back.
fn round_trip(value: &[u8]) -> Vec<u8> {
    let arena = PdfArena::new();
    let mut catalog = BTreeMap::new();
    catalog.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    catalog.insert(arena.name("Probe"), Object::String(Bytes::copy_from_slice(value)));
    let root = arena.alloc_object(Object::Dictionary(arena.alloc_dict(catalog)));

    let mut out = Vec::new();
    {
        let mut writer = fepdf_model::writer::PdfWriter::new(&mut out, &arena);
        writer.write_header("2.0").expect("a header");
        writer.finish(root, None).expect("a document");
    }

    let read = fepdf_model::reader::load_document(&out).expect("the file should parse");
    let catalog = read
        .trailer
        .and_then(|t| read.arena.get_dict(t))
        .and_then(|d| d.get(&read.arena.name("Root")).cloned())
        .and_then(|r| r.resolve(&read.arena).as_dict_handle())
        .and_then(|h| read.arena.get_dict(h))
        .expect("a catalogue");
    match catalog.get(&read.arena.name("Probe")).map(|o| o.resolve(&read.arena)) {
        Some(Object::String(b) | Object::Hex(b)) => b.to_vec(),
        other => panic!("the probe came back as {other:?}"),
    }
}

/// Every byte, not just the printable ones. An encrypted string is uniformly random, so
/// one byte in 256 is a carriage return and every document hits it many times over.
#[test]
fn a_string_of_arbitrary_bytes_survives_a_round_trip() {
    let all: Vec<u8> = (0..=255u8).collect();
    assert_eq!(round_trip(&all), all, "not every byte survived");
}

/// The specific byte the defect turned on, and the sequences around it. `\r\n` is the
/// sharper case: unescaped it collapses to one byte, so the string comes back *shorter*.
#[test]
fn carriage_returns_survive_being_written() {
    for probe in
        [&b"\r"[..], b"\r\n", b"before\rafter", b"\r\r\r", b"\n\r", b"mixed\r\ncontent\rhere\n"]
    {
        assert_eq!(round_trip(probe), probe, "{probe:?} did not survive");
    }
}

/// The bytes that were already escaped, so that fixing the others did not break them.
#[test]
fn parentheses_and_backslashes_still_survive() {
    for probe in [&b"(unbalanced"[..], b"balanced (pair)", b"back\\slash", b"\\(\\)", b")("] {
        assert_eq!(round_trip(probe), probe, "{probe:?} did not survive");
    }
}
