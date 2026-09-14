//! Which page boxes the corpus actually declares (14.11.2), and on how many pages.

fn main() {
    let boxes = ["MediaBox", "CropBox", "BleedBox", "TrimBox", "ArtBox"];
    println!("{:<22} {:>6}  {}", "file", "pages", boxes.join("  "));
    for path in std::env::args().skip(1) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(doc) = fepdf::PdfDocument::open(bytes.into()) else { continue };
        let count = doc.page_count().unwrap_or(0);
        let mut own = [0usize; 5];
        for index in 0..count {
            let Ok(page) = doc.inner().get_page(index) else { continue };
            for (slot, name) in boxes.iter().enumerate() {
                // `resolve_attribute` walks up the page tree; this asks what the page
                // itself declares, which is what a resize has to rewrite.
                if page.resolve_attribute(name).is_some() {
                    own[slot] += 1;
                }
            }
        }
        let name = std::path::Path::new(&path)
            .file_name()
            .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
        let cells: Vec<String> = own.iter().map(|n| format!("{n:>8}")).collect();
        println!("{name:<22} {count:>6}  {}", cells.join(" "));
    }
}
