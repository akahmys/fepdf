//! Any bytes, taken the way a caller takes a document: opened, its first page drawn, its
//! text extracted and the result saved. A refusal is an answer; a panic, a hang or an
//! allocation the machine cannot meet is a finding (ROADMAP Z-1).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(doc) = fepdf::PdfDocument::open(data.to_vec().into()) else { return };
    let mut recorder = fepdf_fixtures::recorder::Recorder::new();
    let _ = doc.render_page(0, &mut recorder, kurbo::Affine::IDENTITY);
    let _ = doc.extract_text(0);
    let path = std::env::temp_dir().join(format!("fepdf_fuzz_{}.pdf", std::process::id()));
    let _ = doc.save_with_options(&path, "2.0", &fepdf::SaveOptions::default());
    let _ = std::fs::remove_file(&path);
});
