//! Schema integration tests for fepdf-model.

use fepdf_model::Object;
use fepdf_model::PdfArena;
use fepdf_model::font::schema::PdfFont;
use fepdf_model::graphics::schema::PdfExtGState;
use fepdf_model::object::{FromPdfObject, PdfSchema};

#[test]
fn test_font_schema_expansion() {
    let arena = PdfArena::new();
    let mut dict = std::collections::BTreeMap::new();

    dict.insert(arena.name("Type"), Object::Name(arena.name("Font")));
    dict.insert(arena.name("Subtype"), Object::Name(arena.name("Type0")));
    dict.insert(arena.name("BaseFont"), Object::Name(arena.name("Arial-BoldMT")));

    let handle = arena.alloc_dict(dict);
    let obj = Object::Dictionary(handle);

    let font = PdfFont::from_pdf_object(obj, &arena).unwrap();
    assert_eq!(font.base_font.as_str(), "Arial-BoldMT");
    assert_eq!(PdfFont::iso_clause(), "9.2");
}

#[test]
fn test_graphics_schema_expansion() {
    let arena = PdfArena::new();
    let mut dict = std::collections::BTreeMap::new();

    dict.insert(arena.name("Type"), Object::Name(arena.name("ExtGState")));
    dict.insert(arena.name("CA"), Object::Real(0.5));
    dict.insert(arena.name("ca"), Object::Real(0.5));
    dict.insert(arena.name("BM"), Object::Name(arena.name("Multiply")));

    let handle = arena.alloc_dict(dict);
    let obj = Object::Dictionary(handle);

    let gs = PdfExtGState::from_pdf_object(obj, &arena).unwrap();
    assert_eq!(gs.blend_mode, Some(fepdf_model::graphics::BlendMode::Multiply));
    assert_eq!(PdfExtGState::iso_clause(), "8.4.5");
}

/// `/PageMode` and `/PageLayout` are names from a fixed list, and the list has grown
/// twice — `UseOC` in 1.5, `UseAttachments` in 1.6. A file may therefore carry a value
/// newer than this code, so an unrecognised one is kept rather than folded to a default.
#[test]
fn the_display_entries_read_their_names_and_keep_the_ones_they_do_not_know() {
    use fepdf_model::document::{PageLayout, PageMode, PdfCatalog};

    let catalogue = |mode: &str, layout: &str| {
        let arena = PdfArena::new();
        let pages = arena.alloc_object(Object::Null);
        let mut dict = std::collections::BTreeMap::new();
        dict.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
        dict.insert(arena.name("Pages"), Object::Reference(pages));
        dict.insert(arena.name("PageMode"), Object::Name(arena.name(mode)));
        dict.insert(arena.name("PageLayout"), Object::Name(arena.name(layout)));
        dict.insert(arena.name("Lang"), Object::Text("cy-GB".to_string()));
        let handle = arena.alloc_dict(dict);
        PdfCatalog::from_pdf_object(Object::Dictionary(handle), &arena).expect("a catalogue")
    };

    let known = catalogue("UseAttachments", "TwoPageRight");
    assert_eq!(known.page_mode, Some(PageMode::UseAttachments));
    assert_eq!(known.page_layout, Some(PageLayout::TwoPageRight));
    assert_eq!(known.lang.as_deref(), Some("cy-GB"), "/Lang was written and not read back");

    // Not `UseNone`, and not an error either: the name survives so a caller can see it.
    let future = catalogue("UseSomethingFromPdf3", "SixColumnsRight");
    assert_eq!(future.page_mode, Some(PageMode::Other("UseSomethingFromPdf3".to_string())));
    assert_eq!(future.page_layout, Some(PageLayout::Other("SixColumnsRight".to_string())));
}

/// Absent is absent. A document that says nothing about how to display itself must not
/// come back claiming a default the file never stated.
#[test]
fn a_catalogue_that_states_no_display_preference_reports_none() {
    use fepdf_model::document::PdfCatalog;

    let arena = PdfArena::new();
    let pages = arena.alloc_object(Object::Null);
    let mut dict = std::collections::BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    dict.insert(arena.name("Pages"), Object::Reference(pages));
    let handle = arena.alloc_dict(dict);

    let catalogue =
        PdfCatalog::from_pdf_object(Object::Dictionary(handle), &arena).expect("a catalogue");
    assert!(catalogue.page_mode.is_none());
    assert!(catalogue.page_layout.is_none());
    assert!(catalogue.lang.is_none());
}
