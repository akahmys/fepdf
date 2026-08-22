//! Which files contain a given PDF name, anywhere.
//!
//! Names are interned as a file is read, so asking whether a document ever mentioned
//! `/DefaultCMYK` is a lookup rather than a search — and it reaches names inside object
//! streams and compressed streams, which `grep` does not.
//!
//! **A name this engine interns on its own would show as present everywhere.** The
//! reader calls `intern_name` for the keys it looks up regardless of what the file holds,
//! so this only answers honestly for names the engine never mentions itself. Check a
//! control before believing a count: a file known to carry the name must report it and a
//! file known not to must not.
//!
//! ```text
//! cargo run --release -p fepdf --example survey_names -- DefaultCMYK,DefaultRGB samples/*.pdf
//! ```

use fepdf::PdfDocument;
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(names) = args.next() else {
        eprintln!("usage: survey_names <Name,Name,…> <file.pdf>…");
        return;
    };
    let wanted: Vec<&str> = names.split(',').filter(|n| !n.is_empty()).collect();
    let paths: Vec<String> = args.collect();

    let mut carriers: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let (mut opened, mut refused) = (0_usize, 0_usize);

    for path in &paths {
        let Ok(bytes) = std::fs::read(path) else {
            refused += 1;
            continue;
        };
        let Ok(doc) = PdfDocument::open(bytes.into()) else {
            refused += 1;
            continue;
        };
        opened += 1;
        let arena = doc.inner().arena();
        let short = std::path::Path::new(path)
            .file_name()
            .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
        for name in &wanted {
            if arena.get_name_by_str(name).is_some() {
                carriers.entry(name).or_default().push(short.clone());
            }
        }
    }

    println!("files opened {opened}, would not open {refused}");
    for name in &wanted {
        let files = carriers.get(name).map(Vec::as_slice).unwrap_or_default();
        println!("\n/{name}: {} files", files.len());
        for file in files.iter().take(10) {
            println!("    {file}");
        }
        if files.len() > 10 {
            println!("    … and {} more", files.len() - 10);
        }
    }
}
