//! The text of one page, for checking what an extraction wrote.

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else { return };
    let index: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    let Ok(bytes) = std::fs::read(&path) else { return };
    let Ok(doc) = fepdf::PdfDocument::open(bytes.into()) else { return };
    match doc.extract_text(index) {
        Ok(text) => println!("{}", text.trim().lines().take(3).collect::<Vec<_>>().join(" / ")),
        Err(why) => println!("{why:?}"),
    }
}
