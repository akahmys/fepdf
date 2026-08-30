//! What is actually settled when `Document::open` returns, and what is not.
//!
//! `ARCHITECTURE.md` §4.4 is the claim under test: *"A `Document` is one normalised state,
//! not the file … everything above happens before application code sees anything."* Three
//! things it names hold — the revision chain is merged, the ciphertext is gone, the
//! metadata has one answer. **Fonts do not**, and a count is what shows it: the cache is
//! empty on every sample when `open` returns, and fills as pages are drawn.
//!
//! ```text
//! cargo run --release -p fepdf --example load_state_probe -- samples/*.pdf
//! ```
//!
//! The resource that fills it is not the one the ingest pass built. `normalize_resources`
//! clears the cache the ingest pass populated, and `Interpreter::get_font` rebuilds each
//! font on demand by a different route — merging a Type0's descendant into it and running
//! reconstruction again — then writes it back. So the decisions recorded at load describe
//! resources that no longer exist by the time anything is drawn, which is why fixing the
//! load-time path alone moved no number in
//! [ADR-0041](../../../docs/adr/0041-a-character-collection-is-declared-not-guessed.md).

use fepdf::PdfDocument;
use fepdf_doc::remediation::TextExtractionBackend;
use kurbo::Affine;

/// Pages to draw before asking again. Enough to make the point on every sample without
/// walking `intel_sdm.pdf`'s five thousand.
const PAGES: usize = 40;

fn main() {
    println!("{:<26} {:>9} {:>9} {:>8}", "file", "at open", "after", "pages");
    for path in std::env::args().skip(1) {
        let Ok(data) = std::fs::read(&path) else {
            eprintln!("{path}: unreadable");
            continue;
        };
        let doc = match PdfDocument::open(data.into()) {
            Ok(doc) => doc,
            Err(e) => {
                eprintln!("{path}: {e:?}");
                continue;
            }
        };
        let before = doc.inner().font_cache.read().len();
        let pages = doc.page_count().unwrap_or(0).min(PAGES);
        let mut backend = TextExtractionBackend::new();
        for index in 0..pages {
            // A page that will not interpret has still asked for its fonts, which is the
            // only thing being counted here.
            let _ = doc.render_page(index, &mut backend, Affine::IDENTITY);
        }
        let after = doc.inner().font_cache.read().len();
        let name = path.rsplit('/').next().unwrap_or(path.as_str());
        println!("{name:<26} {before:>9} {after:>9} {pages:>8}");
    }
}
