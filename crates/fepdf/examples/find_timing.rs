//! How long reading every page's runs takes, per sample: the cost of one search of a whole
//! document, which is what decides whether the studio can search as a reader types
//! (ROADMAP W-15).
use fepdf::{IngestionOptions, PdfDocument};
use std::time::Instant;

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("samples/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pdf"))
        .filter(|p| std::env::args().nth(1).is_none_or(|only| p.ends_with(only)))
        .collect();
    files.sort();
    println!("{:<24} {:>6} {:>10} {:>10}", "file", "pages", "first ms", "again ms");
    for f in files {
        let Ok(bytes) = std::fs::read(&f) else { continue };
        let Ok(doc) = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        else {
            continue;
        };
        let pages = doc.page_count().unwrap_or(0);
        let mut times = Vec::new();
        for _ in 0..2 {
            let started = Instant::now();
            let mut stretches = 0usize;
            for p in 0..pages {
                let runs = fepdf::text::runs_of_page(doc.inner(), p).unwrap_or_default();
                stretches += fepdf::text::stretches_of(&runs).len();
            }
            std::hint::black_box(stretches);
            times.push(started.elapsed().as_millis());
        }
        let name = f.file_name().unwrap_or_default().to_string_lossy().to_string();
        println!("{name:<24} {pages:>6} {:>10} {:>10}", times[0], times[1]);
    }
}
