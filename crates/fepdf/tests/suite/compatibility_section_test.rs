//! A compatibility section (`BX` … `EX`, ISO 32000-2 7.8.2, Table 33) survives a save.
//!
//! The saved file is read through the byte layer (`reader::load_document`), which
//! normalises nothing, so what is asserted is what the writer wrote.

use bytes::Bytes;
use fepdf::{PdfDocument, SaveOptions};
use fepdf_model::Object;
use fepdf_model::access::{entry, items};

/// A page whose content stream puts an unknown operator outside a compatibility section
/// and a vendor operator inside one, between two filled rectangles.
fn page_with_a_section() -> PdfDocument {
    let content = "0 0 10 10 re f\n1 2 zzunknown\nBX 3 4 yyvendor EX\n20 20 5 5 re f\n";
    let bytes = fepdf_fixtures::Pdf::new().version("1.7").assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ]);
    PdfDocument::open(bytes.into()).expect("the page opens")
}

/// **`BX` and `EX` are written around the vendor operator.** The parser mapped both to no
/// command, so a save wrote `3 4 yyvendor` outside any section, where 7.8.2 makes an
/// unrecognised operator an error.
#[test]
fn the_section_is_written_as_the_file_had_it() {
    let options = SaveOptions { compress: false, obj_stm: false, ..SaveOptions::default() };
    let path = std::env::temp_dir()
        .join(format!("fepdf-compatibility-section-{}.pdf", std::process::id()));
    page_with_a_section().save_with_options(&path, "2.0", &options).expect("the page saves");
    let bytes = std::fs::read(&path).expect("the save was written");
    let _ = std::fs::remove_file(&path);

    let raw = fepdf_model::reader::load_document(&Bytes::from(bytes)).expect("the save reads");
    let arena = &raw.arena;
    let trailer = raw.trailer.map(Object::Dictionary).expect("a trailer");
    let root = entry(arena, &trailer, "Root").expect("a catalogue");
    let pages = entry(arena, &root, "Pages").expect("a page tree");
    let page = items(arena, &pages, "Kids").into_iter().next().expect("the page");
    let Some(Object::Stream(_, data)) = entry(arena, &page, "Contents") else {
        panic!("the page has one content stream")
    };
    let content = arena.get_stream_bytes(&data).expect("the stream reads");
    let tokens: Vec<&[u8]> =
        content.split(u8::is_ascii_whitespace).filter(|t| !t.is_empty()).collect();
    let shown = String::from_utf8_lossy(&content);
    let section: &[&[u8]] = &[b"BX", b"3", b"4", b"yyvendor", b"EX"];
    assert!(tokens.windows(section.len()).any(|w| w == section), "{shown}");
    assert!(tokens.contains(&b"zzunknown".as_slice()), "{shown}");
}

/// **`BX` and `EX` are operators the interpreter runs**, so drawing the page says nothing
/// about them while it still says `zzunknown` is not one it runs.
#[test]
fn the_section_bounds_are_not_reported_as_unknown() {
    let doc = page_with_a_section();
    doc.extract_text(0).expect("the page reads");
    let unrun: Vec<String> = doc
        .decisions()
        .into_iter()
        .filter(|d| d.found.contains("is not one this engine runs"))
        .map(|d| d.found)
        .collect();
    assert!(unrun.iter().any(|f| f.contains("\"zzunknown\"")), "{unrun:#?}");
    assert!(!unrun.iter().any(|f| f.contains("\"BX\"") || f.contains("\"EX\"")), "{unrun:#?}");
}
