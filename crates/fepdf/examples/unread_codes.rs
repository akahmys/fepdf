//! How many codes each sample's runs draw and cannot name, over the first five pages.
use fepdf::{IngestionOptions, PdfDocument};

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("samples/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pdf"))
        .collect();
    files.sort();
    println!("{:<24} {:>8} {:>8}", "file", "codes", "unread");
    for f in files {
        let Ok(bytes) = std::fs::read(&f) else { continue };
        let Ok(doc) = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        else {
            continue;
        };
        let (mut codes, mut unread) = (0usize, 0usize);
        for page in 0..doc.page_count().unwrap_or(0).min(5) {
            for run in fepdf::text::runs_of_page(doc.inner(), page).unwrap_or_default() {
                codes += run.pieces.len();
                unread += run.pieces.iter().filter(|p| p.is_empty()).count();
            }
        }
        let name = f.file_name().unwrap_or_default().to_string_lossy().to_string();
        println!("{name:<24} {codes:>8} {unread:>8}");
    }
}
