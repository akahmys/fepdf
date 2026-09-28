//! PDF Declarations (PDF Association, 2019): what a document claims to conform to, as a
//! `pdfd:conformsTo` in its XMP metadata.
//!
//! **A declaration is its author's statement** (ISO 14289-2 7.2.2, EXAMPLE 1): WTPDF 6.1
//! requires one naming each conformance level a file claims, and this engine checks
//! nothing of WTPDF. What is declared is the caller's to say.

use crate::refine::xmp_carry::{RDF, descriptions, escape};
use crate::{Document, PdfError, PdfResult};
use std::collections::BTreeMap;

/// The PDF Declarations namespace.
pub const NAMESPACE: &str = "http://pdfa.org/declarations/";

/// WTPDF 6.1.2's conformance level for reuse.
pub const WTPDF_REUSE: &str = "http://pdfa.org/declarations/wtpdf/#reuse1.0";

/// WTPDF 6.1.3's conformance level for accessibility.
pub const WTPDF_ACCESSIBILITY: &str = "http://pdfa.org/declarations/wtpdf/#accessibility1.0";

/// The declarations a packet holds: each `rdf:li` as written, its `pdfd:conformsTo`, and
/// the namespaces in scope where they are written.
struct Declared {
    items: Vec<(String, String)>,
    namespaces: BTreeMap<String, String>,
}

fn read(packet: &str) -> Declared {
    let mut declared = Declared { items: Vec::new(), namespaces: BTreeMap::new() };
    let Ok(xml) = roxmltree::Document::parse(packet) else { return declared };
    let is = |n: &roxmltree::Node<'_, '_>, ns: &str, name: &str| {
        n.tag_name().namespace() == Some(ns) && n.tag_name().name() == name
    };
    for description in descriptions(&xml) {
        let Some(declarations) = description.children().find(|n| is(n, NAMESPACE, "declarations"))
        else {
            continue;
        };
        for ns in declarations.namespaces() {
            if let Some(prefix) = ns.name().filter(|p| !["xml", "rdf"].contains(p)) {
                declared.namespaces.entry(prefix.to_string()).or_insert(ns.uri().to_string());
            }
        }
        let bag = declarations.children().filter(|n| is(n, RDF, "Bag"));
        for item in bag.flat_map(|b| b.children()).filter(|n| is(n, RDF, "li")) {
            let conforms = item.descendants().find(|n| is(n, NAMESPACE, "conformsTo"));
            let uri = conforms.and_then(|c| c.text()).unwrap_or_default().trim().to_string();
            if let Some(written) = xml.input_text().get(item.range()) {
                declared.items.push((written.to_string(), uri));
            }
        }
    }
    declared
}

/// What the document declares conformity with: each `pdfd:conformsTo` in its packet.
#[must_use]
pub fn declared(doc: &Document) -> Vec<String> {
    crate::metadata::catalog_packet(doc)
        .map(|packet| read(&packet).items.into_iter().map(|(_, uri)| uri).collect())
        .unwrap_or_default()
}

/// Declares conformity with `uri`, beside the declarations the document has, which keep
/// what they carry — `pdfd:claimData` among it. Declaring it again changes nothing.
///
/// # Errors
/// For an empty `uri`; and if the catalogue is not a dictionary.
pub fn declare(doc: &Document, uri: &str) -> PdfResult<()> {
    let uri = uri.trim();
    if uri.is_empty() {
        return Err(PdfError::Other("a declaration names what it conforms to".into()));
    }
    let Declared { items, mut namespaces } =
        crate::metadata::catalog_packet(doc).map(|p| read(&p)).unwrap_or_else(|| read(""));
    if items.iter().any(|(_, declared)| declared == uri) {
        return Ok(());
    }
    namespaces.insert("pdfd".to_string(), NAMESPACE.to_string());
    let mut description = String::from("<rdf:Description rdf:about=\"\"");
    for (prefix, ns) in &namespaces {
        description.push_str(&format!(" xmlns:{prefix}=\"{}\"", escape(ns)));
    }
    description.push_str("><pdfd:declarations><rdf:Bag>");
    for (written, _) in &items {
        description.push_str(written);
    }
    description.push_str(&format!(
        "<rdf:li rdf:parseType=\"Resource\"><pdfd:conformsTo>{}</pdfd:conformsTo></rdf:li>",
        escape(uri)
    ));
    description.push_str("</rdf:Bag></pdfd:declarations></rdf:Description>");
    crate::metadata::state_in_packet(doc, &description)
}
