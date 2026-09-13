//! Page counts, for checking what an insertion produced.

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let count = fepdf::PdfDocument::open(bytes.into())
            .ok()
            .and_then(|d| d.page_count().ok())
            .map_or_else(|| "?".to_string(), |n| n.to_string());
        println!("{count:>6}  {path}");
    }
}
