//! What a file claims about itself in XMP survives being opened and saved.
//!
//! **It did not.** The packet is rebuilt from the fields the engine models, at ingest and
//! at save, and a PDF/UA-2 file came out without `pdfuaid:part` and `pdfuaid:rev`, and a
//! WTPDF file without its `pdfd:declarations` — every one of the 138 files in the veraPDF
//! PDF/UA-2 corpus, measured 2026-09-28. Nor did two the engine writes itself, because it
//! did not read them: a property written as an attribute, and `dc:rights`.

use fepdf::{IngestionOptions, PdfDocument};

const PACKET: &str = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:pdfuaid="http://www.aiim.org/pdfua/ns/id/" xmlns:pdfd="http://pdfa.org/declarations/" xmlns:xmp="http://ns.adobe.com/xap/1.0/" pdfuaid:part="2" pdfuaid:rev="2024" xmp:CreatorTool="Maker"><dc:title><rdf:Alt><rdf:li xml:lang="x-default">Claims</rdf:li></rdf:Alt></dc:title><dc:rights><rdf:Alt><rdf:li xml:lang="x-default">(c) Someone</rdf:li></rdf:Alt></dc:rights><pdfd:declarations><rdf:Bag><rdf:li rdf:parseType="Resource"><pdfd:conformsTo>http://pdfa.org/declarations/wtpdf/#accessibility1.0</pdfd:conformsTo></rdf:li></rdf:Bag></pdfd:declarations></rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end="r"?>"#;

/// The catalogue's metadata stream, as text.
fn packet(doc: &PdfDocument) -> String {
    let arena = doc.inner().arena();
    let catalog = doc.inner().catalog_handle().expect("a catalogue");
    let dict = doc.inner().resolve_to_dict(catalog).expect("a dictionary");
    let metadata = arena.dict_entry(dict, arena.name("Metadata")).expect("a metadata stream");
    let bytes = doc.inner().decode_stream(&metadata.resolve(arena)).expect("it decodes");
    String::from_utf8(bytes.to_vec()).expect("UTF-8")
}

#[test]
fn a_ua2_and_wtpdf_claim_survives_a_save() {
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R /Metadata 4 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
        format!(
            "<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n{PACKET}\nendstream",
            PACKET.len() + 1
        ),
    ]);
    let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("it opens");
    let path = std::env::temp_dir().join("fepdf-xmp-claims.pdf");
    doc.save_as_version(&path, "2.0").expect("it writes");
    let written = std::fs::read(&path).expect("it is on disk");
    let reopened = PdfDocument::open_with_options(written.into(), &IngestionOptions::default())
        .expect("it reopens");
    let text = packet(&reopened);
    for claim in [
        r#"pdfuaid:part="2""#,
        r#"pdfuaid:rev="2024""#,
        "http://pdfa.org/declarations/wtpdf/#accessibility1.0",
    ] {
        assert_eq!(text.matches(claim).count(), 1, "{claim} in {text}");
    }
    assert!(text.contains("Claims"), "the title was lost: {text}");
    // Properties the engine writes itself are read first: a simple one written as an
    // attribute (XMP Part 1, 7.9.2.2), and `dc:rights`, which was written and never read.
    assert!(text.contains("<xmp:CreatorTool>Maker</xmp:CreatorTool>"), "{text}");
    assert!(text.contains("(c) Someone"), "the notice was lost: {text}");
}
