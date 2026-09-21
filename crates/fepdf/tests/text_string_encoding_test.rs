//! Entries ISO 32000-2 defines as *text strings* (7.9.2.2), through a file and back.
//!
//! `apply/` built twenty PDF strings with `Object::String(Bytes::from(<a Rust String>))`,
//! which writes the Rust string's raw UTF-8 bytes as a byte string. For an entry the
//! standard types as a text string — PDFDocEncoding, or UTF-16BE/UTF-8 behind a byte
//! order mark — that is wrong, and a reader that follows 7.9.2.2 gets mojibake: a layer
//! named `第一層` came back as `ç¬¬ä¸\u{80}å±¤`.
//!
//! **Every value below is deliberately outside PDFDocEncoding.** An ASCII value round
//! trips through that defect untouched and would assert nothing — which is the whole
//! reason these cases are in Japanese rather than in English.
//!
//! **And every assertion is made on a reopened file**, not on the arena the operation
//! left behind: it is the writer that turns an `Object::Text` into UTF-16BE and the
//! parser that turns it back, so an in-memory comparison would pass with either spelling.
//!
//! The entries that are *not* text strings are here too — the `/EmbeddedFiles` name-tree
//! key, the collection `/D` that has to equal one, and a `GoToE` target's `/N` — because
//! what makes them byte strings is that a lookup compares them to each other. Those cases
//! assert the bytes match, and fail if anyone makes one of the three text.

use fepdf::PdfDocument;
use fepdf_model::arena::PdfArena;
use fepdf_model::handle::Handle;
use fepdf_model::object::{Object, PdfName};
use std::collections::BTreeMap;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// Writes the document out and opens the bytes again.
///
/// `name` only keeps concurrently running cases out of each other's file.
fn round_trip(doc: &PdfDocument, name: &str) -> PdfDocument {
    let dir = std::env::temp_dir().join("fepdf-text-string-encoding");
    std::fs::create_dir_all(&dir).expect("a directory to write into");
    let path = dir.join(format!("{name}.pdf"));
    doc.save_as_version(&path, "2.0").expect("it writes");
    let bytes = std::fs::read(&path).expect("it is on disk");
    let _ = std::fs::remove_file(&path);
    PdfDocument::open(bytes.into()).expect("it opens")
}

/// The catalogue of `doc`.
fn catalog(doc: &PdfDocument) -> Dict {
    let inner = doc.inner();
    let handle = inner.catalog_handle().expect("the document has a catalogue");
    let dh = inner.resolve_to_dict(handle).expect("the catalogue is a dictionary");
    inner.arena().get_dict(dh).expect("the catalogue dictionary is there")
}

/// The dictionary `object` is or refers to.
fn dict_of(arena: &PdfArena, object: &Object) -> Dict {
    let handle = object.resolve(arena).as_dict_handle().expect("a dictionary was expected");
    arena.get_dict(handle).expect("the dictionary is there")
}

/// The dictionary at `key`.
fn dict_at(arena: &PdfArena, dict: &Dict, key: &str) -> Dict {
    dict_of(arena, dict.get(&arena.name(key)).unwrap_or_else(|| panic!("/{key} is missing")))
}

/// The array at `key`.
fn array_at(arena: &PdfArena, dict: &Dict, key: &str) -> Vec<Object> {
    let entry = dict.get(&arena.name(key)).unwrap_or_else(|| panic!("/{key} is missing"));
    let handle = entry.resolve(arena).as_array().expect("an array was expected");
    arena.get_array(handle).expect("the array is there")
}

/// The text at `key`, decoded however the file spelled it (7.9.2.2).
///
/// A file this engine wrote carries UTF-16BE behind a BOM; the refinery may have turned
/// that into an `Object::Text` already. Both arms are the *correct* reading — the defect
/// this file is about shows up as the wrong characters, not as a missing entry.
fn text_at(arena: &PdfArena, dict: &Dict, key: &str) -> String {
    let entry = dict.get(&arena.name(key)).unwrap_or_else(|| panic!("/{key} is missing"));
    match entry.resolve(arena) {
        Object::Text(text) => text,
        Object::String(bytes) | Object::Hex(bytes) => {
            fepdf_model::refine::text::recover_string(&bytes)
        }
        other => panic!("/{key} is {other:?}, not a string"),
    }
}

/// The raw bytes at `key`, for the entries that are compared rather than displayed.
fn bytes_at(arena: &PdfArena, dict: &Dict, key: &str) -> Vec<u8> {
    let entry = dict.get(&arena.name(key)).unwrap_or_else(|| panic!("/{key} is missing"));
    match entry.resolve(arena) {
        Object::String(bytes) | Object::Hex(bytes) => bytes.to_vec(),
        other => panic!("/{key} is {other:?}, and a byte string was expected"),
    }
}

