use crate::{Document, FromPdfObject, Handle, Object, PdfArena};
use std::collections::BTreeMap;

/// Refined PDF Info Dictionary (ISO 32000-2:2020 Clause 14.3.3)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "14.3.3")]
pub struct PdfInfo {
    #[pdf_key("Title")]
    /// `/Title`.
    pub title: Option<String>,
    #[pdf_key("Author")]
    /// `/Author`.
    pub author: Option<String>,
    #[pdf_key("Subject")]
    /// `/Subject`.
    pub subject: Option<String>,
    #[pdf_key("Keywords")]
    /// `/Keywords`.
    pub keywords: Option<String>,
    #[pdf_key("Creator")]
    /// `/Creator`: the application that authored the original.
    pub creator: Option<String>,
    #[pdf_key("Producer")]
    /// `/Producer`: the application that wrote the PDF.
    pub producer: Option<String>,
    #[pdf_key("CreationDate")]
    /// `/CreationDate`.
    pub creation_date: Option<String>,
    #[pdf_key("ModDate")]
    /// `/ModDate`.
    pub mod_date: Option<String>,
}

/// Basic document metadata.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MetadataInfo {
    /// The document title.
    pub title: Option<String>,
    /// The document author.
    pub author: Option<String>,
    /// The document subject.
    pub subject: Option<String>,
    /// The document keywords.
    pub keywords: Option<String>,
    /// The application that created the original document.
    pub creator: Option<String>,
    /// The application that produced the PDF.
    pub producer: Option<String>,
    /// The date and time the document was created.
    pub creation_date: Option<String>,
    /// The date and time the document was last modified.
    pub mod_date: Option<String>,
}

/// Reads the `/Info` dictionary (14.3.3).
fn read_info(doc: &Document) -> MetadataInfo {
    let arena = doc.arena();
    let mut info = MetadataInfo::default();
    if let Some(info_handle) = doc.info_handle()
        && let Some(obj) = arena.get_object(info_handle)
        && let Ok(pdf_info) = PdfInfo::from_pdf_object(obj, arena)
    {
        info.title = pdf_info.title;
        info.author = pdf_info.author;
        info.subject = pdf_info.subject;
        info.keywords = pdf_info.keywords;
        info.creator = pdf_info.creator;
        info.producer = pdf_info.producer;
        info.creation_date = pdf_info.creation_date;
        info.mod_date = pdf_info.mod_date;
    }
    info
}

/// Reads the catalogue's own metadata stream (14.3.2), if it has one.
fn read_catalog_xmp(doc: &Document) -> Option<MetadataInfo> {
    let arena = doc.arena();
    let Object::Dictionary(catalog_handle) = arena.get_object(*doc.root_handle())? else {
        return None;
    };
    let metadata_obj = arena.get_dict(catalog_handle)?.get(&arena.name("Metadata"))?.clone();
    let xml_data = doc.decode_stream(&metadata_obj.resolve(arena)).ok()?;
    let xml_str = std::str::from_utf8(&xml_data).ok()?;
    let xml_doc = roxmltree::Document::parse(xml_str).ok()?;
    let mut info = MetadataInfo::default();
    apply_xmp_metadata(&xml_doc, &mut info);
    Some(info)
}

/// Collects document metadata, preferring XMP over the `/Info` dictionary.
///
/// After [`settle`] has run at ingest the two agree, so this returns the same answer
/// whenever it is asked. It is the derivation, not the decision.
pub fn extract_metadata(doc: &Document) -> MetadataInfo {
    let mut info = read_info(doc);
    if let Some(xmp) = read_catalog_xmp(doc) {
        overlay(&mut info, xmp);
    }
    info
}

