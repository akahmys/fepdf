//! How far the next run starts from where the last one ended, per sample.
//!
//! ROADMAP W-E3d needs a rule for "these two runs are one line of text". **Measured as a
//! distance and not as an x gap**, because `advance` is already a vector in the
//! direction the text is set: a first attempt compared the y coordinates and called
//! `fugaku.pdf` 0% contiguous, which is what asking a horizontal question of vertical
//! Japanese looks like. Reported against the em, because a tolerance in points means
//! different things at different sizes.
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
    println!(
        "{:<24} {:>7} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "file", "pairs", "<.01em", "<.05em", "<.10em", "<.20em", "<.33em"
    );
    for f in files {
        let Ok(bytes) = std::fs::read(&f) else { continue };
        let Ok(doc) = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        else {
            continue;
        };
        let mut rel: Vec<f64> = Vec::new();
        for p in 0..doc.page_count().unwrap_or(0).min(10) {
            let runs = fepdf::text::runs_of_page(doc.inner(), p).unwrap_or_default();
            for w in runs.windows(2) {
                let (a, b) = (&w[0], &w[1]);
                let em = a.rise.0.hypot(a.rise.1);
                if em <= 0.0 {
                    continue;
                }
                let end = (a.origin.0 + a.advance.0, a.origin.1 + a.advance.1);
                rel.push((b.origin.0 - end.0).hypot(b.origin.1 - end.1) / em);
            }
        }
        if rel.is_empty() {
            continue;
        }
        rel.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
        let under =
            |t: f64| count(rel.iter().filter(|g| **g < t).count()) * 100.0 / count(rel.len());
        let name = f.file_name().unwrap_or_default().to_string_lossy().to_string();
        println!(
            "{name:<24} {:>7} {:>7.0}% {:>7.0}% {:>7.0}% {:>7.0}% {:>7.0}%",
            rel.len(),
            under(0.01),
            under(0.05),
            under(0.10),
            under(0.20),
            under(0.33)
        );
    }
}

/// A count as a float, through `u32`, so nothing is cast away where nobody looks.
fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}
