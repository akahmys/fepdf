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

/// Collects document metadata, preferring XMP over the `/Info` dictionary.
pub fn extract_metadata(doc: &Document) -> MetadataInfo {
    let arena = doc.arena();
    let mut info = MetadataInfo::default();

    // 1. Extract from legacy Info dictionary
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

    // 2. Supplement with XMP Metadata from Catalog/Metadata stream
    if let Some(Object::Dictionary(catalog_handle)) = arena.get_object(*doc.root_handle())
        && let Some(catalog_dict) = arena.get_dict(catalog_handle)
        && let Some(metadata_obj) = catalog_dict.get(&arena.name("Metadata"))
    {
        let resolved = metadata_obj.resolve(arena);
        if let Ok(xml_data) = doc.decode_stream(&resolved)
            && let Ok(xml_str) = std::str::from_utf8(&xml_data)
            && let Ok(xml_doc) = roxmltree::Document::parse(xml_str)
        {
            apply_xmp_metadata(&xml_doc, &mut info);
        }
    }

    info
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
fn find_tag_text(doc: &roxmltree::Document, ns: &str, tag: &str) -> Option<String> {
    let node = doc.descendants().find(|n| n.has_tag_name((ns, tag)))?;
    // A container yields its first item; a bare property yields its own text.
    let source = node.descendants().find(|n| n.has_tag_name(RDF_LI)).unwrap_or(node);
    let text: String = source.children().filter(|n| n.is_text()).filter_map(|n| n.text()).collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
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

    /// A property that is whitespace only has no value, and must not displace the one
    /// the `/Info` dictionary carries. `samples/fy05.pdf` is this case.
    #[test]
    fn a_whitespace_only_alternative_is_absent_rather_than_empty() {
        let xml = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"
            xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
            xmlns:dc="http://purl.org/dc/elements/1.1/">
          <rdf:RDF><rdf:Description>
            <dc:title><rdf:Alt><rdf:li xml:lang="x-default">   </rdf:li></rdf:Alt></dc:title>
          </rdf:Description></rdf:RDF>
        </x:xmpmeta>"#;
        let doc = roxmltree::Document::parse(xml).expect("well formed");
        assert_eq!(find_tag_text(&doc, "http://purl.org/dc/elements/1.1/", "title"), None);
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
