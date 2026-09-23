//! How long opening a file takes, best of N, so a change to the reader can be measured.
//!
//! Written for ROADMAP W-A4, which asked what the arena's copy-on-read costs. The answer
//! came from running this before and after: `samples/intel_sdm.pdf` opened in 1.2706 s
//! and, once the three call sites that copied a dictionary to read one entry of it asked
//! instead, in 1.2328 s.
//!
//! ```text
//! cargo run --release --example open_timing -- samples/intel_sdm.pdf 5
//! ```
//!
//! **Best of N and not the mean**, because what is being compared is the work, and the
//! slowest run is whatever else the machine was doing.

use fepdf::{IngestionOptions, PdfDocument};

fn main() {
    let path = std::env::args().nth(1).expect("a pdf path");
    let runs: usize = std::env::args().nth(2).map_or(3, |s| s.parse().unwrap_or(3));
    let bytes = std::fs::read(&path).expect("the file reads");
    let mut best = std::time::Duration::MAX;
    let mut chars = 0usize;
    for _ in 0..runs {
        let t = std::time::Instant::now();
        let doc =
            PdfDocument::open_with_options(bytes.clone().into(), &IngestionOptions::default())
                .expect("it opens");
        let d = t.elapsed();
        if d < best {
            best = d;
        }
        chars = (0..20.min(doc.page_count().unwrap_or(0)))
            .map(|p| doc.extract_text(p).unwrap_or_default().len())
            .sum();
    }
    println!("open (best of {runs}) {best:?}   [{chars} chars extracted]");
}
