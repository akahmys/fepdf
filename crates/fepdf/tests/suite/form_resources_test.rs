//! A form XObject with no `/Resources`, given the ones it names from what draws it (8.10.2,
//! ROADMAP AC-1, ADR-0127).

use fepdf::PdfDocument;
use fepdf_model::Object;
use fepdf_model::access::entry;

/// Two pages and five forms without `/Resources`. Page 1 lends `/F1` (Helvetica, object 4)
/// and `/G1`, and draws A, C, D and E; A draws B. Page 2 lends `/F1` as Times (object 11)
/// and draws D.
///
/// - A names `/F1` and `/B`; B, drawn only by A, names `/G1`.
/// - C names nothing.
/// - D names `/F1`, which its two pages lend for different fonts.
/// - E names `/F9`, which nothing that draws it has.
fn forms() -> PdfDocument {
    let form = |content: &str| {
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        )
    };
    let stream =
        |content: &str| format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len());
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 9 0 R] /Count 2 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 10 0 R /Resources \
           << /Font << /F1 4 0 R >> /ExtGState << /G1 << /CA 0.5 >> >> \
           /XObject << /A 5 0 R /B 6 0 R /C 7 0 R /D 8 0 R /E 12 0 R >> >> >>"
            .to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        form("BT /F1 12 Tf (a) Tj ET /B Do"),
        form("/G1 gs 0 0 1 1 re f"),
        form("0 0 1 1 re f"),
        form("BT /F1 9 Tf (d) Tj ET"),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 13 0 R /Resources \
           << /Font << /F1 11 0 R >> /XObject << /D 8 0 R >> >> >>"
            .to_string(),
        stream("/A Do /C Do /D Do /E Do"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".to_string(),
        form("BT /F9 9 Tf (e) Tj ET"),
        stream("/D Do"),
    ]);
    PdfDocument::open(bytes.into()).expect("the forms open")
}

/// The `/Resources` of object `form`, if it has one.
fn resources(doc: &PdfDocument, form: u32) -> Option<Object> {
    let object = Object::Reference(fepdf_model::Handle::new(form));
    entry(doc.inner().arena(), &object, "Resources")
}

/// What `form`'s resources hold under `category` for `name`, as the object it refers to.
fn lent(doc: &PdfDocument, form: u32, category: &str, name: &str) -> Option<Object> {
    let arena = doc.inner().arena();
    let held = entry(arena, &resources(doc, form)?, category)?;
    arena.dict_entry(held.as_dict_handle()?, arena.name(name))
}

/// **A form is given what it names, from what draws it**: A the page's `/F1` and `/B`, and
/// B — drawn by A, which draws with the page's — the page's `/G1`.
#[test]
fn a_form_is_given_what_it_names_from_what_draws_it() {
    let doc = forms();
    let reference = |n: u32| Some(Object::Reference(fepdf_model::Handle::new(n)));
    assert_eq!(lent(&doc, 5, "Font", "F1"), reference(4));
    assert_eq!(lent(&doc, 5, "XObject", "B"), reference(6));
    assert!(lent(&doc, 6, "ExtGState", "G1").is_some(), "{:?}", resources(&doc, 6));
}

/// **A form that names nothing is given an empty `/Resources`**, and what is given is not
/// recorded: it loses nothing, and the file it came from conformed.
#[test]
fn a_form_that_names_nothing_is_given_an_empty_one() {
    let doc = forms();
    assert!(
        doc.decisions()
            .iter()
            .all(|d| d.clause != "8.10.2"
                || d.found.starts_with("1 form XObjects hold no /Resources")),
        "{:?}",
        doc.decisions()
    );
    let arena = doc.inner().arena();
    let held = resources(&doc, 7).and_then(|r| r.as_dict_handle()).and_then(|r| arena.get_dict(r));
    assert_eq!(held.map(|d| d.len()), Some(0));
}

/// **Where it cannot be told what a name meant, the form is left as it was, and that is
/// said**: D's two pages lend `/F1` for different fonts, and nothing that draws E has
/// `/F9`.
#[test]
fn an_ambiguous_or_missing_name_leaves_the_form_as_it_was() {
    let doc = forms();
    assert!(resources(&doc, 8).is_none(), "D was given a guess");
    assert!(resources(&doc, 12).is_none(), "E was given part of what it names");
    let said: Vec<String> =
        doc.decisions().into_iter().filter(|d| d.clause == "8.10.2").map(|d| d.found).collect();
    assert_eq!(said.len(), 2, "{said:?}");
    for count in
        ["1 form XObjects hold no /Resources, and", "1 form XObjects hold no /Resources and name a"]
    {
        assert!(said.iter().any(|s| s.starts_with(count)), "{count}: {said:?}");
    }
}

/// **A save writes what the form was given**, read back through the byte layer.
#[test]
fn a_save_writes_what_the_form_was_given() {
    let doc = forms();
    let path =
        std::env::temp_dir().join(format!("fepdf-form-resources-{}.pdf", std::process::id()));
    let options = fepdf::SaveOptions { obj_stm: false, ..fepdf::SaveOptions::default() };
    let _ = doc.save_with_options(&path, "2.0", &options).expect("it saves");
    let bytes = std::fs::read(&path).expect("it was written");
    let _ = std::fs::remove_file(&path);
    let raw = fepdf_model::reader::load_document(&bytes::Bytes::from(bytes)).expect("it reads");
    let trailer = raw.trailer.map(Object::Dictionary).expect("a trailer");
    let root = entry(&raw.arena, &trailer, "Root").expect("a catalogue");
    let pages = entry(&raw.arena, &root, "Pages").expect("pages");
    let kids = fepdf_model::access::items(&raw.arena, &pages, "Kids");
    let xobjects = entry(&raw.arena, &kids[0], "Resources")
        .and_then(|r| entry(&raw.arena, &r, "XObject"))
        .expect("the page's forms");
    let a = entry(&raw.arena, &xobjects, "A").expect("form A");
    let font = entry(&raw.arena, &a, "Resources").and_then(|r| entry(&raw.arena, &r, "Font"));
    assert!(font.and_then(|f| entry(&raw.arena, &f, "F1")).is_some(), "A's /F1 was not written");
}
