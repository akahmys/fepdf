//! A linearised save is the save with linearising on, and every option means what it means
//! there (Y-F1).

use fepdf::{PdfDocument, SaveOptions};

fn one_page() -> PdfDocument {
    let bytes = fepdf_fixtures::assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ]);
    PdfDocument::open(bytes.into()).expect("the fixture opens")
}

/// `doc` linearised with `options`, read back, or the error the save gave.
fn linearised(doc: &PdfDocument, options: &SaveOptions) -> fepdf::PdfResult<PdfDocument> {
    let path = std::env::temp_dir().join(format!(
        "fepdf-linearize-options-{}-{:?}.pdf",
        std::process::id(),
        std::thread::current().id()
    ));
    let saved = doc.save_linearized(&path, "2.0", options);
    let read = std::fs::read(&path);
    let _ = std::fs::remove_file(&path);
    saved?;
    Ok(PdfDocument::open(read.expect("written").into()).expect("it reads back"))
}

/// **The options a save reads, a linearised save reads.** It read `title` and `author`
/// alone, so the language and the rights a caller gave were dropped.
#[test]
fn a_linearised_save_writes_the_language_and_rights_it_was_given() {
    let options = SaveOptions {
        lang: Some("ja".into()),
        copyright: Some("(c) 2026".into()),
        stamped_at: Some(0),
        ..SaveOptions::default()
    };
    let back = linearised(&one_page(), &options).expect("it linearises");
    let catalog = fepdf_model::Object::Reference(*back.inner().root_handle());
    let lang = match fepdf_model::access::entry(back.inner().arena(), &catalog, "Lang") {
        Some(fepdf_model::Object::Text(t)) => Some(t),
        Some(fepdf_model::Object::String(b) | fepdf_model::Object::Hex(b)) => {
            Some(fepdf_model::refine::text::recover_string(&b))
        }
        _ => None,
    };
    assert_eq!(lang.as_deref(), Some("ja"), "the language was not written");
    assert_eq!(back.metadata().rights.as_deref(), Some("(c) 2026"));
}

/// **A password is refused, not dropped.** The linearised layout writes no `/Encrypt`, so
/// a caller asking for one got a plaintext file and no word of it.
#[test]
fn a_password_is_refused_rather_than_ignored() {
    let options = SaveOptions { password: Some("secret".into()), ..SaveOptions::default() };
    assert!(
        matches!(linearised(&one_page(), &options), Err(fepdf::PdfError::Refused { .. })),
        "a linearised save asked to encrypt wrote a file"
    );
}
