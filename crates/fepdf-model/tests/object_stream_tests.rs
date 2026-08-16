//! Packing objects into `/ObjStm`, and the rules about what may not go in one (7.5.7).
//!
//! Object streams and cross-reference streams are one feature, not two: a classic
//! cross-reference table has no entry type that can say "object 12 is the third object
//! inside stream 40", so a file with object streams and a classic table is unreadable.
//! These tests read the produced bytes back rather than asking the writer what it did.

use bytes::Bytes;
use fepdf_model::{Handle, Object, PdfArena};
use std::collections::BTreeMap;

/// A document with a handful of dictionaries and one stream, so that both sides of the
/// packing decision are exercised.
fn document(arena: &PdfArena) -> (Handle<Object>, Handle<Object>) {
    let mut stream_dict = BTreeMap::new();
    stream_dict.insert(arena.name("Type"), Object::Name(arena.name("Probe")));
    let stream = arena.alloc_object(Object::Stream(
        arena.alloc_dict(stream_dict),
        std::sync::Arc::new(fepdf_model::object::SublimatedData::Raw(Bytes::from_static(
            b"stream contents",
        ))),
    ));

    let mut page = BTreeMap::new();
    page.insert(arena.name("Type"), Object::Name(arena.name("Page")));
    page.insert(arena.name("Contents"), Object::Reference(stream));
    let page_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(page)));

    let mut pages = BTreeMap::new();
    pages.insert(arena.name("Type"), Object::Name(arena.name("Pages")));
    pages.insert(
        arena.name("Kids"),
        Object::Array(arena.alloc_array(vec![Object::Reference(page_h)])),
    );
    pages.insert(arena.name("Count"), Object::Integer(1));
    let pages_h = arena.alloc_object(Object::Dictionary(arena.alloc_dict(pages)));

    let mut catalog = BTreeMap::new();
    catalog.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    catalog.insert(arena.name("Pages"), Object::Reference(pages_h));
    catalog.insert(arena.name("Marker"), Object::String(Bytes::from_static(b"findable")));
    (arena.alloc_object(Object::Dictionary(arena.alloc_dict(catalog))), stream)
}

/// Compression is on because the assertions depend on it: an uncompressed container
/// leaves its objects readable in the file, so "these bytes are still here" would be
/// true whether or not the object was packed.
fn write(pack: bool) -> Vec<u8> {
    let arena = PdfArena::new();
    let (root, _) = document(&arena);
    let mut out = Vec::new();
    {
        let mut writer = fepdf_model::writer::PdfWriter::new(&mut out, &arena);
        writer.set_pack_objects(pack);
        writer.set_compression(9);
        writer.write_header("2.0").expect("a header");
        writer.finish(root, None).expect("a document");
    }
    out
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack.windows(needle.len()).filter(|w| *w == needle).count()
}

/// The switch is one switch: asking for object streams gets a cross-reference stream
/// too, because the alternative cannot describe the result.
#[test]
fn packing_brings_a_cross_reference_stream_with_it() {
    let packed = write(true);
    assert!(count(&packed, b"/Type /ObjStm") > 0, "no object stream was written");
    assert!(count(&packed, b"/Type /XRef") > 0, "no cross-reference stream was written");
    assert_eq!(count(&packed, b"\r\ntrailer"), 0, "a classic trailer was written as well");

    let loose = write(false);
    assert_eq!(count(&loose, b"/Type /ObjStm"), 0, "objects were packed without being asked");
    assert_eq!(count(&loose, b"/Type /XRef"), 0);
    assert!(count(&loose, b"trailer") > 0, "the classic path lost its trailer");
}

/// 7.5.7 forbids it, and it could not work: a stream inside a stream has no `/Length`
/// the outer container could honour.
#[test]
fn a_stream_object_is_never_packed() {
    let packed = write(true);
    assert!(count(&packed, b"/Type /ObjStm") > 0, "nothing was packed, so this proves nothing");
    // A dictionary that *was* packed is gone from the file's plain bytes, which is what
    // makes the stream's survival below mean something.
    assert_eq!(count(&packed, b"/Type /Catalog"), 0, "the container is not compressed");
    // The stream's own payload is Flate-compressed, so look for its dictionary: that
    // is written in the clear as part of the stream object, and would be inside the
    // container if the object had been packed.
    assert!(count(&packed, b"/Type /Probe") > 0, "the stream object was packed into a container");
}

/// The whole point: a packed object is reachable through the type 2 entry that names
/// its container. Reading the file back has to find the catalogue and everything under
/// it, all of which now lives inside an `/ObjStm`.
#[test]
fn a_packed_document_reads_back_whole() {
    let packed = write(true);
    let read = fepdf_model::reader::load_document(&packed).expect("the file should parse");
    let arena = &read.arena;

    let catalog = read
        .trailer
        .and_then(|t| arena.get_dict(t))
        .and_then(|d| d.get(&arena.name("Root")).cloned())
        .and_then(|r| r.resolve(arena).as_dict_handle())
        .and_then(|h| arena.get_dict(h))
        .expect("a catalogue, reached through a type 2 entry");

    match catalog.get(&arena.name("Marker")).map(|o| o.resolve(arena)) {
        Some(Object::String(b) | Object::Hex(b)) => {
            assert_eq!(&b[..], b"findable", "the packed catalogue came back wrong");
        }
        other => panic!("the marker came back as {other:?}"),
    }

    // And the page tree under it, which is a second and a third packed object.
    let pages = catalog
        .get(&arena.name("Pages"))
        .map(|p| p.resolve(arena))
        .and_then(|p| p.as_dict_handle())
        .and_then(|h| arena.get_dict(h))
        .expect("a page tree");
    assert_eq!(pages.get(&arena.name("Count")).map(|c| c.resolve(arena)), Some(Object::Integer(1)));
}
