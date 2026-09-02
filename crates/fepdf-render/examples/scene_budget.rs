//! How close a page's scene comes to vello's fixed bin-data buffer.
//!
//! **`binning_size` is a subtraction with no floor.** `vello_encoding`'s `RenderConfig`
//! computes `bin_data.len() - layout.bin_data_start`, where `bin_data` is a fixed
//! `1 << 18` words and `bin_data_start` is the sum of every draw tag's `info_size()` —
//! which grows with the scene and has no bound. Cross the two and the subtraction
//! underflows: a panic in a debug build, and in release a wrap to something near 2^32
//! that is then used to size and dispatch GPU work.
//!
//! The viewer panicked there once on `samples/volvo_xc90.pdf`, which is what this exists
//! to size. `headless.rs` already records the release-side symptom on the same file —
//! "the scene exceeded what the GPU buffers could take and vello reported success anyway".
//!
//! ```text
//! cargo run --release -p fepdf-render --example scene_budget -- samples/volvo_xc90.pdf
//! ```

use fepdf::PdfDocument;
use fepdf_render::VelloBackend;
use std::sync::Arc;

/// `vello_encoding::config::BufferSizes` fixes this. `fepdf_render::budget` is what now
/// checks a scene against it; this probe reports what documents actually cost.
const BIN_DATA_WORDS: u32 = fepdf_render::budget::BIN_DATA_BUDGET;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.first().cloned().unwrap_or_default();
    let data = std::fs::read(&path)?;
    let mut doc = PdfDocument::open(data.into())?;
    doc.set_system_fonts((*VelloBackend::load_system_fonts()).clone());
    let pages = doc.page_count()?;

    println!("{path}: {pages} pages, budget {BIN_DATA_WORDS} words");
    let (mut worst, mut worst_page, mut over) = (0_u32, 0_usize, 0_usize);
    let mut costs: Vec<u32> = Vec::with_capacity(pages);
    for index in 0..pages {
        let r = doc.get_page_box(index)?;
        let h = (r.y2 - r.y1).abs();
        let scale = 4.0 / 3.0;
        let mut backend = VelloBackend::new(Arc::clone(&doc.inner().system_fonts));
        let at = kurbo::Affine::new([scale, 0.0, 0.0, -scale, 0.0, h * scale]);
        if doc.render_page(index, &mut backend, at).is_err() {
            continue;
        }
        // The same sum `resolve.rs` makes, from the encoding the engine just built.
        let start = fepdf_render::budget::bin_data_cost(backend.scene());
        costs.push(start);
        if start > BIN_DATA_WORDS {
            over += 1;
            println!(
                "  page {:<5} {start:>9} words  OVER by {}",
                index + 1,
                start - BIN_DATA_WORDS
            );
        }
        if start > worst {
            worst = start;
            worst_page = index + 1;
        }
    }
    let pct = f64::from(worst) * 100.0 / f64::from(BIN_DATA_WORDS);
    println!("  worst: page {worst_page} at {worst} words ({pct:.1}% of the budget)");
    println!("  pages over budget: {over}");
    report_composed_window(&costs);
    Ok(())
}

/// The viewer composes every *visible* page into one scene, so what matters is not the
/// worst page but the worst run of them. This reports the shortest run that crosses the
/// budget — the number of pages on screen at which the viewer would submit a scene that
/// underflows `binning_size`, were nothing checking.
fn report_composed_window(costs: &[u32]) {
    let mut shortest: Option<(usize, usize)> = None;
    for start in 0..costs.len() {
        let mut sum = 0_u32;
        for (n, cost) in costs[start..].iter().enumerate() {
            sum = sum.saturating_add(*cost);
            if sum > BIN_DATA_WORDS {
                let run = n + 1;
                if shortest.is_none_or(|(best, _)| run < best) {
                    shortest = Some((run, start + 1));
                }
                break;
            }
        }
    }
    match shortest {
        Some((run, at)) => println!(
            "  composed: {run} consecutive pages cross the budget (worst run starts at page {at})"
        ),
        None => println!("  composed: the whole document fits in one scene"),
    }
}
