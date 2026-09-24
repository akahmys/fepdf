//! How many font dictionaries over the samples answer with a program (ROADMAP W-E3b).
//!
//! The figure `font_program_test.rs` quotes — 291 of 299 — was taken once and computed
//! nowhere; this re-derives it.
//!
//! ```text
//! cargo run --release -p fepdf-model --example font_program_census
//! ```
use fepdf_model::{Handle, Object, document::Document, ingest::IngestionOptions};

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("samples/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pdf"))
        .collect();
    files.sort();
    let (mut type3, mut other, mut answering) = (0usize, 0usize, 0usize);
    for f in &files {
        let Ok(bytes) = std::fs::read(f) else { continue };
        let Ok(doc) = Document::open(bytes.into(), &IngestionOptions::default()) else { continue };
        let arena = doc.arena();
        for i in 0..arena.object_count() {
            let h = Handle::new(i);
            let Some(Object::Dictionary(dh)) = arena.get_object(h) else { continue };
            let Some(dict) = arena.get_dict(dh) else { continue };
            let name_of = |key: &str| {
                dict.iter()
                    .find(|(k, _)| arena.get_name(**k).is_some_and(|n| n.as_str() == key))
                    .and_then(|(_, v)| v.resolve(arena).as_name())
                    .and_then(|n| arena.get_name(n))
                    .map(|n| n.as_str().to_string())
            };
            if name_of("Type").as_deref() != Some("Font") {
                continue;
            }
            if name_of("Subtype").as_deref() == Some("Type3") {
                type3 += 1;
                continue;
            }
            other += 1;
            if doc.get_font(h).ok().is_some_and(|font| font.program().is_some_and(|p| p.len() > 4))
            {
                answering += 1;
            }
        }
    }
    println!(
        "{} samples: {type3} Type 3, {other} other, {answering} of those answering with a program",
        files.len()
    );
}
