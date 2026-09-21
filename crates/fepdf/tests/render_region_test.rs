//! A rectangle of a page, rasterised — the read a snapshot is made of.
//!
//! **What decides is that the region agrees with the whole.** A region rendered on its own
//! and the same region cut out of the whole page rendered at the same scale are two routes
//! to one answer, and a transform that is subtly wrong gives two pictures that each look
//! plausible. Comparing them is the only way to tell.
//!
//! The rasteriser is named `Cpu` throughout: the GPU pipeline turns one scene into more
//! than one image — three distinct images in eight renders of one page, one isolated pixel
//! apart ([ADR-0043](../../../docs/adr/0043-the-scene-repeats-and-the-rasteriser-does-not.md))
//! — and a test comparing two renders cannot be the thing that decides which of them is
//! the difference.

use fepdf::{IngestionOptions, PdfDocument, Rasteriser};

fn opened(name: &str) -> PdfDocument {
    let bytes = std::fs::read(format!("../../samples/{name}")).expect("the sample is there");
    PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the sample opens")
}

/// The pixel at `(x, y)` of an RGBA image `width` across.
fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    pixels[at..at + 4].try_into().expect("four channels")
}

/// **A region is what the whole page shows there.**
///
/// The page is rendered whole **by the route that already existed** —
/// `render_page_to_file_with`, which works out its own transform and its own size — and a
/// region of it is rendered by the new one. Every pixel of the second has to be the pixel
/// of the first at the place the region was taken from.
///
/// **The whole page has to come from somewhere else, and a first version of this did not
/// notice.** It rendered the whole page through `render_region` too, so both sides shared
/// a transform: dropping the vertical flip from it failed nothing, because the comparison
/// flipped with it. A test of a thing against itself passes whatever the thing does.
///
/// **The remaining difference was measured before it was allowed** (2026-09-21,
/// `print_sample.pdf` page 3, the region below): the two images are different sizes, so an
/// anti-aliased edge is averaged from a different neighbourhood. The tolerance is what
/// that measurement bounds, and a transform that is wrong moves every edge rather than
/// blurring a few.
#[test]
fn a_region_is_the_part_of_the_page_it_was_taken_from() {
    let doc = opened("print_sample.pdf");
    let page = doc.get_page_box(2).expect("the page has a box");
    // The scale `render_page_to_file` draws at: 96 DPI, times the page's user unit.
    let scale = (4.0 / 3.0) * doc.get_page_user_unit(2).expect("the page states one");
    // **Chosen to land on the page image's own pixel grid.** At 4/3 a rectangle starting
    // at 100 begins a third of a pixel into one, and a glyph edge sampled a third of a
    // pixel over is a different colour — 142 of 255 at the worst, which is the distance
    // between two renderings of the same thing rather than a defect in either. Multiples
    // of three land whole: 99, 300, 399 and 501 are 132, 400, 508 and 372 pixels.
    let keep = (99.0, 399.0, 300.0, 501.0);

    let path = std::env::temp_dir().join(format!("fepdf_region_whole_{}.png", std::process::id()));
    doc.render_page_to_file_with(2, &path, Rasteriser::Cpu).expect("the page renders");
    let whole = image::open(&path).expect("what was written reads").to_rgba8();
    let _ = std::fs::remove_file(&path);

    let (part, part_wide, part_tall) =
        doc.render_region_with(2, keep, scale, Rasteriser::Cpu).expect("the region renders");

    // Where the region's top-left corner falls in the whole page's image: across from the
    // page's left edge, and down from its top.
    // The region was chosen to land on whole pixels, so these are exact rather than
    // rounded into place — and a corner that did not land on one would be caught here
    // rather than silently compared against the wrong column.
    let whole_pixels = |points: f64| {
        let at = points * scale;
        assert!((at - at.round()).abs() < 1e-9, "{points} points is {at} pixels, not a whole one");
        // Counted up rather than cast, and bounded by the widest page this format can
        // express — 14,400 points at any sane scale is fewer pixels than this.
        (0u32..1_000_000).find(|n| f64::from(*n) >= at - 0.5).expect("a corner inside the page")
    };
    let left = whole_pixels(keep.0 - page.x1);
    let top = whole_pixels(page.y2 - keep.3);
    assert!(
        left + part_wide <= whole.width() && top + part_tall <= whole.height(),
        "the region does not fit in the page's image, so this compares nothing"
    );

    let (mut differing, mut worst) = (0u32, 0i32);
    for y in 0..part_tall {
        for x in 0..part_wide {
            let here = pixel(&part, part_wide, x, y);
            let there = whole.get_pixel(left + x, top + y).0;
            let apart =
                (0..4).map(|c| (i32::from(here[c]) - i32::from(there[c])).abs()).max().unwrap_or(0);
            if apart > 0 {
                differing += 1;
                worst = worst.max(apart);
            }
        }
    }
    let total = part_wide * part_tall;
    assert!(worst <= 8, "a pixel of the region is {worst} of 255 away from the page");
    assert!(
        differing * 20 < total,
        "{differing} of {total} pixels differ, which is too many for two sizes of the same \
         picture — the region is not where it says it is"
    );
}

/// **The scale is the caller's, and asking for twice gives twice.**
///
/// A snapshot taken at whatever the screen happens to be showing is one nobody can ask
/// for twice.
#[test]
fn a_region_asked_for_at_twice_the_scale_is_twice_as_many_pixels() {
    let doc = opened("print_sample.pdf");
    let keep = (100.0, 400.0, 300.0, 500.0);

    let (_, wide, tall) =
        doc.render_region_with(2, keep, 1.0, Rasteriser::Cpu).expect("it renders");
    let (_, twice_wide, twice_tall) =
        doc.render_region_with(2, keep, 2.0, Rasteriser::Cpu).expect("it renders");

    assert_eq!((wide, tall), (200, 100), "the region is not the size it was asked for");
    assert_eq!((twice_wide, twice_tall), (400, 200), "twice the scale is not twice the image");
}

/// A region the reader dragged is never answered as no image.
#[test]
fn a_region_smaller_than_a_pixel_still_makes_one() {
    let doc = opened("print_sample.pdf");
    let (pixels, wide, tall) = doc
        .render_region_with(2, (100.0, 400.0, 100.3, 400.3), 1.0, Rasteriser::Cpu)
        .expect("it renders");
    assert_eq!((wide, tall), (1, 1), "a third of a point came out as {wide} by {tall}");
    assert_eq!(pixels.len(), 4, "one pixel is four channels");
}

/// A region with no area, and a scale that draws nothing, are refused by name.
#[test]
fn a_region_with_no_area_and_a_scale_of_nothing_are_refused() {
    let doc = opened("print_sample.pdf");
    let flat = doc
        .render_region_with(2, (100.0, 400.0, 100.0, 500.0), 1.0, Rasteriser::Cpu)
        .expect_err("it refuses");
    assert!(flat.to_string().contains("no area"), "the refusal does not say why: {flat}");

    let nothing = doc
        .render_region_with(2, (100.0, 400.0, 300.0, 500.0), 0.0, Rasteriser::Cpu)
        .expect_err("it refuses");
    assert!(
        nothing.to_string().contains("draws nothing"),
        "the refusal does not say why: {nothing}"
    );
}