/// A page label prefix survives (Table 161, `/P`).
#[test]
fn a_page_label_prefix_is_a_text_string() {
    let mut doc = PdfDocument::create_empty().expect("a new document opens");
    doc.apply(fepdf::Operation::SetPageLabels(vec![fepdf::PageLabelSpec {
        start_page: 0,
        style: fepdf::PageLabelStyle::Decimal,
        prefix: Some("付録-".to_string()),
        start_number: 1,
    }]))
    .expect("the labels are written");

    let reopened = round_trip(&doc, "page-label");
    let arena = reopened.inner().arena();
    let labels = dict_at(arena, &catalog(&reopened), "PageLabels");
    let nums = array_at(arena, &labels, "Nums");
    let range = dict_of(arena, &nums[1]);
    assert_eq!(text_at(arena, &range, "P"), "付録-");
}

/// A filespec's `/UF` and `/Desc` survive, and its `/F` stays a path (Table 43).
#[test]
fn a_file_specification_separates_its_path_from_its_unicode_name() {
    let mut doc = PdfDocument::create_empty().expect("a new document opens");
    doc.apply(fepdf::Operation::CreatePortfolio(fepdf::PortfolioCollection {
        view_mode: fepdf::CollectionViewMode::Details,
        initial_document: Some("添付ファイル.txt".to_string()),
        items: vec![fepdf::PortfolioItem {
            filename: "添付ファイル.txt".to_string(),
            mime_type: Some("text/plain".to_string()),
            description: Some("説明文".to_string()),
            size_bytes: 3,
            data: b"abc".to_vec(),
        }],
    }))
    .expect("the portfolio is written");

    let reopened = round_trip(&doc, "filespec");
    let arena = reopened.inner().arena();
    let cdict = catalog(&reopened);
    let names = dict_at(arena, &cdict, "Names");
    let embedded = dict_at(arena, &names, "EmbeddedFiles");
    let entries = array_at(arena, &embedded, "Names");
    let filespec = dict_of(arena, &entries[1]);

    assert_eq!(text_at(arena, &filespec, "UF"), "添付ファイル.txt");
    assert_eq!(text_at(arena, &filespec, "Desc"), "説明文");

    // **`/F` is a file specification string (7.11.2) and stays raw bytes.** A BOM in front
    // of it would be a BOM in front of a path. This is the entry `/UF` exists to relieve,
    // so the assertion is that the two differ, not that `/F` is readable.
    let path_bytes = bytes_at(arena, &filespec, "F");
    assert_eq!(
        path_bytes,
        "添付ファイル.txt".as_bytes(),
        "/F stopped being the raw file specification string"
    );

    // The name-tree key and the collection's `/D` are compared against each other, so both
    // are byte strings and both must still be these exact bytes.
    let key = match entries[0].resolve(arena) {
        Object::String(bytes) | Object::Hex(bytes) => bytes.to_vec(),
        other => panic!("the name-tree key is {other:?}"),
    };
    assert_eq!(key, path_bytes, "the name-tree key no longer matches the file it names");
    let collection = dict_at(arena, &cdict, "Collection");
    assert_eq!(
        bytes_at(arena, &collection, "D"),
        key,
        "the collection's /D no longer matches a key of the name tree"
    );
}

/// An optional content group's `/Name` survives (Table 98).
#[test]
fn a_layer_name_is_a_text_string() {
    let mut doc = PdfDocument::create_empty().expect("a new document opens");
    doc.apply(fepdf::Operation::UpdateLayers(fepdf::OptionalContentProperties {
        layers: vec![fepdf::LayerGroup {
            name: "第一層".to_string(),
            default_state: fepdf::VisibilityState::On,
            printable: true,
        }],
    }))
    .expect("the layers are written");

    let reopened = round_trip(&doc, "layer-name");
    let arena = reopened.inner().arena();
    let props = dict_at(arena, &catalog(&reopened), "OCProperties");
    let groups = array_at(arena, &props, "OCGs");
    let ocg = dict_of(arena, &groups[0]);
    assert_eq!(text_at(arena, &ocg, "Name"), "第一層");
}

/// An output intent's `/OutputConditionIdentifier` and `/Info` survive (Table 401).
#[test]
fn an_output_intent_identifier_and_info_are_text_strings() {
    let mut doc = PdfDocument::create_empty().expect("a new document opens");
    doc.apply(fepdf::Operation::SetOutputIntent(fepdf::OutputIntent {
        subtype: "GTS_PDFA1".to_string(),
        identifier: "日本印刷条件".to_string(),
        info: Some("追加情報".to_string()),
        icc_profile_bytes: None,
    }))
    .expect("the output intent is written");

    let reopened = round_trip(&doc, "output-intent");
    let arena = reopened.inner().arena();
    let intents = array_at(arena, &catalog(&reopened), "OutputIntents");
    let intent = dict_of(arena, &intents[0]);
    assert_eq!(text_at(arena, &intent, "OutputConditionIdentifier"), "日本印刷条件");
    assert_eq!(text_at(arena, &intent, "Info"), "追加情報");
}

