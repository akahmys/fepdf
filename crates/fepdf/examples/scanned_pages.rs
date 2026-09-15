//! What a resize meets on a scanned page: rotation, and pages that are one image.

fn main() {
    println!("{:<22} {:>6} {:>8} {:>10} {:>12}", "file", "pages", "rotated", "no text", "sizes");
    for path in std::env::args().skip(1) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(doc) = fepdf::PdfDocument::open(bytes.into()) else { continue };
        let count = doc.page_count().unwrap_or(0);
        let mut rotated = 0;
        let mut textless = 0;
        let mut sizes = std::collections::BTreeSet::new();
        for index in 0..count {
            if doc.get_page_rotation(index).unwrap_or(0) % 360 != 0 {
                rotated += 1;
            }
            if doc.extract_text(index).is_ok_and(|t| t.trim().is_empty()) {
                textless += 1;
            }
            if let Ok((w, h)) = doc.get_page_size(index) {
                sizes.insert((format!("{w:.0}"), format!("{h:.0}")));
            }
        }
        let name = std::path::Path::new(&path)
            .file_name()
            .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
        println!("{name:<22} {count:>6} {rotated:>8} {textless:>10} {:>12}", sizes.len());
    }
}
