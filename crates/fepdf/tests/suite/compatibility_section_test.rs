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
    page("0 0 10 10 re f\n1 2 zzunknown\nBX 3 4 yyvendor EX\n20 20 5 5 re f\n", None)
}

/// A one-page PDF 1.7 drawing `content`, with `form` as the content of a form XObject
/// `/Fm1` when there is one.
fn page(content: &str, form: Option<&str>) -> PdfDocument {
    let stream = |data: &str, dict: &str| {
        format!("<< {dict} /Length {} >>\nstream\n{data}\nendstream", data.len() + 1)
    };
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R \
           /Resources << /XObject << /Fm1 5 0 R >> >> >>"
            .to_string(),
        stream(content, ""),
    ];
    if let Some(form) = form {
        objects.push(stream(form, "/Type /XObject /Subtype /Form /BBox [0 0 100 100]"));
    }
    let bytes = fepdf_fixtures::Pdf::new().version("1.7").assemble(&objects);
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

/// The operators drawing `doc`'s page reported as not run, by the clause each cites.
fn unrun(doc: &PdfDocument) -> Vec<(String, String)> {
    doc.extract_text(0).expect("the page reads");
    doc.decisions()
        .into_iter()
        .filter(|d| d.found.contains("is not one this engine runs"))
        .map(|d| (d.clause.to_string(), d.found))
        .collect()
}

/// Whether `unrun` names `op`.
fn names(unrun: &[(String, String)], op: &str) -> bool {
    unrun.iter().any(|(_, found)| found.contains(&format!("{op:?}")))
}

/// **An unknown operator is an error outside a compatibility section and nothing inside
/// one** (7.8.2): `zzunknown` is recorded under 7.8.2, `yyvendor` is not recorded, and
/// neither are `BX` and `EX`, which the interpreter runs.
#[test]
fn only_an_unknown_operator_outside_a_section_is_a_violation() {
    let unrun = unrun(&page_with_a_section());
    assert!(
        unrun.iter().any(|(clause, found)| clause == "7.8.2" && found.contains("\"zzunknown\"")),
        "{unrun:#?}"
    );
    for op in ["yyvendor", "BX", "EX"] {
        assert!(!names(&unrun, op), "{op}: {unrun:#?}");
    }
}

/// **Sections nest** (7.8.2), so the first `EX` of two does not end the outer one.
#[test]
fn an_inner_ex_leaves_the_outer_section_open() {
    let unrun = unrun(&page("BX BX 1 aavendor EX 2 bbvendor EX 3 ccafter", None));
    assert!(!names(&unrun, "aavendor") && !names(&unrun, "bbvendor"), "{unrun:#?}");
    assert!(names(&unrun, "ccafter"), "{unrun:#?}");
}

/// **A section is a bracket within one content stream**: a form that leaves one open does
/// not cover what the page draws after the `Do`.
#[test]
fn a_section_a_form_leaves_open_ends_with_the_form() {
    let unrun = unrun(&page("/Fm1 Do 1 zzafter", Some("0 zzform BX 2 yyinside")));
    assert!(names(&unrun, "zzform"), "the form ran: {unrun:#?}");
    assert!(!names(&unrun, "yyinside"), "{unrun:#?}");
    assert!(names(&unrun, "zzafter"), "{unrun:#?}");
}