/// An article thread's `/Title` survives (12.4.3, and Table 349 for the entry itself).
#[test]
fn an_article_thread_title_is_a_text_string() {
    let mut doc = PdfDocument::create_empty().expect("a new document opens");
    doc.apply(fepdf::Operation::UpdateArticleThreads(vec![fepdf::ArticleThread {
        title: "特集記事".to_string(),
        beads: vec![fepdf::ArticleBead { page: 0, rect: [50.0, 50.0, 300.0, 400.0] }],
    }]))
    .expect("the threads are written");

    let reopened = round_trip(&doc, "article-thread");
    let arena = reopened.inner().arena();
    let threads = array_at(arena, &catalog(&reopened), "Threads");
    let thread = dict_of(arena, &threads[0]);
    let info = dict_at(arena, &thread, "I");
    assert_eq!(text_at(arena, &info, "Title"), "特集記事");
}

/// A structure element's `/Alt` survives (14.9.3).
///
/// The reader that answers this — `struct_tree::parse_alt_text_helper` — had the same
/// defect in the other direction, reading the entry with `as_string` and then
/// `String::from_utf8`: it dropped an `Object::Text` and it dropped the UTF-16BE that a
/// conforming file carries. Both halves are exercised here, because the assertion is made
/// after the value has been through a file.
#[test]
fn an_alternate_description_is_a_text_string() {
    let mut doc = PdfDocument::create_empty().expect("a new document opens");
    let element = struct_element(&doc);
    doc.apply(fepdf::Operation::UpdateStructElem(fepdf::StructElemUpdate {
        handle_index: element,
        new_tag: Some("Figure".to_string()),
        new_alt: Some("代替テキスト".to_string()),
    }))
    .expect("the element is updated");

    // In the arena, where the entry is an `Object::Text` the old reader answered `None` for.
    let node = fepdf_doc::StructureTreeVisitor::extract(doc.inner()).expect("a structure tree");
    assert_eq!(node.alt_text.as_deref(), Some("代替テキスト"), "read from the arena");

    let reopened = round_trip(&doc, "alt-text");
    let arena = reopened.inner().arena();
    let root = dict_at(arena, &catalog(&reopened), "StructTreeRoot");
    assert_eq!(text_at(arena, &root, "Alt"), "代替テキスト", "read from the file");

    // And from the file, where it is UTF-16BE behind a BOM — which the old reader handed to
    // `String::from_utf8`, so it answered `None` for this too.
    let node = fepdf_doc::StructureTreeVisitor::extract(reopened.inner()).expect("a tree");
    assert_eq!(node.alt_text.as_deref(), Some("代替テキスト"), "read back through the reader");
}

/// A user property's `/N`, its string `/V` and its `/F` survive (Table 380).
#[test]
fn a_user_property_name_value_and_format_are_text_strings() {
    let mut doc = PdfDocument::create_empty().expect("a new document opens");
    let element = struct_element(&doc);
    doc.apply(fepdf::Operation::AddUserProperties {
        target_handle: element,
        properties: vec![fepdf::UserProperty {
            name: "発行元".to_string(),
            value: fepdf::UserPropertyValue::Text("株式会社れい".to_string()),
            formatted: Some("株式会社れい（東京）".to_string()),
        }],
    })
    .expect("the properties are written");

    let reopened = round_trip(&doc, "user-property");
    let arena = reopened.inner().arena();
    let root = dict_at(arena, &catalog(&reopened), "StructTreeRoot");
    let attributes = array_at(arena, &root, "A");
    let attribute = dict_of(arena, &attributes[0]);
    let properties = array_at(arena, &attribute, "P");
    let property = dict_of(arena, &properties[0]);

    assert_eq!(text_at(arena, &property, "N"), "発行元");
    assert_eq!(text_at(arena, &property, "V"), "株式会社れい");
    assert_eq!(text_at(arena, &property, "F"), "株式会社れい（東京）");
}

/// A structure element hung off the catalogue as `/StructTreeRoot`, and its handle index.
///
/// Both operations above address their target by object index and neither creates one.
/// The catalogue is where it has to hang: an object nothing refers to is not written to
/// the file, and these assertions are all made on a file.
fn struct_element(doc: &PdfDocument) -> u32 {
    let inner = doc.inner();
    let arena = inner.arena();
    let mut element = BTreeMap::new();
    element.insert(arena.name("Type"), Object::Name(arena.name("StructElem")));
    element.insert(arena.name("S"), Object::Name(arena.name("Document")));
    let dh = arena.alloc_dict(element);
    let handle = arena.alloc_object(Object::Dictionary(dh));

    let cah = inner.catalog_handle().expect("the document has a catalogue");
    let cadh = inner.resolve_to_dict(cah).expect("the catalogue is a dictionary");
    let mut cdict = arena.get_dict(cadh).expect("the catalogue dictionary is there");
    cdict.insert(arena.name("StructTreeRoot"), Object::Reference(handle));
    arena.set_dict(cadh, cdict);

    handle.index()
}
