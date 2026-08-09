use fepdf_core::Object;
use fepdf_core::PdfArena;
use fepdf_core::font::schema::PdfFont;
use fepdf_core::graphics::schema::PdfExtGState;
use fepdf_core::object::{FromPdfObject, PdfSchema};

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
    assert_eq!(gs.blend_mode, Some(fepdf_core::graphics::BlendMode::Multiply));
    assert_eq!(PdfExtGState::iso_clause(), "8.4.5");
}
