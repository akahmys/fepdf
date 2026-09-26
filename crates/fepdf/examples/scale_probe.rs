//! Which pages of the samples declare a scale to measure in (12.9, ROADMAP W-16).
//!
//! ```bash
//! cargo run --release --example scale_probe
//! ```

fn main() {
    let Ok(entries) = std::fs::read_dir("samples") else { return };
    let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths.iter().filter(|p| p.extension().is_some_and(|e| e == "pdf")) {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Ok(doc) = fepdf::PdfDocument::open(bytes.into()) else { continue };
        let declaring: Vec<(usize, String)> = (0..doc.page_count().unwrap_or(0))
            .filter_map(|page| {
                let scales = doc.scales_on(page);
                scales.first().map(|scale| (page + 1, scale.ratio.clone()))
            })
            .collect();
        println!("{}: {} pages declare a scale {declaring:?}", path.display(), declaring.len());
    }
}
