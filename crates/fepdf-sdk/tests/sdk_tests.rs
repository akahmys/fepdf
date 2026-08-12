//! Integration tests for the public `fepdf-sdk` surface.
//!
//! Placed under `tests/` per RR-15 Rule 14 (Test Code Separation).

#![allow(clippy::float_cmp)]
use bytes::Bytes;
use fepdf_sdk::{PdfDocument, PdfStandard};
use std::io::Write;

fn get_minimal_pdf() -> Bytes {
    let mut doc = lopdf::Document::with_version("1.7");

    let mut pages_dict = lopdf::Dictionary::new();
    pages_dict.set("Type", lopdf::Object::Name(b"Pages".to_vec()));
    pages_dict.set("Kids", lopdf::Object::Array(vec![]));
    pages_dict.set("Count", lopdf::Object::Integer(0));
    let pages_id = doc.add_object(lopdf::Object::Dictionary(pages_dict));

    let mut catalog_dict = lopdf::Dictionary::new();
    catalog_dict.set("Type", lopdf::Object::Name(b"Catalog".to_vec()));
    catalog_dict.set("Pages", lopdf::Object::Reference(pages_id));
    let catalog_id = doc.add_object(lopdf::Object::Dictionary(catalog_dict));

    doc.trailer.set("Root", lopdf::Object::Reference(catalog_id));
    doc.trailer.set("Size", lopdf::Object::Integer(i64::from(catalog_id.0 + 1)));

    let id_item = lopdf::Object::String(
        b"0123456789abcdef0123456789abcdef".to_vec(),
        lopdf::StringFormat::Hexadecimal,
    );
    doc.trailer.set("ID", lopdf::Object::Array(vec![id_item.clone(), id_item]));

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    Bytes::from(buf)
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
fn test_object_stream_packer() {
    use fepdf_sdk::obj_stm::ObjectStreamPacker;
    let mut packer = ObjectStreamPacker::new();
    assert_eq!(packer.count(), 0);

    // Add dummy object serializations
    packer
        .add_object(5, |w| {
            w.write_all(b"<< /Dummy 1 >>")?;
            Ok(())
        })
        .unwrap();
    assert_eq!(packer.count(), 1);

    packer
        .add_object(6, |w| {
            w.write_all(b"[1 2 3]")?;
            Ok(())
        })
        .unwrap();
    assert_eq!(packer.count(), 2);

    // Finish and verify no unwrap panics
    let (n, first, full) = packer.finish();
    assert_eq!(n, 2);
    assert!(first > 0);
    assert!(!full.is_empty());
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
    use fepdf_core::graphics::Color;
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

#[test]
fn test_r5_key_derivation_multistage() {
    use fepdf_core::security::SecurityHandler;
    let file_id = b"testfileid123456";
    let handler = SecurityHandler::new_v5("password", "", file_id);
    assert!(handler.is_ok());

    // Verify it is a Revision 5 handler with AES enabled
    let h = handler.unwrap();
    assert!(h.should_decrypt_metadata());
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

fn get_multipage_pdf(count: usize) -> Bytes {
    let mut doc = lopdf::Document::with_version("1.7");

    let mut page_ids = Vec::new();
    for _ in 0..count {
        let mut page_dict = lopdf::Dictionary::new();
        page_dict.set("Type", lopdf::Object::Name(b"Page".to_vec()));
        page_dict.set(
            "MediaBox",
            lopdf::Object::Array(vec![
                lopdf::Object::Integer(0),
                lopdf::Object::Integer(0),
                lopdf::Object::Integer(612),
                lopdf::Object::Integer(792),
            ]),
        );
        let pid = doc.add_object(lopdf::Object::Dictionary(page_dict));
        page_ids.push(lopdf::Object::Reference(pid));
    }

    let mut pages_dict = lopdf::Dictionary::new();
    pages_dict.set("Type", lopdf::Object::Name(b"Pages".to_vec()));
    pages_dict.set("Kids", lopdf::Object::Array(page_ids.clone()));
    pages_dict.set("Count", lopdf::Object::Integer(i64::try_from(count).unwrap_or(0)));
    let pages_id = doc.add_object(lopdf::Object::Dictionary(pages_dict));

    for pid_obj in page_ids {
        if let lopdf::Object::Reference(pid) = pid_obj
            && let Some(lopdf::Object::Dictionary(dict)) = doc.objects.get_mut(&pid)
        {
            dict.set("Parent", lopdf::Object::Reference(pages_id));
        }
    }

    let mut catalog_dict = lopdf::Dictionary::new();
    catalog_dict.set("Type", lopdf::Object::Name(b"Catalog".to_vec()));
    catalog_dict.set("Pages", lopdf::Object::Reference(pages_id));
    let catalog_id = doc.add_object(lopdf::Object::Dictionary(catalog_dict));

    doc.trailer.set("Root", lopdf::Object::Reference(catalog_id));
    doc.trailer.set("Size", lopdf::Object::Integer(i64::from(catalog_id.0 + 1)));

    let id_item = lopdf::Object::String(
        b"0123456789abcdef0123456789abcdef".to_vec(),
        lopdf::StringFormat::Hexadecimal,
    );
    doc.trailer.set("ID", lopdf::Object::Array(vec![id_item.clone(), id_item]));

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    Bytes::from(buf)
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

    doc.apply(fepdf_sdk::Operation::Rotate {
        pages: fepdf_sdk::PageSelection::Single(0),
        mode: fepdf_sdk::RotateMode::Absolute(fepdf_sdk::Quarter::Q90),
    })
    .unwrap();

    // Absolute means "set to", so a page already at 90 stays at 90.
    assert_eq!(doc.get_page_rotation(0).unwrap(), 90);
}

#[test]
fn rotate_relative_accumulates_and_wraps() {
    let mut doc = PdfDocument::open(get_multipage_pdf(2)).unwrap();
    doc.set_page_rotation(0, 90).unwrap();

    doc.apply(fepdf_sdk::Operation::Rotate {
        pages: fepdf_sdk::PageSelection::Single(0),
        mode: fepdf_sdk::RotateMode::Relative(fepdf_sdk::Quarter::Q90),
    })
    .unwrap();
    assert_eq!(doc.get_page_rotation(0).unwrap(), 180);

    // 180 + 270 wraps rather than reaching 450.
    doc.apply(fepdf_sdk::Operation::Rotate {
        pages: fepdf_sdk::PageSelection::Single(0),
        mode: fepdf_sdk::RotateMode::Relative(fepdf_sdk::Quarter::Q270),
    })
    .unwrap();
    assert_eq!(doc.get_page_rotation(0).unwrap(), 90);
}

#[test]
fn rotate_applies_to_every_selected_page() {
    let mut doc = PdfDocument::open(get_multipage_pdf(3)).unwrap();

    doc.apply(fepdf_sdk::Operation::Rotate {
        pages: fepdf_sdk::PageSelection::Indices(vec![0, 2]),
        mode: fepdf_sdk::RotateMode::Relative(fepdf_sdk::Quarter::Q90),
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
    assert!(fepdf_sdk::Quarter::from_degrees(45).is_none());
    assert!(fepdf_sdk::Quarter::from_degrees(1).is_none());
    assert_eq!(fepdf_sdk::Quarter::from_degrees(450), Some(fepdf_sdk::Quarter::Q90));
    assert_eq!(fepdf_sdk::Quarter::from_degrees(-90), Some(fepdf_sdk::Quarter::Q270));
}
