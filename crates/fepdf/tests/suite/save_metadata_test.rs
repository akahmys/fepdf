//! What a save says about the document it makes: its ID and its dates (Y-F2, Y-F4, Y-F5,
//! Y-F6).

use fepdf::{PdfDocument, SaveOptions};
use fepdf_model::Object;
use fepdf_model::access::entry;

/// 2026-10-03T00:00:00Z.
const STAMP: u64 = 1_790_985_600;

/// A one-page document whose trailer carries `id` and whose `/Info` is `info`.
fn document(info: &str, id: &str) -> PdfDocument {
    // The trailer gives the information dictionary and an `/ID`, which is what a source
    // without XMP is known by.
    let bytes = fepdf_fixtures::Pdf::new()
        .trailer_entries(&format!("/Info 4 0 R /ID [<{id}> <{id}>]"))
        .assemble(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            info,
        ]);
    PdfDocument::open(bytes.into()).expect("the fixture opens")
}

/// The XMP packet and the `/Info` of `doc` saved with `options`, read back.
fn saved(doc: &PdfDocument, options: &SaveOptions) -> (String, String) {
    let path = std::env::temp_dir().join(format!(
        "fepdf-save-metadata-{}-{:?}.pdf",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = doc.save_with_options(&path, "2.0", options).expect("it saves");
    // Read as any caller would: opening keeps the packet's identity (ROADMAP Y-F29).
    let back = PdfDocument::open(std::fs::read(&path).expect("written").into()).expect("it reads");
    let _ = std::fs::remove_file(&path);
    let inner = back.inner();
    let arena = inner.arena();
    let catalog = Object::Reference(*inner.root_handle());
    let packet = entry(arena, &catalog, "Metadata")
        .map(|m| {
            String::from_utf8_lossy(&inner.decode_stream(&m).expect("it decodes")).into_owned()
        })
        .unwrap_or_default();
    let info = inner.info_handle().map_or(String::new(), |h| {
        let date = |k: &str| fepdf_model::access::entry(arena, &Object::Reference(h), k);
        format!("{:?} {:?}", date("CreationDate"), date("ModDate"))
    });
    (packet, info)
}

/// The text between `<{tag}>` and its close in an XMP packet, or the attribute's value.
fn xmp(packet: &str, tag: &str) -> Option<String> {
    if let Some(at) = packet.find(&format!("<{tag}>")) {
        let rest = &packet[at + tag.len() + 2..];
        return rest.find("</").map(|end| rest[..end].to_string());
    }
    let at = packet.find(&format!("{tag}=\""))?;
    let rest = &packet[at + tag.len() + 2..];
    rest.find('"').map(|end| rest[..end].to_string())
}

fn stamped() -> SaveOptions {
    SaveOptions { stamped_at: Some(STAMP), ..SaveOptions::default() }
}

/// **Two untitled documents are two documents.** The ID was the title's hash, so every
/// untitled document saved shared one (Y-F4); the same document stamped alike still
/// writes the same one.
#[test]
fn untitled_documents_do_not_share_an_id() {
    let a = document("<< /Producer (A) >>", "0011");
    let b = document("<< /Producer (A) >>", "2233");
    let id = |doc: &PdfDocument| xmp(&saved(doc, &stamped()).0, "xmpMM:DocumentID");
    assert!(id(&a).is_some(), "no DocumentID written");
    assert_ne!(id(&a), id(&b), "two untitled documents were given one ID");
    assert_eq!(id(&a), id(&a), "one document stamped alike was given two IDs");
}

/// **A date the source does not state is not written** (Y-F5), and the modification is
/// the save's, in the packet and in `/Info` alike (Y-F6).
#[test]
fn a_dateless_document_gets_no_invented_creation_and_the_save_as_its_modification() {
    let doc = document("<< /Producer (A) >>", "0011");
    let (packet, info) = saved(&doc, &stamped());
    assert_eq!(xmp(&packet, "xmp:CreateDate"), None, "a creation date was invented: {packet}");
    assert_eq!(xmp(&packet, "xmp:ModifyDate").as_deref(), Some("2026-10-03T00:00:00Z"));
    assert_eq!(xmp(&packet, "xmp:MetadataDate").as_deref(), Some("2026-10-03T00:00:00Z"));
    assert!(info.contains("D:20261003000000Z"), "/Info's ModDate is not the save's: {info}");
}

/// The source's creation date is kept; its modification date is not, since the save is a
/// new document (ADR-0012).
#[test]
fn the_source_creation_is_kept_and_its_modification_replaced() {
    let doc =
        document("<< /CreationDate (D:20200102030405Z) /ModDate (D:20210102030405Z) >>", "0011");
    let (packet, _) = saved(&doc, &stamped());
    assert_eq!(xmp(&packet, "xmp:CreateDate").as_deref(), Some("2020-01-02T03:04:05Z"));
    assert_eq!(xmp(&packet, "xmp:ModifyDate").as_deref(), Some("2026-10-03T00:00:00Z"));
}

/// **A creation date the caller gives is the one written** (Y-F2), and one that is no
/// date is refused.
#[test]
fn a_creation_date_the_caller_gives_is_written_and_a_bad_one_refused() {
    let doc = document("<< /CreationDate (D:20200102030405Z) >>", "0011");
    let given = SaveOptions { creation_date: Some("D:19991231235959Z".into()), ..stamped() };
    let (packet, info) = saved(&doc, &given);
    assert_eq!(xmp(&packet, "xmp:CreateDate").as_deref(), Some("1999-12-31T23:59:59Z"));
    assert!(info.contains("D:19991231235959Z"), "{info}");

    let path = std::env::temp_dir().join(format!("fepdf-bad-date-{}.pdf", std::process::id()));
    let bad = SaveOptions { creation_date: Some("yesterday".into()), ..stamped() };
    assert!(
        matches!(doc.save_with_options(&path, "2.0", &bad), Err(fepdf::PdfError::Refused { .. })),
        "a creation date that is no date was written"
    );
    let _ = std::fs::remove_file(&path);
}

/// The packet an opened document holds, as the engine holds it.
fn held_packet(doc: &PdfDocument) -> String {
    let inner = doc.inner();
    let catalog = Object::Reference(*inner.root_handle());
    entry(inner.arena(), &catalog, "Metadata")
        .map(|m| {
            String::from_utf8_lossy(&inner.decode_stream(&m).expect("it decodes")).into_owned()
        })
        .unwrap_or_default()
}

/// **Opening a file keeps which document it is.** Opening settled the metadata into a
/// packet drawn from the clock with no derivation, so one file opened twice named two
/// documents and its `DerivedFrom` was gone (Y-F29).
#[test]
fn opening_keeps_the_files_identity() {
    let saved_once = {
        let doc = document("<< /Producer (A) >>", "0011");
        let path = std::env::temp_dir().join(format!("fepdf-identity-{}.pdf", std::process::id()));
        let _ = doc.save_with_options(&path, "2.0", &stamped()).expect("it saves");
        let bytes = std::fs::read(&path).expect("written");
        let _ = std::fs::remove_file(&path);
        bytes
    };
    let first = held_packet(&PdfDocument::open(saved_once.clone().into()).expect("it opens"));
    let second = held_packet(&PdfDocument::open(saved_once.into()).expect("it opens"));
    let id = xmp(&first, "xmpMM:DocumentID");
    assert!(id.is_some(), "the file's DocumentID was dropped: {first}");
    assert_eq!(id, xmp(&second, "xmpMM:DocumentID"), "one file opened twice named two documents");
    assert!(first.contains("DerivedFrom"), "the file's derivation was dropped: {first}");
}

/// A file with no packet is given one holding its fields, and no identity it never had.
#[test]
fn opening_a_file_without_a_packet_invents_no_identity() {
    let doc = document("<< /Producer (A) >>", "0011");
    let packet = held_packet(&doc);
    assert_eq!(xmp(&packet, "xmpMM:DocumentID"), None, "an identity was invented: {packet}");
}
