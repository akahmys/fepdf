//! What an XMP packet says that this engine does not write, carried into the one it does.
//!
//! **The packet is rebuilt from the nine fields the engine models**, at ingest and at every
//! save, and a rebuilt packet held nothing else: a PDF/UA-2 file's `pdfuaid:part` and
//! `pdfuaid:rev`, its WTPDF `pdfd:declarations`, a PDF/A extension schema — each was gone
//! from the output of opening a file and saving it. A claim a file makes about itself is
//! not the engine's to drop.
//!
//! **What is carried is what the generator does not own**, and what it owns is read from
//! the generator: one packet rendered with every field it knows filled in, whose property
//! names are the list. A property it owns is its to write or leave out — a title the
//! caller removed stays removed — and a property it does not own is copied as written, in
//! an `rdf:Description` of its own with the namespaces it was written in.

use crate::document::Provenance;
use crate::object::PdfName;
use crate::refine::RefinedObject;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

/// The RDF namespace.
pub(crate) const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

/// A property's namespace and local name.
type Name = (String, String);

/// Every property the generator can write: those of a packet rendered with every field set.
static OWNED: LazyLock<BTreeSet<Name>> = LazyLock::new(|| {
    let fields = [
        "Title",
        "Author",
        "Subject",
        "Keywords",
        "Creator",
        "Producer",
        "CreationDate",
        "ModDate",
        "Rights",
    ];
    let info: BTreeMap<PdfName, RefinedObject> = fields
        .into_iter()
        .map(|f| (PdfName::new(f), RefinedObject::Text("D:20260101000000Z".into())))
        .collect();
    let provenance = Provenance {
        source_id: Some("uuid:source".into()),
        original_id: Some("uuid:original".into()),
        ..Provenance::default()
    };
    let packet = super::metadata::info_to_xmp_derived(&info, &provenance, 0);
    let Ok(xml) = roxmltree::Document::parse(&packet) else { return BTreeSet::new() };
    descriptions(&xml).flat_map(|d| properties(d).map(|(name, _)| name)).collect()
});

/// The `rdf:Description`s of a packet: the ones `rdf:RDF` holds, not those a property's
/// value is written as, which belong to the property.
pub(crate) fn descriptions<'a>(
    xml: &'a roxmltree::Document<'a>,
) -> impl Iterator<Item = roxmltree::Node<'a, 'a>> {
    let rdf = |n: &roxmltree::Node<'_, '_>, name: &str| {
        n.tag_name().namespace() == Some(RDF) && n.tag_name().name() == name
    };
    xml.descendants()
        .filter(move |n| rdf(n, "Description"))
        .filter(move |n| n.parent_element().is_some_and(|p| rdf(&p, "RDF")))
}

/// A description's properties, each with the text it is written as, prefix and all: ISO
/// 14289-2 clause 5 requires the prefix `pdfuaid`, and a packet declaring a second prefix
/// for its namespace would otherwise come out with the other one.
fn properties<'a>(
    description: roxmltree::Node<'a, 'a>,
) -> impl Iterator<Item = (Name, String)> + 'a {
    let source = description.document().input_text();
    let attributes = description.attributes().filter_map(move |a| {
        let ns = a.namespace().filter(|ns| *ns != RDF)?;
        Some(((ns.to_string(), a.name().to_string()), source.get(a.range())?.to_string()))
    });
    let elements =
        description.children().filter(roxmltree::Node::is_element).filter_map(move |e| {
            let ns = e.tag_name().namespace()?;
            let name = (ns.to_string(), e.tag_name().name().to_string());
            Some((name, source.get(e.range())?.to_string()))
        });
    attributes.chain(elements)
}

/// A namespace URI, escaped to be written between double quotes.
pub(crate) fn escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;")
}

/// An `rdf:Description` declaring `namespaces`, with `attributes` and `elements`.
fn description(
    namespaces: &BTreeMap<String, String>,
    attributes: &[String],
    elements: &[String],
) -> String {
    let mut carried = String::from("<rdf:Description rdf:about=\"\"");
    for (prefix, uri) in namespaces {
        if prefix != "rdf" {
            carried.push_str(&format!(" xmlns:{prefix}=\"{}\"", escape(uri)));
        }
    }
    for attribute in attributes {
        carried.push(' ');
        carried.push_str(attribute);
    }
    carried.push('>');
    for element in elements {
        carried.push_str(element);
    }
    carried.push_str("</rdf:Description>");
    carried
}

