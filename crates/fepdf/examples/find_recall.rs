//! How many of the words extraction reads each search finds on the page it read them on.
//!
//! ROADMAP W-15 moved the studio's search from extraction's spans to the runs. The old
//! search found a word when one span held it; the new one when a stretch of runs the text
//! matrix carries through does. Neither is the truth — extraction's words are its own
//! reading — so this is a comparison, not a score: it says where the two disagree.
use fepdf::{IngestionOptions, PdfDocument};
use std::collections::BTreeSet;

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("samples/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("pdf"))
        .collect();
    files.sort();
    println!("{:<24} {:>7} {:>8} {:>8} {:>8}", "file", "words", "spans", "runs", "both");
    for f in files {
        let Ok(bytes) = std::fs::read(&f) else { continue };
        let Ok(doc) = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        else {
            continue;
        };
        let (mut words, mut in_spans, mut in_runs, mut in_both) = (0, 0, 0, 0);
        for page in 0..doc.page_count().unwrap_or(0).min(5) {
            let spans = doc.extract_spans(page).unwrap_or_default();
            let runs = fepdf::text::runs_of_page(doc.inner(), page).unwrap_or_default();
            let stretches = fepdf::text::stretches_of(&runs);
            let wanted: BTreeSet<String> = spans
                .iter()
                .flat_map(|s| s.text.split_whitespace().map(str::to_string).collect::<Vec<_>>())
                .filter(|w| w.chars().count() >= 2)
                .collect();
            for word in wanted {
                let span = spans.iter().any(|s| s.text.contains(&word));
                let run = stretches.iter().any(|s| s.text.contains(&word));
                words += 1;
                in_spans += usize::from(span);
                in_runs += usize::from(run);
                in_both += usize::from(span && run);
                if span && !run && std::env::var_os("MISSED").is_some() {
                    let near: Vec<&str> = stretches
                        .iter()
                        .map(|s| s.text.as_str())
                        .filter(|t| word.chars().next().is_some_and(|c| t.contains(c)))
                        .take(3)
                        .collect();
                    println!("  missed p{page} {word:?} near {near:?}");
                }
            }
        }
        let name = f.file_name().unwrap_or_default().to_string_lossy().to_string();
        println!("{name:<24} {words:>7} {in_spans:>8} {in_runs:>8} {in_both:>8}");
    }
}