fn overlay(base: &mut MetadataInfo, top: MetadataInfo) {
    let fields: [(&mut Option<String>, Option<String>); 8] = [
        (&mut base.title, top.title),
        (&mut base.author, top.author),
        (&mut base.subject, top.subject),
        (&mut base.keywords, top.keywords),
        (&mut base.creator, top.creator),
        (&mut base.producer, top.producer),
        (&mut base.creation_date, top.creation_date),
        (&mut base.mod_date, top.mod_date),
    ];
    for (slot, value) in fields {
        if let Some(v) = value {
            *slot = Some(v);
        }
    }
}

/// The two spellings of a date are the same date.
///
/// `/Info` writes `D:20240620213357Z` and XMP writes `2024-06-20T21:33:57Z` for the
/// same instant, so comparing the strings reports a disagreement on almost every file
/// that carries both. What matters is whether they name the same moment.
fn same_date(a: &str, b: &str) -> bool {
    match (
        crate::refine::metadata::parse_date_string(a),
        crate::refine::metadata::parse_date_string(b),
    ) {
        (Some(x), Some(y)) => x == y,
        // Neither parses as a date, so fall back to what they say.
        _ => a == b,
    }
}

/// Settles a document's metadata into one state, at ingest.
///
/// 14.3.3 deprecates the `/Info` dictionary for everything but `CreationDate` and
/// `ModDate`, and directs document-level metadata to a metadata stream instead. So the
/// two can disagree, and a reader has to choose. This used to happen at save time and
/// in silence: `/Info` was read, then every XMP field overwrote it with no comparison.
/// `samples/fy05.pdf` says 2024-11-14 in `/Info` and 2024-11-08 in its packet, six days
/// apart, and nothing said so.
///
/// XMP wins, because 14.3.3 says that is where the value belongs. The disagreement is
/// recorded rather than swallowed, and the deprecated entries are moved out of `/Info`
/// so that later readings of this document find one answer instead of two.
pub fn settle(doc: &Document, decisions: &mut crate::interpretation::DecisionLog) -> MetadataInfo {
    let from_info = read_info(doc);
    let from_xmp = read_catalog_xmp(doc);
    if let Some(xmp) = &from_xmp {
        report_disagreement(&from_info, xmp, decisions);
    }
    let mut settled = from_info;
    if let Some(xmp) = from_xmp {
        overlay(&mut settled, xmp);
    }
    // Only once the stream holds the value may `/Info` give up its copy: a file whose
    // metadata is only in `/Info` — six of the nine corpus files — would otherwise lose
    // it, and if the write fails there is nowhere else for the entries to live. `doc`
    // here is the document as ingested, whose provenance is empty, so this packet makes
    // no claim about derivation; the save path writes that.
    if update_xmp_metadata(doc, &settled).is_ok() {
        migrate_deprecated_info(doc);
    }
    settled
}

fn report_disagreement(
    info: &MetadataInfo,
    xmp: &MetadataInfo,
    decisions: &mut crate::interpretation::DecisionLog,
) {
    let pairs: [(&str, &Option<String>, &Option<String>, bool); 8] = [
        ("Title", &info.title, &xmp.title, false),
        ("Author", &info.author, &xmp.author, false),
        ("Subject", &info.subject, &xmp.subject, false),
        ("Keywords", &info.keywords, &xmp.keywords, false),
        ("Creator", &info.creator, &xmp.creator, false),
        ("Producer", &info.producer, &xmp.producer, false),
        ("CreationDate", &info.creation_date, &xmp.creation_date, true),
        ("ModDate", &info.mod_date, &xmp.mod_date, true),
    ];
    for (key, left, right, is_date) in pairs {
        let (Some(a), Some(b)) = (left, right) else { continue };
        if a == b || (is_date && same_date(a, b)) {
            continue;
        }
        decisions.push(crate::interpretation::Decision::ambiguity(
            "14.3.3",
            format!("/Info /{key} is {a:?} and the metadata stream says {b:?}"),
            "took the metadata stream, which 14.3.3 makes the place for document-level \
             metadata; the /Info value is not carried"
                .to_string(),
        ));
    }
}

