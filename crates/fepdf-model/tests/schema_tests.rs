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

/// Every Table 147 entry the standard defines, read from one dictionary.
///
/// Written as one dictionary rather than eighteen tests because the failure this guards
/// against is a mistyped `#[pdf_key]` — a key that silently reads as `None` — and that
/// only shows up when every key is set and one comes back empty.
#[test]
fn viewer_preferences_reads_every_entry_of_table_147() {
    use fepdf_model::document::{
        Direction, Duplex, PageBoundary, PageMode, PdfCatalog, PrintScaling,
    };

    let arena = PdfArena::new();
    let pages = arena.alloc_object(Object::Null);
    let mut prefs = std::collections::BTreeMap::new();
    prefs.insert(arena.name("Type"), Object::Name(arena.name("ViewerPreferences")));
    for key in ["HideToolbar", "HideMenubar", "HideWindowUI", "FitWindow", "CenterWindow"] {
        prefs.insert(arena.name(key), Object::Boolean(true));
    }
    prefs.insert(arena.name("DisplayDocTitle"), Object::Boolean(false));
    prefs.insert(arena.name("NonFullScreenPageMode"), Object::Name(arena.name("UseOutlines")));
    prefs.insert(arena.name("Direction"), Object::Name(arena.name("R2L")));
    prefs.insert(arena.name("ViewArea"), Object::Name(arena.name("CropBox")));
    prefs.insert(arena.name("ViewClip"), Object::Name(arena.name("BleedBox")));
    prefs.insert(arena.name("PrintArea"), Object::Name(arena.name("TrimBox")));
    prefs.insert(arena.name("PrintClip"), Object::Name(arena.name("ArtBox")));
    prefs.insert(arena.name("PrintScaling"), Object::Name(arena.name("None")));
    prefs.insert(arena.name("Duplex"), Object::Name(arena.name("DuplexFlipLongEdge")));
    prefs.insert(arena.name("PickTrayByPDFSize"), Object::Boolean(true));
    prefs.insert(arena.name("NumCopies"), Object::Integer(3));
    prefs.insert(arena.name("PrintPageRange"), Object::Array(arena.alloc_array(vec![])));
    prefs.insert(arena.name("Enforce"), Object::Array(arena.alloc_array(vec![])));
    let prefs_handle = arena.alloc_dict(prefs);

    let mut dict = std::collections::BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    dict.insert(arena.name("Pages"), Object::Reference(pages));
    dict.insert(arena.name("ViewerPreferences"), Object::Dictionary(prefs_handle));
    let handle = arena.alloc_dict(dict);

    let catalogue =
        PdfCatalog::from_pdf_object(Object::Dictionary(handle), &arena).expect("a catalogue");
    let p = catalogue.viewer_preferences.expect("/ViewerPreferences was set");

    assert_eq!(p.hide_toolbar, Some(true));
    assert_eq!(p.hide_menubar, Some(true));
    assert_eq!(p.hide_window_ui, Some(true));
    assert_eq!(p.fit_window, Some(true));
    assert_eq!(p.center_window, Some(true));
    // Not `None`: an entry that says `false` said something, and losing the difference
    // is the whole reason these fields are `Option<bool>` and not `bool`.
    assert_eq!(p.display_doc_title, Some(false));
    assert_eq!(p.non_full_screen_page_mode, Some(PageMode::UseOutlines));
    assert_eq!(p.direction, Some(Direction::R2L));
    assert_eq!(p.view_area, Some(PageBoundary::CropBox));
    assert_eq!(p.view_clip, Some(PageBoundary::BleedBox));
    assert_eq!(p.print_area, Some(PageBoundary::TrimBox));
    assert_eq!(p.print_clip, Some(PageBoundary::ArtBox));
    assert_eq!(p.print_scaling, Some(PrintScaling::None));
    assert_eq!(p.duplex, Some(Duplex::DuplexFlipLongEdge));
    assert_eq!(p.pick_tray_by_pdf_size, Some(true));
    assert_eq!(p.num_copies, Some(3));
    assert!(p.print_page_range.is_some(), "/PrintPageRange was set");
    assert!(p.enforce.is_some(), "/Enforce was set");
}

