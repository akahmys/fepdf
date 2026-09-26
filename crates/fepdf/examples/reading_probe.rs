//! What `PdfDocument::reading` hands a synthesiser for each sample (ROADMAP W-19a): how
//! many passages, from where, in which languages, and how the first few begin.
//!
//! ```bash
//! cargo run --release --example reading_probe
//! ```

fn main() {
    let Ok(entries) = std::fs::read_dir("samples") else { return };
    let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths.iter().filter(|p| p.extension().is_some_and(|e| e == "pdf")) {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Ok(doc) = fepdf::PdfDocument::open(bytes.into()) else { continue };
        let started = std::time::Instant::now();
        let reading = doc.reading();
        let mut spoken = std::collections::BTreeMap::new();
        let mut langs = std::collections::BTreeMap::new();
        for passage in &reading.passages {
            *spoken.entry(format!("{:?}", passage.spoken)).or_insert(0) += 1;
            *langs.entry(passage.lang.clone().unwrap_or_default()).or_insert(0) += 1;
        }
        println!(
            "{}: {} passages in {:.2?} {spoken:?} langs {langs:?} lexicons {}",
            path.display(),
            reading.passages.len(),
            started.elapsed(),
            reading.lexicons.len()
        );
        for passage in reading.passages.iter().take(4) {
            let head: String = passage.text.chars().take(40).collect();
            println!("    <{}> {:?}", passage.tag, head.replace('\n', " / "));
        }
    }
}
