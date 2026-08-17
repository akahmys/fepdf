//! Integration tests for the public `fepdf` facade surface.
//!
//! Placed under `tests/` per RR-15 Rule 14 (Test Code Separation).

#![allow(clippy::float_cmp)]
use bytes::Bytes;
use fepdf::{PdfDocument, PdfStandard};
use std::fmt::Write as _;

/// Assembles indirect objects into a file, cross-reference and trailer included.
///
/// Written out by hand rather than by a library, so the tests exercise the reader on
/// bytes a producer would actually emit, offsets and all. Objects are numbered from 1
/// in the order given, and `root` names which of them is the catalogue.
fn assemble(bodies: &[String], root: usize) -> Bytes {
    let mut out = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (index, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{body}\nendobj\n", index + 1);
    }

    let startxref = out.len();
    let size = bodies.len() + 1;
    let _ = write!(out, "xref\n0 {size}\n0000000000 65535 f \n");
    for offset in &offsets {
        let _ = writeln!(out, "{offset:010} 00000 n ");
    }
    let id = "<0123456789abcdef0123456789abcdef>";
    let _ = write!(
        out,
        "trailer\n<< /Size {size} /Root {root} 0 R /ID [{id} {id}] >>\n\
         startxref\n{startxref}\n%%EOF\n"
    );
    Bytes::from(out)
}

/// The smallest conforming document: a catalogue and an empty page tree.
fn get_minimal_pdf() -> Bytes {
    assemble(
        &[
            "<< /Type /Pages /Kids [] /Count 0 >>".to_string(),
            "<< /Type /Catalog /Pages 1 0 R >>".to_string(),
        ],
        2,
    )
}

#[test]
fn test_document_save_settings_sync() {
    let data = get_minimal_pdf();
    let mut doc = PdfDocument::open(data).unwrap();

    // Test initial states
    assert!(!doc.vacuum());
    assert!(!doc.strip());
    assert!(doc.password().is_none());

    // Modify states
    doc.set_vacuum(true);
    doc.set_strip(true);
    doc.set_password(Some("secret".to_string()));

    // Verify mutations
    assert!(doc.vacuum());
    assert!(doc.strip());
    assert_eq!(doc.password(), Some("secret"));

    // Verify SaveOptions serialization syncing
    let file_path = std::env::temp_dir().join("fepdf_test_output.pdf");
    let res = doc.save_as_version(&file_path, "2.0");
    assert!(res.is_ok());

    // Cleanup
    let _ = std::fs::remove_file(file_path);
}

#[test]
fn test_upgrade_to_standard() {
    let data = get_minimal_pdf();
    let mut doc = PdfDocument::open(data).unwrap();

    // Verify base version
    assert_eq!(doc.inner().arena().version(), 1.7);

    // Upgrade to modern PDF 2.0
    doc.upgrade_to_standard(PdfStandard::ISO32000_2).unwrap();
    assert_eq!(doc.inner().arena().version(), 2.0);

    // Upgrade to PDF/A-4 standard and check GTS tag
    doc.upgrade_to_standard(PdfStandard::A4).unwrap();
    assert_eq!(doc.inner().arena().version(), 2.0);

    let cah = doc.inner().catalog_handle().unwrap();
    let cadh = doc.inner().resolve_to_dict(cah).unwrap();
    let catalog = doc.inner().arena().get_dict(cadh).unwrap();
    let gts_key = doc.inner().arena().name("GTS_PDFA14");
    assert!(catalog.contains_key(&gts_key));
}

#[test]
fn test_heuristic_retag_execution() {
    let data = get_minimal_pdf();
    let mut doc = PdfDocument::open(data).unwrap();
    let res = doc.retag_document();
    assert!(res.is_ok());
}

#[test]
fn test_cielab_to_srgb_conversion() {
    use fepdf_model::graphics::Color;
    // Test pure black: L=0, a=0, b=0 -> Rgb(0, 0, 0)
    let lab_black = Color::Lab(0.0, 0.0, 0.0);
    assert_eq!(lab_black.to_rgb(), Color::Rgb(0.0, 0.0, 0.0));

    // Test white point D65 reference: L=100, a=0, b=0 -> Rgb(1, 1, 1)
    let lab_white = Color::Lab(100.0, 0.0, 0.0);
    let rgb_white = lab_white.to_rgb();
    match rgb_white {
        Color::Rgb(r, g, b) => {
            assert!((r - 1.0).abs() < 1e-4);
            assert!((g - 1.0).abs() < 1e-4);
            assert!((b - 1.0).abs() < 1e-4);
        }
        Color::Gray(_) | Color::Cmyk(..) | Color::Lab(..) => panic!("Expected Rgb"),
    }
}

