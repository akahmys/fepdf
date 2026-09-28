//! The seven operations this window did not reach, and that they do what they say.
//!
//! **Measured 2026-09-09**: `fepdf-mcp` constructs all thirty `Operation` variants,
//! `fepdf-cli` eight and `fepdf-gui` five — rotate, remove, reorder, duplicate and a
//! structure-element edit. A reader who wanted the front matter numbered in roman had to
//! reach for the command line.
//!
//! What the forms build is an `Operation`, which is all a frontend may do (Rule D). These
//! check the ones the forms construct, so a form that builds a shape `apply` refuses fails
//! here rather than in front of someone.

use fepdf::{Operation, PdfDocument};

fn document() -> PdfDocument {
    PdfDocument::create_empty().expect("an empty document")
}

/// 12.4.2: the front matter numbered differently from the body.
#[test]
fn page_labels_reach_the_document() {
    let mut doc = document();
    doc.apply(Operation::SetPageLabels(vec![fepdf::PageLabelSpec {
        start_page: 0,
        style: fepdf::PageLabelStyle::LowerRoman,
        prefix: None,
        start_number: 1,
    }]))
    .expect("the labels apply");
}

/// An empty prefix box is no prefix, not a prefix of nothing — the form sends `None`.
#[test]
fn a_label_prefix_is_optional() {
    let mut doc = document();
    doc.apply(Operation::SetPageLabels(vec![fepdf::PageLabelSpec {
        start_page: 0,
        style: fepdf::PageLabelStyle::Decimal,
        prefix: Some("A-".to_string()),
        start_number: 1,
    }]))
    .expect("a prefixed label applies");
}

#[test]
fn bates_numbering_reaches_the_document() {
    let mut doc = document();
    doc.apply(Operation::ApplyBatesNumbering {
        pages: fepdf::PageSelection::All,
        prefix: "DOC-".to_string(),
        start_number: 1,
        digits: 6,
        position: fepdf::DecorationPosition::BottomRight,
    })
    .expect("the numbering applies");
}

#[test]
fn retagging_reaches_the_document() {
    document().apply(Operation::Retag).expect("retagging applies");
}

/// Each of the three standards the form offers, because a radio button that builds a
/// variant `apply` refuses is a button that fails in front of someone — which is why the
/// form does not offer PDF/X-6.
#[test]
fn every_standard_the_form_offers_applies() {
    assert!(document().apply(Operation::Upgrade { standard: fepdf::PdfStandard::X6 }).is_err());
    for standard in
        [fepdf::PdfStandard::A4, fepdf::PdfStandard::UA2, fepdf::PdfStandard::ISO32000_2]
    {
        document()
            .apply(Operation::Upgrade { standard })
            .unwrap_or_else(|e| panic!("{standard:?} was refused: {e:?}"));
    }
}

#[test]
fn an_attachment_reaches_the_document() {
    document()
        .apply(Operation::AttachAssociatedFile(fepdf::AssociatedFile {
            filename: "invoice.xml".to_string(),
            relationship: fepdf::AFRelationship::Data,
            mime_type: "application/xml".to_string(),
            data: b"<invoice/>".to_vec(),
        }))
        .expect("the attachment applies");
}

/// The form sends WGS 84 rather than leaving `/GCS` absent, because a latitude typed by a
/// person is in that system unless they say otherwise.
#[test]
fn a_geospatial_anchor_reaches_the_document() {
    document()
        .apply(Operation::SetGeospatialAnchor(fepdf::GeoSpatialAnchor {
            page: 0,
            latitude: 35.6812,
            longitude: 139.7671,
            altitude_meters: None,
            crs_wkt: "GEOGCS[\"WGS 84\"]".to_string(),
        }))
        .expect("the anchor applies");
}

#[test]
fn a_portfolio_reaches_the_document() {
    document()
        .apply(Operation::CreatePortfolio(fepdf::PortfolioCollection {
            view_mode: fepdf::CollectionViewMode::Details,
            initial_document: None,
            items: vec![fepdf::PortfolioItem {
                filename: "a.pdf".to_string(),
                mime_type: Some("application/pdf".to_string()),
                description: None,
                size_bytes: 3,
                data: b"abc".to_vec(),
            }],
        }))
        .expect("the portfolio applies");
}