/// `generated` with what `original` says that the generator does not own and `generated`
/// does not already say, in an `rdf:Description` of its own before `</rdf:RDF>`;
/// `generated` itself where there is nothing to carry or `original` is not XML.
#[must_use]
pub fn carry(original: &str, generated: String) -> String {
    let Ok(xml) = roxmltree::Document::parse(original) else { return generated };
    // What the new packet states replaces what the old one did: a claim restated.
    let stated: BTreeSet<Name> = roxmltree::Document::parse(&generated)
        .map(|g| descriptions(&g).flat_map(|d| properties(d).map(|(n, _)| n)).collect())
        .unwrap_or_default();
    let mut namespaces = BTreeMap::new();
    let mut attributes = Vec::new();
    let mut elements = Vec::new();
    let mut seen = BTreeSet::new();
    for description in descriptions(&xml) {
        let mut any = false;
        for (name, written) in properties(description) {
            if OWNED.contains(&name) || stated.contains(&name) || !seen.insert(name) {
                continue;
            }
            any = true;
            if written.starts_with('<') {
                elements.push(written);
            } else {
                attributes.push(written);
            }
        }
        if any {
            for ns in description.namespaces() {
                if let Some(prefix) = ns.name().filter(|p| *p != "xml") {
                    namespaces.entry(prefix.to_string()).or_insert_with(|| ns.uri().to_string());
                }
            }
        }
    }
    let Some(at) = generated.rfind("</rdf:RDF>") else { return generated };
    if attributes.is_empty() && elements.is_empty() {
        return generated;
    }
    let carried = description(&namespaces, &attributes, &elements);
    let mut packet = generated;
    packet.insert_str(at, &carried);
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PDF/UA-2 file's packet: a title the generator owns, its identification as
    /// attributes, a WTPDF declaration as an element, and an extension schema whose value
    /// is written as nested `rdf:Description`s.
    const UA2: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/"
  xmlns:pdfuaid="http://www.aiim.org/pdfua/ns/id/" xmlns:pdfd="http://pdfa.org/declarations/"
  xmlns:pdfaExtension="http://www.aiim.org/pdfa/ns/extension/" xmlns:pdfaSchema="http://www.aiim.org/pdfa/ns/schema#"
  pdfuaid:part="2" pdfuaid:rev="2024">
 <dc:title><rdf:Alt><rdf:li xml:lang="x-default">Old</rdf:li></rdf:Alt></dc:title>
 <pdfd:declarations><rdf:Bag><rdf:li rdf:parseType="Resource">
  <pdfd:conformsTo>http://pdfa.org/declarations/wtpdf/#reuse1.0</pdfd:conformsTo>
 </rdf:li></rdf:Bag></pdfd:declarations>
 <pdfaExtension:schemas><rdf:Bag><rdf:li><rdf:Description pdfaSchema:prefix="pdfuaid"/></rdf:li></rdf:Bag></pdfaExtension:schemas>
</rdf:Description></rdf:RDF></x:xmpmeta>"#;

    /// A packet the generator renders with no fields: what a caller who removed the title
    /// is left with.
    fn generated() -> String {
        super::super::metadata::info_to_xmp_derived(&BTreeMap::new(), &Provenance::default(), 0)
    }

    /// Each property of `packet`'s top-level descriptions, by namespace and name.
    fn names(packet: &str) -> Vec<Name> {
        let xml = roxmltree::Document::parse(packet).expect("the packet is XML");
        descriptions(&xml).flat_map(|d| properties(d).map(|(name, _)| name)).collect()
    }

    fn has(packet: &str, ns: &str, name: &str) -> usize {
        names(packet).iter().filter(|(n, l)| n == ns && l == name).count()
    }

    #[test]
    fn a_claim_the_file_makes_is_carried() {
        let packet = carry(UA2, generated());
        assert_eq!(has(&packet, "http://www.aiim.org/pdfua/ns/id/", "part"), 1, "{packet}");
        assert_eq!(has(&packet, "http://www.aiim.org/pdfua/ns/id/", "rev"), 1, "{packet}");
        assert_eq!(has(&packet, "http://pdfa.org/declarations/", "declarations"), 1, "{packet}");
        assert!(packet.contains("wtpdf/#reuse1.0"), "{packet}");
    }

    /// **The generator's own properties are its to leave out**: carrying `dc:title` from
    /// the packet in place would bring back a title the caller removed.
    #[test]
    fn a_property_the_generator_owns_is_not_carried() {
        let packet = carry(UA2, generated());
        assert_eq!(has(&packet, "http://purl.org/dc/elements/1.1/", "title"), 0, "{packet}");
        assert!(!packet.contains("Old"), "{packet}");
    }

    /// **A value written as a description belongs to its property**: the extension
    /// schema's inner description comes with `pdfaExtension:schemas`, and its
    /// `pdfaSchema:prefix` is not made a property of the document.
    #[test]
    fn a_description_inside_a_value_stays_there() {
        let packet = carry(UA2, generated());
        assert_eq!(has(&packet, "http://www.aiim.org/pdfa/ns/extension/", "schemas"), 1);
        assert_eq!(has(&packet, "http://www.aiim.org/pdfa/ns/schema#", "prefix"), 0, "{packet}");
    }

    /// Saved twice, a claim is there once: the second save carries it out of the first's
    /// packet, where it is in a description of its own.
    #[test]
    fn carrying_twice_is_carrying_once() {
        let once = carry(UA2, generated());
        let twice = carry(&once, generated());
        assert_eq!(has(&twice, "http://www.aiim.org/pdfua/ns/id/", "part"), 1, "{twice}");
        assert_eq!(names(&once).len(), names(&twice).len());
    }

    /// **A property keeps the prefix it was written with**: ISO 14289-2 clause 5 requires
    /// `pdfuaid`, and a packet that also binds `pdfuadd` to the namespace came out with it.
    #[test]
    fn a_property_keeps_its_prefix() {
        let original = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:pdfuaid="http://www.aiim.org/pdfua/ns/id/" xmlns:pdfuadd="http://www.aiim.org/pdfua/ns/id/" pdfuaid:part="2" pdfuadd:rev="2024"/></rdf:RDF></x:xmpmeta>"#;
        let packet = carry(original, generated());
        // One namespace, two prefixes: a prefix looked up rather than copied is one of the
        // two, and rewrites the other property.
        assert!(packet.contains(r#"pdfuaid:part="2""#), "{packet}");
        assert!(packet.contains(r#"pdfuadd:rev="2024""#), "{packet}");
    }

    #[test]
    fn a_packet_that_is_not_xml_carries_nothing() {
        // One packet: its `InstanceID` is salted with the time.
        let packet = generated();
        assert_eq!(carry("<x:xmpmeta", packet.clone()), packet);
    }
}