/// Revision 5 and 6 derive their key from `/U`, `/UE`, `/O` and `/OE`, and from no
/// part of `/ID`.
///
/// This asserted the reverse until it was measured: `new_v5` took a file id, invented
/// salts from it, and returned `Ok` for every password — so the test passed while the
/// handler produced a key that could not decrypt anything. Passing on a wrong answer is
/// worse than failing, and a test written from the code rather than the clause will do
/// that indefinitely.
#[test]
fn test_aes256_requires_the_documents_own_strings() {
    use fepdf::{AesV5Spec, SecurityHandler};
    let spec = AesV5Spec {
        u: &[0u8; 48],
        ue: &[0u8; 32],
        o: &[0u8; 48],
        oe: &[0u8; 32],
        revision: 6,
        encrypt_metadata: true,
    };
    assert!(
        SecurityHandler::new_aes256("password", &spec).is_none(),
        "a password that authenticates against neither /U nor /O must not build a handler"
    );
}

#[test]
fn test_open_invalid_pdf_bytes() {
    let invalid_bytes = Bytes::from_static(b"NOT A VALID PDF HEADER");
    let doc = PdfDocument::open(invalid_bytes);
    assert!(doc.is_err());
}

#[test]
fn test_document_page_count() {
    let data = get_minimal_pdf();
    let doc = PdfDocument::open(data).unwrap();
    assert_eq!(doc.page_count().unwrap(), 0);
}

/// A document with `count` pages, all children of one page tree node.
fn get_multipage_pdf(count: usize) -> Bytes {
    let pages_number = count + 1;
    let mut bodies: Vec<String> = (0..count)
        .map(|_| format!("<< /Type /Page /Parent {pages_number} 0 R /MediaBox [0 0 612 792] >>"))
        .collect();

    let kids: Vec<String> = (1..=count).map(|n| format!("{n} 0 R")).collect();
    bodies.push(format!("<< /Type /Pages /Kids [{}] /Count {count} >>", kids.join(" ")));
    bodies.push(format!("<< /Type /Catalog /Pages {pages_number} 0 R >>"));
    assemble(&bodies, count + 2)
}

#[test]
fn test_reorder_and_remove_pages() {
    let data = get_multipage_pdf(3);
    let mut doc = PdfDocument::open(data).unwrap();
    assert_eq!(doc.page_count().unwrap(), 3);

    // Test reorder page
    assert!(doc.reorder_page(0, 2).is_ok());
    assert_eq!(doc.page_count().unwrap(), 3);

    // Test remove page
    assert!(doc.remove_page(1).is_ok());
    assert_eq!(doc.page_count().unwrap(), 2);
}

#[test]
fn test_insert_pages_from() {
    let data1 = get_multipage_pdf(2);
    let data2 = get_multipage_pdf(3);
    let mut doc1 = PdfDocument::open(data1).unwrap();
    let doc2 = PdfDocument::open(data2).unwrap();

    assert_eq!(doc1.page_count().unwrap(), 2);
    assert_eq!(doc2.page_count().unwrap(), 3);

    let inserted = doc1.insert_pages_from(&doc2, 1).unwrap();
    assert_eq!(inserted, 3);
    assert_eq!(doc1.page_count().unwrap(), 5);
}

#[test]
fn test_page_rotation() {
    let data = get_multipage_pdf(2);
    let mut doc = PdfDocument::open(data).unwrap();

    assert_eq!(doc.get_page_rotation(0).unwrap(), 0);
    let (w0, h0) = doc.get_page_size(0).unwrap();

    assert!(doc.set_page_rotation(0, 90).is_ok());
    assert_eq!(doc.get_page_rotation(0).unwrap(), 90);
    let (w90, h90) = doc.get_page_size(0).unwrap();
    assert_eq!((w90, h90), (h0, w0));

    assert!(doc.set_page_rotation(0, 180).is_ok());
    assert_eq!(doc.get_page_rotation(0).unwrap(), 180);
    let (w180, h180) = doc.get_page_size(0).unwrap();
    assert_eq!((w180, h180), (w0, h0));

    assert!(doc.set_page_rotation(0, 270).is_ok());
    assert_eq!(doc.get_page_rotation(0).unwrap(), 270);
    let (w270, h270) = doc.get_page_size(0).unwrap();
    assert_eq!((w270, h270), (h0, w0));
}

