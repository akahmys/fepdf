//! How long a run actually is, per sample — the measurement ROADMAP W-E3d rests on.
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
        "{:<34} {:>6} {:>7} {:>7} {:>8} {:>7}",
        "file", "runs", "median", "mean", "single%", "max"
    );
    for f in files {
        let Ok(bytes) = std::fs::read(&f) else { continue };
        let Ok(doc) = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        else {
            continue;
        };
        let pages = doc.page_count().unwrap_or(0).min(30);
        let mut lens: Vec<usize> = Vec::new();
        for p in 0..pages {
            for run in fepdf::text::runs_of_page(doc.inner(), p).unwrap_or_default() {
                lens.push(run.text.chars().count());
            }
        }
        if lens.is_empty() {
            continue;
        }
        lens.sort_unstable();
        let median = lens[lens.len() / 2];
        let mean = count(lens.iter().sum::<usize>()) / count(lens.len());
        let single = count(lens.iter().filter(|n| **n <= 1).count()) * 100.0 / count(lens.len());
        let name = f.file_name().unwrap_or_default().to_string_lossy().to_string();
        println!(
            "{name:<34} {:>6} {median:>7} {mean:>7.1} {single:>7.0}% {:>7}",
            lens.len(),
            lens.last().copied().unwrap_or(0)
        );
    }
}

/// A count as a float, through `u32`, so nothing is cast away where nobody looks.
fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}
