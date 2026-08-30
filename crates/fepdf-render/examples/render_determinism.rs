//! Does the same page produce the same picture twice, and if not, where does it stop?
//!
//! **Measured: it does not.** Repeated renders of one page with one binary give two
//! different PNGs — `samples/sample.pdf` page 1 four and four out of eight, differing at
//! one isolated pixel. RR-15 Rule 10 makes determinism a rule, so this exists to say
//! *which layer* is not keeping it, because the fix is different at each:
//!
//! 1. **The scene** — the engine walked the page differently. That would be ours, and a
//!    defect in the interpreter or the backend.
//! 2. **The rasteriser** — the same scene came out as different pixels. That is Vello on
//!    wgpu, below anything this workspace owns.
//!
//! It is the second. See [ADR-0043] for what follows from that.
//!
//! [ADR-0043]: https://github.com/akahmys/fepdf/blob/main/docs/adr/0043-the-scene-repeats-and-the-rasteriser-does-not.md
//!
//! The scene is fingerprinted rather than compared field by field: `path_data` and
//! `draw_data` carry every coordinate and every colour in the order they were encoded, so
//! two glyphs swapping places changes the hash. The stream lengths and the counts vello
//! keeps beside them are hashed too, which catches a difference that lands in a stream
//! this does not read.
//!
//! ```text
//! cargo run --release -p fepdf-render --example render_determinism -- samples/sample.pdf 1 8
//! ```

use fepdf::PdfDocument;
use fepdf_render::VelloBackend;
use fepdf_render::headless::Rasteriser;
use std::collections::BTreeMap;
use std::sync::Arc;

/// FNV-1a over a `u32` stream. Not cryptographic — this only has to separate runs.
fn fold(hash: &mut u64, words: &[u32]) {
    for word in words {
        for byte in word.to_le_bytes() {
            *hash ^= u64::from(byte);
            *hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

/// A fingerprint of everything the engine encoded for this page.
fn fingerprint(scene: &vello::Scene) -> u64 {
    let e = scene.encoding();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    fold(&mut hash, &e.path_data);
    fold(&mut hash, &e.draw_data);
    // Lengths and counts, so a difference confined to a stream this does not walk —
    // transforms, styles, the glyph buffers — still moves the number.
    let counts = [
        u32::try_from(e.path_tags.len()).unwrap_or(u32::MAX),
        u32::try_from(e.draw_tags.len()).unwrap_or(u32::MAX),
        u32::try_from(e.transforms.len()).unwrap_or(u32::MAX),
        u32::try_from(e.styles.len()).unwrap_or(u32::MAX),
        u32::try_from(e.resources.glyphs.len()).unwrap_or(u32::MAX),
        u32::try_from(e.resources.glyph_runs.len()).unwrap_or(u32::MAX),
        u32::try_from(e.resources.patches.len()).unwrap_or(u32::MAX),
        u32::try_from(e.resources.color_stops.len()).unwrap_or(u32::MAX),
        e.n_paths,
        e.n_path_segments,
        e.n_clips,
        e.n_open_clips,
        e.flags,
    ];
    fold(&mut hash, &counts);
    hash
}

/// Builds the scene for one page, exactly as `render_page_to_file` does.
fn build_scene(
    doc: &PdfDocument,
    index: usize,
) -> Result<vello::Scene, Box<dyn std::error::Error>> {
    let r = doc.get_page_box(index)?;
    let h = (r.y2 - r.y1).abs();
    let scale = 4.0 / 3.0; // exactly 96 DPI, as the CLI renders
    let mut backend = VelloBackend::new(Arc::clone(&doc.inner().system_fonts));
    // Unrotated: every page this is pointed at is upright, and a rotation here would
    // change the scene without changing what the comparison is asking.
    let at = kurbo::Affine::new([scale, 0.0, 0.0, -scale, 0.0, h * scale]);
    doc.render_page(index, &mut backend, at)?;
    Ok(backend.scene().clone())
}

/// How many distinct values, and how often each.
fn tally<T: Ord + Copy>(values: &[T]) -> BTreeMap<T, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(*value).or_default() += 1;
    }
    counts
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.first().map_or("samples/sample.pdf", String::as_str);
    let page: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let runs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(8);

    let data = std::fs::read(path)?;
    let mut doc = PdfDocument::open(bytes::Bytes::from(data))?;
    doc.set_system_fonts((*VelloBackend::load_system_fonts()).clone());
    let index = page.saturating_sub(1);

    println!("{path} page {page}, {runs} runs\n");

    // 1. The scene, built afresh each time.
    let mut scenes = Vec::new();
    let mut prints = Vec::new();
    for _ in 0..runs {
        let scene = build_scene(&doc, index)?;
        prints.push(fingerprint(&scene));
        scenes.push(scene);
    }
    let scene_counts = tally(&prints);
    println!("  scene built {runs} times   -> {} distinct", scene_counts.len());
    for (print, n) in &scene_counts {
        println!("      {print:#018x}  x{n}");
    }

    // 2. The rasteriser, given *one* scene every time. Only reached when the scene is
    //    stable — comparing pixels from scenes that already differ says nothing.
    if scene_counts.len() > 1 {
        println!("\n  the scene itself differs, so the rasteriser is not the question yet");
        return Ok(());
    }
    let (width, height) = {
        let r = doc.get_page_box(index)?;
        let scale = 4.0 / 3.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        (
            (((r.x2 - r.x1).abs()) * scale).round() as u32,
            (((r.y2 - r.y1).abs()) * scale).round() as u32,
        )
    };
    let scene = scenes.first().ok_or("no scene was built")?;
    let mut verdicts = Vec::new();
    for rasteriser in [Rasteriser::Gpu, Rasteriser::Cpu] {
        let mut pixel_prints = Vec::new();
        for _ in 0..runs {
            let pixels = pollster::block_on(fepdf_render::headless::render_to_bytes_with(
                scene, width, height, rasteriser,
            ))?;
            let mut hash = 0xcbf2_9ce4_8422_2325_u64;
            for byte in &pixels {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
            pixel_prints.push(hash);
        }
        let counts = tally(&pixel_prints);
        println!(
            "\n  one scene, {rasteriser:?} rasteriser, {runs} times -> {} distinct",
            counts.len()
        );
        for (print, n) in &counts {
            println!("      {print:#018x}  x{n}");
        }
        verdicts.push((rasteriser, counts.len()));
    }

    println!();
    for (rasteriser, distinct) in verdicts {
        if distinct > 1 {
            println!("  {rasteriser:?}: the same scene rasterises to {distinct} different images");
        } else {
            println!("  {rasteriser:?}: one scene, one image");
        }
    }
    Ok(())
}