#[test]
fn rotate_absolute_sets_the_angle_regardless_of_current_rotation() {
    let mut doc = PdfDocument::open(get_multipage_pdf(2)).unwrap();
    doc.set_page_rotation(0, 90).unwrap();

    doc.apply(fepdf::Operation::Rotate {
        pages: fepdf::PageSelection::Single(0),
        mode: fepdf::RotateMode::Absolute(fepdf::Quarter::Q90),
    })
    .unwrap();

    // Absolute means "set to", so a page already at 90 stays at 90.
    assert_eq!(doc.get_page_rotation(0).unwrap(), 90);
}

#[test]
fn rotate_relative_accumulates_and_wraps() {
    let mut doc = PdfDocument::open(get_multipage_pdf(2)).unwrap();
    doc.set_page_rotation(0, 90).unwrap();

    doc.apply(fepdf::Operation::Rotate {
        pages: fepdf::PageSelection::Single(0),
        mode: fepdf::RotateMode::Relative(fepdf::Quarter::Q90),
    })
    .unwrap();
    assert_eq!(doc.get_page_rotation(0).unwrap(), 180);

    // 180 + 270 wraps rather than reaching 450.
    doc.apply(fepdf::Operation::Rotate {
        pages: fepdf::PageSelection::Single(0),
        mode: fepdf::RotateMode::Relative(fepdf::Quarter::Q270),
    })
    .unwrap();
    assert_eq!(doc.get_page_rotation(0).unwrap(), 90);
}

#[test]
fn rotate_applies_to_every_selected_page() {
    let mut doc = PdfDocument::open(get_multipage_pdf(3)).unwrap();

    doc.apply(fepdf::Operation::Rotate {
        pages: fepdf::PageSelection::Indices(vec![0, 2]),
        mode: fepdf::RotateMode::Relative(fepdf::Quarter::Q90),
    })
    .unwrap();

    assert_eq!(doc.get_page_rotation(0).unwrap(), 90);
    assert_eq!(doc.get_page_rotation(1).unwrap(), 0);
    assert_eq!(doc.get_page_rotation(2).unwrap(), 90);
}

#[test]
fn quarter_rejects_angles_that_are_not_multiples_of_90() {
    // The type is what stops `--angle 45` reaching /Rotate, where ISO 32000-2
    // requires a multiple of 90.
    assert!(fepdf::Quarter::from_degrees(45).is_none());
    assert!(fepdf::Quarter::from_degrees(1).is_none());
    assert_eq!(fepdf::Quarter::from_degrees(450), Some(fepdf::Quarter::Q90));
    assert_eq!(fepdf::Quarter::from_degrees(-90), Some(fepdf::Quarter::Q270));
}

#[test]
fn implemented_operations_still_succeed() {
    let mut doc = PdfDocument::open(get_multipage_pdf(2)).unwrap();
    assert!(
        doc.apply(fepdf::Operation::Rotate {
            pages: fepdf::PageSelection::All,
            mode: fepdf::RotateMode::Relative(fepdf::Quarter::Q90),
        })
        .is_ok()
    );
    assert_eq!(doc.get_page_rotation(0).unwrap(), 90);

    assert!(
        doc.apply(fepdf::Operation::SetPageLabels(vec![fepdf::PageLabelSpec {
            start_page: 0,
            style: fepdf::PageLabelStyle::Decimal,
            prefix: None,
            start_number: 1,
        }]))
        .is_ok()
    );

    assert!(
        doc.apply(fepdf::Operation::AddAnnotation(fepdf::AnnotationSpec {
            page: 0,
            rect: [50.0, 50.0, 150.0, 150.0],
            kind: fepdf::AnnotationKind::TextComment { contents: "Test Comment".to_string() },
        }))
        .is_ok()
    );

    assert!(
        doc.apply(fepdf::Operation::ApplyBatesNumbering {
            pages: fepdf::PageSelection::All,
            prefix: "TEST-".to_string(),
            start_number: 100,
            digits: 6,
            position: fepdf::DecorationPosition::BottomRight,
        })
        .is_ok()
    );

    assert!(
        doc.apply(fepdf::Operation::SetPronunciationLexicon {
            lexicon_xml_bytes: b"<lexicon/>".to_vec(),
        })
        .is_ok()
    );

    assert!(
        doc.apply(fepdf::Operation::ExecuteAction(
            fepdf::PdfAction::Named("NextPage".to_string(),)
        ))
        .is_ok()
    );
}