/// Removes from `/Info` the entries 14.3.3 deprecates, once they are held elsewhere.
///
/// Not recorded as a `Decision`. Carrying them is deprecated, not non-conformant, and
/// moving them loses nothing — the value is in the metadata stream before this runs, and
/// `CreationDate` and `ModDate`, which 14.3.3 still allows, stay put. All nine corpus
/// files carry some, so recording it would put a line in every log and make the log a
/// constant rather than a signal (`ARCHITECTURE.md` §5.3). What *is* recorded is the
/// case where something is lost: `/Info` and the stream disagreeing, which is one file.
fn migrate_deprecated_info(doc: &Document) {
    const DEPRECATED: [&str; 6] = ["Title", "Author", "Subject", "Keywords", "Creator", "Producer"];
    let arena = doc.arena();
    let Some(info_handle) = doc.info_handle() else { return };
    let Some(Object::Dictionary(dh)) = arena.get_object(info_handle) else { return };
    let Some(mut dict) = arena.get_dict(dh) else { return };

    let mut moved = false;
    for key in DEPRECATED {
        moved |= dict.remove(&arena.name(key)).is_some();
    }
    if moved {
        arena.set_dict(dh, dict);
    }
}

/// Updates the document metadata in the arena.
pub fn update_document_metadata(
    doc: &crate::Document,
    info: &MetadataInfo,
) -> crate::PdfResult<()> {
    let _arena = doc.arena();

    // 1. Update legacy Info dictionary (if it exists)
    update_legacy_info(doc, info)?;

    // 2. Update XMP Metadata in Catalog
    update_xmp_metadata(doc, info)?;

    Ok(())
}

fn update_legacy_info(doc: &crate::Document, info: &MetadataInfo) -> crate::PdfResult<()> {
    let arena = doc.arena();
    if let Some(info_handle) = doc.info_handle()
        && let Some(Object::Dictionary(dh)) = arena.get_object(info_handle)
    {
        let mut dict = arena.get_dict(dh).unwrap_or_default();
        // Remove deprecated keys in PDF 2.0
        dict.remove(&arena.name("Title"));
        dict.remove(&arena.name("Author"));
        dict.remove(&arena.name("Subject"));
        dict.remove(&arena.name("Keywords"));
        dict.remove(&arena.name("Creator"));
        dict.remove(&arena.name("Producer"));

        // Format dates as standard ASCII PDF string literals (D:...)
        if let Some(v) = &info.creation_date {
            dict.insert(arena.name("CreationDate"), Object::String(bytes::Bytes::from(v.clone())));
        }
        if let Some(v) = &info.mod_date {
            dict.insert(arena.name("ModDate"), Object::String(bytes::Bytes::from(v.clone())));
        }
        arena.set_dict(dh, dict);
    }
    Ok(())
}

fn update_xmp_metadata(doc: &crate::Document, info: &MetadataInfo) -> crate::PdfResult<()> {
    let arena = doc.arena();
    let root_handle = *doc.root_handle();
    if let Some(Object::Dictionary(catalog_dh)) = arena.get_object(root_handle) {
        let mut catalog_dict = arena
            .get_dict(catalog_dh)
            .ok_or_else(|| crate::error::PdfError::Other("Invalid Catalog".into()))?;

        let refined_map = build_refined_metadata_map(info);
        let raw_xmp = crate::refine::metadata::info_to_xmp_derived(&refined_map, &doc.provenance);

        // Append 2KB space padding and replace the read-only flag end="r" with writable flag end="w"
        let trimmed = raw_xmp.trim_end();
        let suffix = "<?xpacket end=\"r\"?>";
        let xmp_str = if let Some(base) = trimmed.strip_suffix(suffix) {
            let mut padded = String::with_capacity(base.len() + 2048 + 32);
            padded.push_str(base);
            // Append 20 lines of 100-character spaces as padding (2000 spaces)
            for _ in 0..20 {
                padded.push_str("                                                                                                    \n");
            }
            padded.push_str("<?xpacket end=\"w\"?>");
            padded
        } else {
            raw_xmp
        };

        let xmp_refined = crate::refine::metadata::create_metadata_stream(xmp_str);

        if let crate::refine::RefinedObject::Stream(dict, data) = xmp_refined {
            let metadata_handle = commit_metadata_stream(arena, dict, data);
            catalog_dict.insert(arena.name("Metadata"), Object::Reference(metadata_handle));
            arena.set_dict(catalog_dh, catalog_dict);
        }
    }
    Ok(())
}

