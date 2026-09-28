//! A document's claims about itself: a standard's identification and PDF Declarations
//! (ISO 14289-2 clause 5 and 7.2.2; WTPDF 6.1).
//!
//! **Nothing could make one.** `Upgrade` wrote `/PdfUA 2` and `/GTS_PDFA14` into the
//! catalogue, keys no standard defines, and no XMP; and no operation wrote a declaration,
//! which is how WTPDF says a file is well tagged.

use fepdf::{IngestionOptions, Operation, PdfDocument, PdfStandard};
use fepdf_model::declarations::{WTPDF_ACCESSIBILITY, WTPDF_REUSE, declared};

/// A one-page document, with `packet` as its metadata stream where one is given.
fn document(packet: Option<&str>) -> PdfDocument {
    let mut objects = vec![
        format!(
            "<< /Type /Catalog /Pages 2 0 R {}>>",
            if packet.is_some() { "/Metadata 4 0 R " } else { "" }
        ),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>".to_string(),
    ];
    if let Some(packet) = packet {
        objects.push(format!(
            "<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n{packet}\nendstream",
            packet.len() + 1
        ));
    }
    let bytes = fepdf_fixtures::assemble(&objects);
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default()).expect("it opens")
}

/// A packet around one `rdf:Description`'s namespaces, attributes and content.
fn packet(description: &str) -> String {
    format!(
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF \
         xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
         <rdf:Description rdf:about=\"\" {description}</rdf:Description></rdf:RDF></x:xmpmeta>"
    )
}

/// Written out and read back in.
fn saved(doc: &PdfDocument, name: &str) -> PdfDocument {
    let path = std::env::temp_dir().join(name);
    doc.save_as_version(&path, "2.0").expect("it writes");
    let bytes = std::fs::read(&path).expect("it is on disk");
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default()).expect("reopens")
}

/// The catalogue's metadata stream, as text.
fn text(doc: &PdfDocument) -> String {
    let arena = doc.inner().arena();
    let catalog = doc.inner().resolve_to_dict(doc.inner().catalog_handle().expect("a catalogue"));
    let metadata = arena.dict_entry(catalog.expect("a dictionary"), arena.name("Metadata"));
    let bytes = doc.inner().decode_stream(&metadata.expect("a packet").resolve(arena));
    String::from_utf8(bytes.expect("it decodes").to_vec()).expect("UTF-8")
}

fn declare(uri: &str) -> Operation {
    Operation::DeclareConformance { conforms_to: uri.to_string() }
}

/// **A document identified as PDF/UA-2 and declared WTPDF's accessibility level says both,
/// in its packet, after a save** — `pdfuaid:part` 2 and `pdfuaid:rev` 2024 as clause 5
/// requires, and no catalogue key.
#[test]
fn a_ua2_document_is_identified_and_declared() {
    let mut doc = document(None);
    doc.apply(Operation::Upgrade { standard: PdfStandard::UA2 }).expect("it upgrades");
    doc.apply(declare(WTPDF_ACCESSIBILITY)).expect("it is declared");
    let doc = saved(&doc, "fepdf-declared-ua2.pdf");
    let packet = text(&doc);
    assert_eq!(packet.matches(r#"pdfuaid:part="2""#).count(), 1, "{packet}");
    assert_eq!(packet.matches(r#"pdfuaid:rev="2024""#).count(), 1, "{packet}");
    assert_eq!(declared(doc.inner()), vec![WTPDF_ACCESSIBILITY.to_string()]);

    let arena = doc.inner().arena();
    let catalog = doc.inner().resolve_to_dict(doc.inner().catalog_handle().expect("catalogue"));
    let catalog = arena.get_dict(catalog.expect("a dictionary")).expect("entries");
    assert!(!catalog.contains_key(&arena.name("PdfUA")), "a key no standard defines");
}

/// **An identification restated replaces the one there**: a PDF/UA-1 file upgraded says
/// part 2, once, and no longer part 1.
#[test]
fn upgrading_replaces_the_identification_there() {
    let mut doc = document(Some(&packet(
        "xmlns:pdfuaid=\"http://www.aiim.org/pdfua/ns/id/\" pdfuaid:part=\"1\">",
    )));
    doc.apply(Operation::Upgrade { standard: PdfStandard::UA2 }).expect("it upgrades");
    // Before a save as well as after: saving keeps the first of two, which would hide a
    // second left behind.
    for packet in [text(&doc), text(&saved(&doc, "fepdf-upgraded-ua1.pdf"))] {
        assert_eq!(packet.matches("pdfuaid:part=").count(), 1, "{packet}");
        assert!(packet.contains(r#"pdfuaid:part="2""#), "{packet}");
    }
}

/// **A declaration joins those there, which keep their claim data**: a file declaring the
/// reuse level, claimed by someone, declares both levels, and the claim is still theirs.
#[test]
fn a_declaration_joins_those_there() {
    let mut doc = document(Some(&packet(&format!(
        "xmlns:pdfd=\"http://pdfa.org/declarations/\"><pdfd:declarations><rdf:Bag>\
         <rdf:li rdf:parseType=\"Resource\"><pdfd:conformsTo>{WTPDF_REUSE}</pdfd:conformsTo>\
         <pdfd:claimData><rdf:Bag><rdf:li rdf:parseType=\"Resource\">\
         <pdfd:claimBy>Someone</pdfd:claimBy></rdf:li></rdf:Bag></pdfd:claimData>\
         </rdf:li></rdf:Bag></pdfd:declarations>"
    ))));
    doc.apply(declare(WTPDF_ACCESSIBILITY)).expect("it is declared");
    let doc = saved(&doc, "fepdf-declared-both.pdf");
    assert_eq!(
        declared(doc.inner()),
        vec![WTPDF_REUSE.to_string(), WTPDF_ACCESSIBILITY.to_string()]
    );
    assert_eq!(text(&doc).matches("<pdfd:claimBy>Someone</pdfd:claimBy>").count(), 1);
}

/// Declared twice, it is declared once; declared as nothing, it is refused.
#[test]
fn declaring_again_or_declaring_nothing() {
    let mut doc = document(None);
    doc.apply(declare(WTPDF_REUSE)).expect("it is declared");
    doc.apply(declare(WTPDF_REUSE)).expect("again");
    assert_eq!(declared(doc.inner()), vec![WTPDF_REUSE.to_string()]);
    assert!(doc.apply(declare("  ")).is_err(), "a declaration of nothing was written");
}
