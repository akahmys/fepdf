//! How two documents differ, and how long it took to find out (ROADMAP W-18).
//!
//! ```bash
//! cargo run --release --example compare_probe --features render -- a.pdf b.pdf
//! ```

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(a), Some(b)) = (args.next(), args.next()) else { return };
    let open = |path: &str| {
        std::fs::read(path).ok().and_then(|bytes| fepdf::PdfDocument::open(bytes.into()).ok())
    };
    let (Some(a), Some(b)) = (open(&a), open(&b)) else { return };
    let started = std::time::Instant::now();
    let Ok(comparison) = fepdf::compare::compare(&a, &b, 72.0) else { return };
    println!(
        "{:?} pages, {} differ, in {:.2?}",
        comparison.pages,
        comparison.differences.len(),
        started.elapsed()
    );
    for difference in comparison.differences.iter().take(5) {
        println!(
            "  page {}: -{:?} +{:?} regions {:?}",
            difference.page + 1,
            difference.removed,
            difference.added,
            difference.regions
        );
    }
}