fn insert_text_if_present(
    map: &mut BTreeMap<crate::object::PdfName, crate::refine::RefinedObject>,
    key: &str,
    val: &Option<String>,
) {
    if let Some(v) = val {
        map.insert(crate::object::PdfName::new(key), crate::refine::RefinedObject::Text(v.clone()));
    }
}

fn build_refined_metadata_map(
    info: &MetadataInfo,
) -> BTreeMap<crate::object::PdfName, crate::refine::RefinedObject> {
    let mut refined_map = BTreeMap::new();
    insert_text_if_present(&mut refined_map, "Title", &info.title);
    insert_text_if_present(&mut refined_map, "Author", &info.author);
    insert_text_if_present(&mut refined_map, "Subject", &info.subject);
    insert_text_if_present(&mut refined_map, "Keywords", &info.keywords);
    insert_text_if_present(&mut refined_map, "Creator", &info.creator);
    insert_text_if_present(&mut refined_map, "Producer", &info.producer);
    insert_text_if_present(&mut refined_map, "CreationDate", &info.creation_date);
    insert_text_if_present(&mut refined_map, "ModDate", &info.mod_date);
    refined_map
}

fn commit_metadata_stream(
    arena: &PdfArena,
    dict: BTreeMap<crate::object::PdfName, crate::refine::RefinedObject>,
    data: bytes::Bytes,
) -> Handle<Object> {
    let mut stream_dict = BTreeMap::new();
    for (k, v) in dict {
        if let crate::refine::RefinedObject::Name(n) = v {
            stream_dict.insert(arena.intern_name(k), Object::Name(arena.intern_name(n)));
        }
    }
    let sdh = arena.alloc_dict(stream_dict);
    arena.alloc_object(Object::Stream(
        sdh,
        std::sync::Arc::new(crate::object::SublimatedData::Raw(data)),
    ))
}

/// The RDF namespace, whose containers wrap almost every XMP value.
const RDF_LI: (&str, &str) = ("http://www.w3.org/1999/02/22-rdf-syntax-ns#", "li");

/// The text of an XMP property.
///
/// XMP wraps language alternatives and ordered values in `rdf:Alt`, `rdf:Seq` and
/// `rdf:Bag`, so `dc:title` is never a bare string — it is an `rdf:Alt` holding one
/// `rdf:li` per language. This used to take the first text node among the descendants,
/// which in a packet written with indentation is the newline and spaces between
/// `<dc:title>` and `<rdf:Alt>`. That is not nothing, so it overwrote the value read
/// from `/Info`, and `dc:title` and `dc:description` came out empty on every file whose
/// packet was indented — two of the nine corpus files lost their title on save.
/// Whitespace around a value is layout; a value that *is* whitespace is a value. The
/// two are told apart by the node, not the characters: a node with element children is
/// a container, and text between its children is the pretty-printer's. A leaf's text is
/// what the property says, even when every character of it is a space —
/// `samples/fy05.pdf` titles itself with one ideographic space, and dropping it would
/// make writing then reading the packet lose what it holds.
fn find_tag_text(doc: &roxmltree::Document, ns: &str, tag: &str) -> Option<String> {
    let node = doc.descendants().find(|n| n.has_tag_name((ns, tag)))?;
    // A container yields its first item; a bare property yields its own text.
    let source = node.descendants().find(|n| n.has_tag_name(RDF_LI)).unwrap_or(node);
    let text: String = source.children().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }
    let is_container = source.children().any(|n| n.is_element());
    (!is_container && !text.is_empty()).then_some(text)
}

