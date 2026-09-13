//! What `/Outlines` holds, across every sample.

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        args = glob_samples();
    }
    println!("{:<24} {:>7} {:>9} {:>7}  first title", "file", "items", "placeless", "looped");
    for path in args {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(doc) = fepdf::PdfDocument::open(bytes.into()) else {
            println!("{path:<24} unreadable");
            continue;
        };
        let (tree, report) = fepdf_doc::read_outlines(doc.inner());
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or(path.clone());
        let first = tree.items.first().map_or_else(
            || "-".to_string(),
            |n| format!("{:?} → p{}", n.title, n.destination_page),
        );
        println!(
            "{:<24} {:>7} {:>9} {:>7}  {}",
            name, report.items, report.placeless, report.looped, first
        );
    }
}

fn glob_samples() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(dir) = std::fs::read_dir("samples") {
        for entry in dir.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "pdf") {
                out.push(p.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}
