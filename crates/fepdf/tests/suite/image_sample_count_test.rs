//! What a backend is handed must match what the image dictionary describes.
//!
//! **The "headless rendering fails on a small page" entry was neither.** A 64×32 page
//! produced *"Copy at offset 0 for 8192 bytes would end up overrunning the bounds of the
//! Source buffer of size 1024"* from wgpu, and it was filed as a rendering defect and
//! worked around by enlarging a fixture. Reproducing it names the cause in arithmetic: a
//! 64×32 image at **one bit per component** decodes to 256 bytes, a backend that reads
//! one byte per pixel makes 256 pixels of RGBA out of them — 1024 bytes — and the texture
//! it is being written into is 64×32, which wants 8192. Eight times too short, which is
//! the same defect Phase M records fixing for `/DeviceGray` scans; the page size never
//! entered into it. Enlarging the fixture did not help either: 256×128 fails the same way
//! against the same code, by the same factor.
//!
//! It has been fixed since, in two independent places, and **neither had a test**. These
//! are that test, at the level the defect lives at: the bytes handed across the
//! [`RenderBackend`] contract. Asserting on those rather than on "the GPU did not crash"
//! is both faster and more precise, and it runs where there is no GPU at all.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_content::PixelFormat;
use kurbo::Affine;

use fepdf_fixtures::assemble;
use fepdf_fixtures::recorder::Recorder;

/// A page exactly the size of the image it draws, with `entries` in the image dictionary.
fn page_with_image(width: u32, height: u32, entries: &str, data: &[u8]) -> Vec<u8> {
    let content = format!("q {width} 0 0 {height} 0 0 cm /Im0 Do Q");
    let mut image = format!(
        "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} {entries} \
         /Length {} >>\nstream\n",
        data.len()
    )
    .into_bytes();
    image.extend_from_slice(data);
    image.extend_from_slice(b"\nendstream");

    assemble(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] \
             /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>"
        )
        .into_bytes(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
        image,
    ])
}

/// Interprets the page and reports what the backend got, with the decisions taken.
fn draw(file: Vec<u8>) -> (Recorder, Vec<String>) {
    let doc = PdfDocument::open_with_options(file.into(), &IngestionOptions::default())
        .expect("the fixture opens");
    let mut drawn = Recorder::new();
    doc.render_page(0, &mut drawn, Affine::IDENTITY).expect("the page interprets");
    let decisions = doc.decisions().iter().map(|d| format!("{} {}", d.clause, d.found)).collect();
    (drawn, decisions)
}

/// A 1-bit `/DeviceGray` image, black in the top-left quarter — the commonest image in a
/// scanned document, at the size that produced the wgpu failure.
fn one_bit_gray(width: u32, height: u32) -> Vec<u8> {
    let stride = (width as usize).div_ceil(8);
    let mut out = vec![0_u8; stride * height as usize];
    for y in 0..height {
        for x in 0..width {
            if x >= width / 2 || y >= height / 2 {
                // 1 is white in `/DeviceGray`; the top-left quarter stays 0, which is black.
                out[y as usize * stride + (x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
    }
    out
}

/// The failure, at the size it was reported at. 64×32 at one bit is 256 bytes; the
/// backend must be handed 2048, one per pixel, or the RGBA buffer it builds is 1024 bytes
/// against a texture wanting 8192.
#[test]
fn a_one_bit_gray_image_reaches_the_backend_one_byte_per_pixel() {
    let (drawn, decisions) = draw(page_with_image(
        64,
        32,
        "/ColorSpace /DeviceGray /BitsPerComponent 1",
        &one_bit_gray(64, 32),
    ));
    let image = drawn.last_image().expect("an image reached the backend");
    assert_eq!(image.size, (64, 32));
    assert_eq!(image.format, PixelFormat::Gray8);
    assert_eq!(
        image.samples.len(),
        64 * 32,
        "the samples are still packed eight to a byte, which is the 8x shortfall that \
         killed the process"
    );
    assert_eq!(image.samples[0], 0, "the top-left quarter is black");
    assert_eq!(image.samples[63], 255, "the top-right is white");
    assert!(decisions.is_empty(), "expanding conforming samples is not a departure: {decisions:?}");
}

/// The page size never entered into it: the same image at the size the fixture was
/// enlarged to fails and passes for exactly the same reason.
#[test]
fn the_same_holds_at_the_size_the_fixture_was_enlarged_to() {
    let (drawn, _) = draw(page_with_image(
        256,
        128,
        "/ColorSpace /DeviceGray /BitsPerComponent 1",
        &one_bit_gray(256, 128),
    ));
    let image = drawn.last_image().expect("an image reached the backend");
    assert_eq!(image.samples.len(), 256 * 128);
}

/// Four bits per component is the other sub-byte depth a scan uses, and a width that is
/// not a whole number of bytes is where a stride calculation goes wrong.
#[test]
fn a_sub_byte_depth_that_does_not_divide_the_width_still_expands() {
    let width = 5_u32;
    let height = 2_u32;
    // Two samples per byte, three bytes per row for five samples: the last nibble is padding.
    let data = [0x0F, 0x0F, 0x00, 0x0F, 0x0F, 0x00];
    let (drawn, _) =
        draw(page_with_image(width, height, "/ColorSpace /DeviceGray /BitsPerComponent 4", &data));
    let image = drawn.last_image().expect("an image reached the backend");
    assert_eq!(image.samples.len(), (width * height) as usize, "row padding was read as samples");
    assert_eq!(image.samples[0], 0, "the first sample is 0 of 15");
    assert_eq!(image.samples[1], 255, "the second is 15 of 15");
}

/// The second guard, independent of the first. An image whose data is short for what its
/// dictionary describes is skipped and recorded — because the alternative was handing the
/// GPU a buffer it refused, and the process died over a document defect.
#[test]
fn an_image_shorter_than_its_dictionary_describes_is_skipped_and_recorded() {
    let (drawn, decisions) =
        draw(page_with_image(64, 32, "/ColorSpace /DeviceGray /BitsPerComponent 8", &[0_u8; 100]));
    assert!(
        drawn.last_image().is_none(),
        "a short image reached the backend: {:?}",
        drawn.count("image")
    );
    assert!(
        decisions.iter().any(|d| d.starts_with("8.9.5.1") && d.contains("2048")),
        "the shortfall was not reported: {decisions:?}"
    );
}