fn apply_xmp_metadata(doc: &roxmltree::Document, info: &mut MetadataInfo) {
    let dc_ns = "http://purl.org/dc/elements/1.1/";
    let xmp_ns = "http://ns.adobe.com/xap/1.0/";
    let pdf_ns = "http://ns.adobe.com/pdf/1.3/";

    if let Some(text) = find_tag_text(doc, dc_ns, "title") {
        info.title = Some(text);
    }
    if let Some(node) = doc.descendants().find(|n| n.has_tag_name((dc_ns, "creator"))) {
        let creators: Vec<String> = node
            .descendants()
            .filter(|n| n.has_tag_name("li"))
            .filter_map(|li| li.text().map(|t| t.to_string()))
            .collect();
        if !creators.is_empty() {
            info.author = Some(creators.join(", "));
        }
    }
    if let Some(text) = find_tag_text(doc, dc_ns, "description") {
        info.subject = Some(text);
    }
    if let Some(text) = find_tag_text(doc, pdf_ns, "Keywords") {
        info.keywords = Some(text);
    }
    if let Some(text) = find_tag_text(doc, xmp_ns, "CreatorTool") {
        info.creator = Some(text);
    }
    if let Some(text) = find_tag_text(doc, pdf_ns, "Producer") {
        info.producer = Some(text);
    }
    if let Some(text) = find_tag_text(doc, xmp_ns, "CreateDate") {
        info.creation_date = Some(text);
    }
    if let Some(text) = find_tag_text(doc, xmp_ns, "ModifyDate") {
        info.mod_date = Some(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Moving the deprecated entries is not a departure and must not be logged as one.
    ///
    /// `ARCHITECTURE.md` §5.3: a decision that fires on conforming input is worse than
    /// none, because it makes the log a constant rather than a signal. All nine corpus
    /// files carry deprecated `/Info` entries, so recording the move put a line in every
    /// one of their logs — the same shape ADR-0008 removed.
    #[test]
    fn moving_the_deprecated_entries_is_not_recorded() {
        // The disagreement in this fixture is a real loss, so exactly one decision is
        // expected: the ambiguity. Not two.
        let doc = open_fixture(info_and_xmp_disagree());
        let taken = doc.decisions.entries();
        assert_eq!(taken.len(), 1, "{taken:?}");
        assert_eq!(taken[0].severity, crate::interpretation::Severity::Ambiguity);
    }

    /// A file whose `/Info` and metadata stream agree loses nothing at all, and says so.
    #[test]
    fn a_document_that_loses_nothing_records_nothing() {
        let doc = open_fixture(metadata_on_more_than_the_catalogue());
        assert!(doc.decisions.is_conforming(), "{:?}", doc.decisions.entries());
    }

    /// A document carrying metadata on an object as well as on the catalogue, which is
    /// what `--strip` used to walk past: 14.3.2 lets any stream or dictionary that
    /// represents a resource bear a `/Metadata` entry, and an Illustrator document
    /// puts one on almost everything.
    fn metadata_on_more_than_the_catalogue() -> Vec<u8> {
        let packet = "<?xpacket begin=\"\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF \
xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
<dc:creator><rdf:Seq><rdf:li>A Person</rdf:li></rdf:Seq></dc:creator>\
</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"r\"?>";
        let stream = |body: &str| {
            format!(
                "<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n{body}\nendstream",
                body.len()
            )
        };
        let bodies: [String; 6] = [
            "<< /Type /Catalog /Pages 2 0 R /Metadata 4 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            // 14.3.2: the page bears one too, and so does nothing else in this file.
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Metadata 6 0 R >>".to_string(),
            stream(packet),
            "<< /CreationDate (D:20240101000000Z) >>".to_string(),
            stream(packet),
        ];
        let mut out = b"%PDF-2.0\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in bodies.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
        }
        let xref_at = out.len();
        out.extend_from_slice(b"xref\n0 7\n0000000000 65535 f \n");
        for at in &offsets {
            out.extend_from_slice(format!("{at:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size 7 /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref_at}\n%%EOF\n"
            )
            .as_bytes(),
        );
        out
    }

    #[test]
    fn stripping_reaches_metadata_on_objects_not_only_the_catalogue() {
        let doc = open_fixture(metadata_on_more_than_the_catalogue());
        let mut log = crate::interpretation::DecisionLog::default();
        let report = strip_metadata_streams(&doc, &mut log);
        assert!(report.entries >= 2, "only {} entries removed", report.entries);

        let arena = doc.arena();
        let key = arena.name("Metadata");
        for dh in arena.all_dict_handles() {
            let Some(dict) = arena.get_dict(dh) else { continue };
            assert!(!dict.contains_key(&key), "a /Metadata entry survived the strip");
        }
        assert!(
            log.entries().iter().any(|d| d.clause == "14.3.2"),
            "the strip was not recorded: {:?}",
            log.entries()
        );
    }

    /// Nothing to strip is not a repair, and must not be reported as one.
    #[test]
    fn stripping_a_document_without_metadata_says_nothing() {
        let doc = open_fixture(metadata_on_more_than_the_catalogue());
        let mut log = crate::interpretation::DecisionLog::default();
        strip_metadata_streams(&doc, &mut log);
        let mut second = crate::interpretation::DecisionLog::default();
        let report = strip_metadata_streams(&doc, &mut second);
        assert_eq!(report, StripReport::default());
        assert!(second.is_conforming(), "{:?}", second.entries());
    }

    /// The spellings differ on every file that carries both, so comparing the strings
    /// would report a disagreement where there is none. Injecting a string comparison
    /// puts two false ambiguities on `samples/print_sample.pdf`, which has none.
    #[test]
    fn the_two_spellings_of_one_instant_are_not_a_disagreement() {
        assert!(same_date("D:20240620213357Z", "2024-06-20T21:33:57Z"));
        assert!(same_date("D:20241108090536+09'00'", "2024-11-08T09:05:36+09:00"));
        // fy05.pdf: six days apart, and a real disagreement.
        assert!(!same_date("D:20241114200008+09'00'", "2024-11-08T09:08:18+09:00"));
        // Neither parses, so the strings are all there is to go on.
        assert!(same_date("not a date", "not a date"));
        assert!(!same_date("not a date", "something else"));
    }

    /// A document whose `/Info` and metadata stream disagree, and whose `/Info` carries
    /// entries 14.3.3 deprecates. Built rather than found: no corpus file has both a
    /// title in each place *and* a disagreement, which is what has to be exercised.
    fn info_and_xmp_disagree() -> Vec<u8> {
        let xmp = "<?xpacket begin=\"\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\
<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF \
xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
<dc:title>\n   <rdf:Alt>\n      <rdf:li xml:lang=\"x-default\">From the packet</rdf:li>\n   \
</rdf:Alt>\n</dc:title></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end=\"r\"?>";

        let bodies: [String; 5] = [
            "<< /Type /Catalog /Pages 2 0 R /Metadata 4 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string(),
            format!(
                "<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n{xmp}\nendstream",
                xmp.len()
            ),
            "<< /Title (From /Info) /Author (An Author) /Producer (A Producer) \
             /CreationDate (D:20240101000000Z) /ModDate (D:20240101000000Z) >>"
                .to_string(),
        ];

        let mut out = b"%PDF-2.0\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in bodies.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
        }
        let xref_at = out.len();
        out.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
        for at in &offsets {
            out.extend_from_slice(format!("{at:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size 6 /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref_at}\n%%EOF\n"
            )
            .as_bytes(),
        );
        out
    }

    fn open_fixture(bytes: Vec<u8>) -> Document {
        Document::open(bytes::Bytes::from(bytes), &crate::ingest::IngestionOptions::default())
            .expect("the fixture is a readable document")
    }

    #[test]
    fn settling_records_the_disagreement_and_takes_the_packet() {
        let doc = open_fixture(info_and_xmp_disagree());
        let said: Vec<String> = doc.decisions.entries().iter().map(|d| d.found.clone()).collect();
        assert!(
            said.iter().any(|f| f.contains("/Info /Title") && f.contains("From the packet")),
            "the disagreement was not recorded: {said:?}"
        );
        assert_eq!(doc.metadata().title.as_deref(), Some("From the packet"));
    }

    #[test]
    fn settling_moves_the_deprecated_entries_out_of_info() {
        let doc = open_fixture(info_and_xmp_disagree());
        let arena = doc.arena();
        let Some(Object::Dictionary(dh)) = arena.get_object(doc.info_handle().expect("an /Info"))
        else {
            panic!("/Info is not a dictionary")
        };
        let dict = arena.get_dict(dh).expect("a readable /Info");
        for gone in ["Title", "Author", "Producer"] {
            assert!(!dict.contains_key(&arena.name(gone)), "/{gone} survived in /Info");
        }
        // 14.3.3 still allows these two, so they stay.
        for kept in ["CreationDate", "ModDate"] {
            assert!(dict.contains_key(&arena.name(kept)), "/{kept} was removed from /Info");
        }
        // What left /Info is reachable, which is the point of moving it rather than
        // dropping it: /Author has nowhere else to live in this file.
        assert_eq!(doc.metadata().author.as_deref(), Some("An Author"));
    }

    /// With settling off the document keeps both states and says nothing, which is what
    /// every reading did before ADR-0013.
    #[test]
    fn settling_can_be_turned_off() {
        let options =
            crate::ingest::IngestionOptions { sublime_metadata: false, ..Default::default() };
        let doc = Document::open(bytes::Bytes::from(info_and_xmp_disagree()), &options)
            .expect("the fixture is a readable document");
        assert!(doc.decisions.is_conforming(), "{:?}", doc.decisions.entries());
        let arena = doc.arena();
        let Some(Object::Dictionary(dh)) = arena.get_object(doc.info_handle().expect("an /Info"))
        else {
            panic!("/Info is not a dictionary")
        };
        assert!(arena.get_dict(dh).expect("a readable /Info").contains_key(&arena.name("Title")));
    }

    /// XMP indents. `dc:title` is an `rdf:Alt` of `rdf:li`, so the first text node
    /// inside it is the whitespace before `<rdf:Alt>` — reading that and calling it the
    /// title emptied the title of every file whose packet was written with indentation.
    #[test]
    fn an_indented_language_alternative_yields_its_value_not_its_indentation() {
        let xml = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"
            xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
            xmlns:dc="http://purl.org/dc/elements/1.1/">
          <rdf:RDF><rdf:Description>
            <dc:title>
               <rdf:Alt>
                  <rdf:li xml:lang="x-default">Intel 64 Manual</rdf:li>
               </rdf:Alt>
            </dc:title>
          </rdf:Description></rdf:RDF>
        </x:xmpmeta>"#;
        let doc = roxmltree::Document::parse(xml).expect("well formed");
        let dc = "http://purl.org/dc/elements/1.1/";
        assert_eq!(find_tag_text(&doc, dc, "title").as_deref(), Some("Intel 64 Manual"));
    }

    /// A leaf whose text is entirely whitespace is holding that whitespace as its
    /// value. `samples/fy05.pdf` titles itself with a single ideographic space, and
    /// once `/Info` has handed its copy over at ingest the packet is the only place it
    /// lives — so reading it back has to return what was written.
    #[test]
    fn a_whitespace_only_alternative_keeps_its_value() {
        let xml = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"
            xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
            xmlns:dc="http://purl.org/dc/elements/1.1/">
          <rdf:RDF><rdf:Description>
            <dc:title><rdf:Alt><rdf:li xml:lang="x-default">   </rdf:li></rdf:Alt></dc:title>
          </rdf:Description></rdf:RDF>
        </x:xmpmeta>"#;
        let doc = roxmltree::Document::parse(xml).expect("well formed");
        let got = find_tag_text(&doc, "http://purl.org/dc/elements/1.1/", "title");
        assert_eq!(got.as_deref(), Some("   "));
    }

    /// A bare property still works: not every XMP value is wrapped in a container.
    #[test]
    fn a_bare_property_yields_its_own_text() {
        let xml = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"
            xmlns:pdf="http://ns.adobe.com/pdf/1.3/">
          <pdf:Producer>Acrobat Distiller 24.0</pdf:Producer>
        </x:xmpmeta>"#;
        let doc = roxmltree::Document::parse(xml).expect("well formed");
        let ns = "http://ns.adobe.com/pdf/1.3/";
        assert_eq!(find_tag_text(&doc, ns, "Producer").as_deref(), Some("Acrobat Distiller 24.0"));
    }

    #[test]
    fn test_metadata_info_default() {
        let info = MetadataInfo::default();
        assert!(info.title.is_none());
        assert!(info.author.is_none());
    }

    #[test]
    fn test_build_refined_metadata_map() {
        let info = MetadataInfo {
            title: Some("ISO 32000-2 Specification".to_string()),
            author: Some("ISO/TC 171".to_string()),
            ..Default::default()
        };
        let map = build_refined_metadata_map(&info);
        assert!(map.contains_key(&crate::object::PdfName::new("Title")));
        assert!(map.contains_key(&crate::object::PdfName::new("Author")));
    }
}

/// What stripping the metadata streams removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StripReport {
    /// `/Metadata` entries removed, at the catalogue and on individual objects.
    pub entries: usize,
    /// Distinct streams those entries pointed at, which no longer have a referrer.
    pub streams: usize,
}

/// Removes every metadata stream the document carries (14.3.2).
///
/// `--strip` used to remove the catalogue's `/Metadata` and nothing else. On
/// `samples/fy05.pdf` that left 198 of the file's 199 metadata streams in place,
/// carrying a personal name in `dc:creator`, `xmpMM:History` with timestamps and
/// software agents, the names of the fonts used, and thumbnail images of the artwork.
/// A flag that says it strips descriptive metadata and removes one packet in 199 is the
/// shape ADR-0007 hid other flags for, made harder to notice by doing something.
///
/// Removing the entries is enough to remove the streams: the writer traces from the
/// catalogue, so a stream nothing refers to is not written. Measured on the corpus, no
/// XMP packet survives `--strip` in any of the nine files, before or after inflating
/// every stream in the output.
///
/// An earlier version of this also counted packets it believed were embedded inside
/// other streams' payloads and reported them as beyond reach. They were not: they were
/// metadata streams that omit the `/Type` "Table 347" requires, and they go with the
/// rest. The count was an artifact of classifying objects by a search of their first
/// 400 bytes, and it cost a decode of every stream in the file to compute.
pub fn strip_metadata_streams(
    doc: &Document,
    decisions: &mut crate::interpretation::DecisionLog,
) -> StripReport {
    let arena = doc.arena();
    let key = arena.name("Metadata");
    let mut targets = std::collections::BTreeSet::new();
    let mut report = StripReport::default();

    for dh in arena.all_dict_handles() {
        let Some(mut dict) = arena.get_dict(dh) else { continue };
        let Some(value) = dict.remove(&key) else { continue };
        arena.set_dict(dh, dict);
        report.entries += 1;
        if let Object::Reference(h) = value {
            targets.insert(h);
        }
    }
    report.streams = targets.len();
    if report.entries > 0 {
        decisions.push(crate::interpretation::Decision::repaired(
            "14.3.2",
            format!(
                "{} /Metadata entries pointed at {} metadata streams",
                report.entries, report.streams
            ),
            "removed every entry, at the catalogue and on individual objects; the streams              they named are left with no referrer and are not written"
                .to_string(),
        ));
    }
    report
}