/// An empty `/ViewerPreferences` is `Some` with nothing in it, not `None` and not
/// eighteen defaults.
///
/// `samples/fy05.pdf` carries exactly this — `inspect catalog` reports it as
/// `dictionary[0]`. Under Table 147's defaults it would read identically to a producer
/// that had deliberately written five `false`s, and a report cannot then say what the
/// file declares. The distinction is the reason every field is an `Option`.
#[test]
fn an_empty_viewer_preferences_states_nothing_rather_than_defaults() {
    use fepdf_model::document::PdfCatalog;

    let arena = PdfArena::new();
    let pages = arena.alloc_object(Object::Null);
    let empty = arena.alloc_dict(std::collections::BTreeMap::new());
    let mut dict = std::collections::BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    dict.insert(arena.name("Pages"), Object::Reference(pages));
    dict.insert(arena.name("ViewerPreferences"), Object::Dictionary(empty));
    let handle = arena.alloc_dict(dict);

    let catalogue =
        PdfCatalog::from_pdf_object(Object::Dictionary(handle), &arena).expect("a catalogue");
    let p = catalogue.viewer_preferences.expect("an empty dictionary is still a dictionary");
    assert_eq!(p.hide_toolbar, None, "Table 147 defaults it to false; the file did not");
    assert_eq!(p.display_doc_title, None);
    assert_eq!(p.direction, None, "Table 147 defaults it to L2R; the file did not");
    assert_eq!(p.print_scaling, None);

    // And absent stays absent, so the two cases above are actually distinguishable.
    let mut bare = std::collections::BTreeMap::new();
    bare.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    bare.insert(arena.name("Pages"), Object::Reference(pages));
    let bare = arena.alloc_dict(bare);
    let catalogue =
        PdfCatalog::from_pdf_object(Object::Dictionary(bare), &arena).expect("a catalogue");
    assert!(catalogue.viewer_preferences.is_none());
}

/// A name Table 147 does not define is kept, not folded to the default.
///
/// The corpus has one `/Direction`, `bokutokitan.pdf`'s `/R2L`. Vertical Japanese is
/// where a producer reaches for something else — `/T2B` and `/V` both circulate without
/// being in the standard — and a viewer that read those as `L2R` would lay the book out
/// backwards while reporting no problem.
#[test]
fn viewer_preferences_keeps_names_it_does_not_recognise() {
    use fepdf_model::document::{Direction, Duplex, PageBoundary, PdfCatalog, PrintScaling};

    let arena = PdfArena::new();
    let pages = arena.alloc_object(Object::Null);
    let mut prefs = std::collections::BTreeMap::new();
    prefs.insert(arena.name("Direction"), Object::Name(arena.name("T2B")));
    prefs.insert(arena.name("ViewArea"), Object::Name(arena.name("SpineBox")));
    prefs.insert(arena.name("PrintScaling"), Object::Name(arena.name("FitToPaper")));
    prefs.insert(arena.name("Duplex"), Object::Name(arena.name("Tumble")));
    let prefs_handle = arena.alloc_dict(prefs);

    let mut dict = std::collections::BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Catalog")));
    dict.insert(arena.name("Pages"), Object::Reference(pages));
    dict.insert(arena.name("ViewerPreferences"), Object::Dictionary(prefs_handle));
    let handle = arena.alloc_dict(dict);

    let p = PdfCatalog::from_pdf_object(Object::Dictionary(handle), &arena)
        .expect("a catalogue")
        .viewer_preferences
        .expect("/ViewerPreferences was set");

    assert_eq!(p.direction, Some(Direction::Other("T2B".to_string())));
    assert_eq!(p.view_area, Some(PageBoundary::Other("SpineBox".to_string())));
    assert_eq!(p.print_scaling, Some(PrintScaling::Other("FitToPaper".to_string())));
    assert_eq!(p.duplex, Some(Duplex::Other("Tumble".to_string())));

    // And what a report prints is the name the file carried, not a Rust identifier.
    assert_eq!(p.direction.expect("set").as_name(), "T2B");
    assert_eq!(Direction::R2L.as_name(), "R2L");
}
