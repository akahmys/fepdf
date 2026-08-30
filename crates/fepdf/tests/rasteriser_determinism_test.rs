//! The same page, rasterised twice, is the same bytes — on the rasteriser that promises it.
//!
//! **RR-15 Rule 10 makes determinism a rule and nothing was checking the renderer against
//! it.** The engine keeps it: the vello `Encoding` for a page is byte-identical across
//! repeated builds, which `fepdf-render`'s `render_determinism` example establishes by
//! fingerprinting it. Vello's GPU pipeline does not — one scene became three distinct
//! images in eight runs of `samples/sample.pdf` page 1, one isolated pixel apart at a
//! channel delta of 1 — and its CPU shaders do
//! ([ADR-0043](../../../docs/adr/0043-the-scene-repeats-and-the-rasteriser-does-not.md)).
//!
//! **So this asserts of `Cpu` what cannot be asserted of `Gpu`, and both halves were
//! measured rather than assumed.** Swapped to `Rasteriser::Gpu` it fails **3 runs in 6**;
//! left on `Cpu` it passes **8 in 8**. The failure is the defect rather than a reason to
//! weaken the assertion: the value of `Rasteriser::Cpu` is exactly that this holds, so if
//! it ever stops holding the parameter has stopped meaning anything.
//!
//! It costs about 18 seconds, which is a page of Japanese rasterised twice on the host in
//! a debug build.
//!
//! Skips when `samples/` is absent. `.gitignore` excludes it, so a fresh clone has no
//! corpus and this is not a failure.

use fepdf::{PdfDocument, Rasteriser};

/// Renders page 1 of `name` to `out` with `rasteriser`, or `None` if the sample is absent.
fn render(name: &str, out: &std::path::Path, rasteriser: Rasteriser) -> Option<Vec<u8>> {
    let path = format!("../../samples/{name}");
    let data = std::fs::read(&path).ok()?;
    let doc = PdfDocument::open(data.into()).expect("the sample opens");
    doc.render_page_to_file_with(0, out, rasteriser).expect("the page renders");
    Some(std::fs::read(out).expect("the image was written"))
}

/// **A page the GPU is known to disagree with itself about.** The sample is load-bearing:
/// written against `print_sample.pdf` first, this passed on `Rasteriser::Gpu` six runs out
/// of six — green against the very defect it exists for, because that page happens not to
/// flake. `sample.pdf` page 1 is one of the pages measured to come out two ways.
const SAMPLE: &str = "sample.pdf";

#[test]
fn the_cpu_rasteriser_draws_the_same_page_the_same_way_twice() {
    let dir = std::env::temp_dir();
    let (first_path, second_path) =
        (dir.join("fepdf-det-cpu-1.png"), dir.join("fepdf-det-cpu-2.png"));

    let Some(first) = render(SAMPLE, &first_path, Rasteriser::Cpu) else {
        eprintln!("Sample {SAMPLE} not found, skipping");
        return;
    };
    let second = render(SAMPLE, &second_path, Rasteriser::Cpu).expect("the sample is still there");

    assert!(!first.is_empty(), "the first render produced no bytes");
    assert_eq!(
        first, second,
        "two CPU renders of {SAMPLE} page 1 differ, so `Rasteriser::Cpu` no longer means \
         what ADR-0043 says it means — the repeatable image is the whole reason it exists"
    );
}

/// The control. Without it, "the CPU repeats itself" and "the renderer drew nothing at
/// all, twice" are the same green test.
#[test]
fn the_cpu_rasteriser_draws_something() {
    let out = std::env::temp_dir().join("fepdf-det-cpu-control.png");
    let Some(pixels) = render(SAMPLE, &out, Rasteriser::Cpu) else {
        eprintln!("Sample {SAMPLE} not found, skipping");
        return;
    };
    // A PNG of a page carrying text and rules cannot be a few hundred bytes; an empty or
    // uniformly white one compresses to almost nothing.
    assert!(
        pixels.len() > 4096,
        "the CPU render of {SAMPLE} came back as {} bytes, which is a blank page",
        pixels.len()
    );
}
