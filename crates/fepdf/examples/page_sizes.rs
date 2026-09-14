//! Page sizes, for checking what fits in a window.

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(doc) = fepdf::PdfDocument::open(bytes.into()) else { continue };
        let count = doc.page_count().unwrap_or(0);
        let mut seen = std::collections::BTreeMap::new();
        for i in 0..count {
            if let Ok((w, h)) = doc.get_page_size(i) {
                // Rounded to a whole point for the tally: two pages differing in the
                // sixth decimal are the same paper to anyone reading this.
                *seen.entry((format!("{w:.0}"), format!("{h:.0}"))).or_insert(0usize) += 1;
            }
        }
        println!("{path}");
        for ((w, h), n) in seen {
            println!("    {w} x {h}  ({n} pages)");
        }
    }
}
