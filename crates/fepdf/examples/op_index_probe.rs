//! What `TextSpan::op_index` says, with refinement on and off (ROADMAP W-E3a).
//!
//! It named an operator of the bytes a page was read from, and a document held as
//! pre-sublimated commands has none — so on the default path it was `0` for every span.
//! Measured on `samples/constitution.pdf` 2026-09-24: 1,007 spans, 1,007 distinct indices
//! with refinement off and **one** with it on. It is an `Option` now, and the default path
//! answers `None`.
//!
//! ```text
//! cargo run --release --example op_index_probe -- samples/constitution.pdf
//! ```
use fepdf::{IngestionOptions, PdfDocument};

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "samples/constitution.pdf".into());
    let bytes = std::fs::read(&path).expect("the file reads");
    for refine in [false, true] {
        let options = IngestionOptions { active_refinement: refine, ..IngestionOptions::default() };
        let Ok(doc) = PdfDocument::open_with_options(bytes.clone().into(), &options) else {
            println!("refinement={refine:5}  will not open");
            continue;
        };
        let spans = doc.extract_spans(0).unwrap_or_default();
        let mut seen: Vec<Option<usize>> = spans.iter().map(|s| s.op_index).collect();
        seen.sort_unstable();
        seen.dedup();
        let first: Vec<String> =
            spans.iter().take(6).map(|s| format!("{:?}", s.op_index)).collect();
        println!(
            "refinement={refine:5}  spans={:<5} distinct op_index={:<5} first six: {}",
            spans.len(),
            seen.len(),
            first.join(", ")
        );
    }
}
