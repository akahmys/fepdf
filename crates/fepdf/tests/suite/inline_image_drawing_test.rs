//! **An inline image is drawn as an image XObject is** (8.9.7, ROADMAP Y-F33).
//!
//! The interpreter handed the backend the bytes after `ID`, still encoded, as eight-bit
//! RGB, so a filtered, grey, one-bit or indexed inline image drew as noise. Measured
//! 2026-10-06: the 17 inline images in the corpus each carry a filter or a colour space
//! named from the resources.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_fixtures::recorder::Recorder;
use fepdf_model::graphics::PixelFormat;
use kurbo::Affine;

/// A page whose resources are `resources` drawing `content`, and what it drew.
fn drawn(resources: &str, content: &[u8]) -> (Recorder, PdfDocument) {
    let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream.extend_from_slice(content);
    stream.extend_from_slice(b"\nendstream");
    let bytes = fepdf_fixtures::assemble(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << {resources} >> \
               /Contents 4 0 R >>"
        )
        .into_bytes(),
        stream,
    ]);
    let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the fixture opens");
    let mut marks = Recorder::new();
    doc.render_page(0, &mut marks, Affine::IDENTITY).expect("it renders");
    (marks, doc)
}

/// **A filter is undone and a grey image stays grey**: two samples written in hex reach
/// the backend as two grey bytes, not as their six hex digits read as RGB.
#[test]
fn a_filtered_grey_image_is_decoded() {
    let (marks, _) =
        drawn("", b"q 100 0 0 50 0 0 cm BI /W 2 /H 1 /CS /G /BPC 8 /F /AHx ID 00FF> EI Q");
    let image = marks.last_image().expect("the image is drawn");
    assert_eq!((image.size, image.format), ((2, 1), PixelFormat::Gray8));
    assert_eq!(image.samples, [0x00, 0xFF]);
}

/// **A colour space the resources name is the resources' space** (8.9.7): `/CS0` an
/// `/Indexed` one over RGB, so the indices become the colours its table gives.
#[test]
fn an_indexed_space_named_from_the_resources_is_looked_up() {
    let (marks, _) = drawn(
        "/ColorSpace << /CS0 [/Indexed /DeviceRGB 1 <FF000000FF00>] >>",
        b"q 100 0 0 50 0 0 cm BI /W 2 /H 1 /CS /CS0 /BPC 8 ID \x00\x01 EI Q",
    );
    let image = marks.last_image().expect("the image is drawn");
    assert_eq!(image.format, PixelFormat::Rgb8);
    assert_eq!(image.samples, [0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00]);
}

/// **A one-bit image is eight samples to the byte**, made bytes before the backend sees
/// them, as an image XObject's are.
#[test]
fn a_one_bit_image_is_expanded() {
    let (marks, _) = drawn("", b"q 100 0 0 50 0 0 cm BI /W 8 /H 1 /CS /G /BPC 1 ID \xF0 EI Q");
    let image = marks.last_image().expect("the image is drawn");
    assert_eq!((image.size, image.format), ((8, 1), PixelFormat::Gray8));
    assert_eq!(image.samples, [255, 255, 255, 255, 0, 0, 0, 0]);
}

/// **One that will not decode is skipped and said**, and the page goes on.
#[test]
fn an_inline_image_that_will_not_decode_is_recorded() {
    let (marks, doc) =
        drawn("", b"q 100 0 0 50 0 0 cm BI /W 2 /H 2 /CS /RGB /BPC 8 /F /DCT ID nonsense EI Q");
    assert!(marks.last_image().is_none(), "noise was drawn");
    assert!(
        doc.decisions().iter().any(|d| d.clause == "8.9.7" && d.found.contains("inline image")),
        "{:?}",
        doc.decisions()
    );
}

/// **A key written both ways is read abbreviated, and that is said** (8.9.7 is silent on
/// a header giving both): `/F /AHx` with a contradicting `/Filter /A85` decodes as hex.
#[test]
fn a_key_written_both_ways_is_read_abbreviated() {
    let (marks, doc) = drawn(
        "",
        b"q 100 0 0 50 0 0 cm BI /W 2 /H 1 /CS /G /BPC 8 /F /AHx /Filter /A85 ID 00FF> EI Q",
    );
    let image = marks.last_image().expect("the image is drawn");
    assert_eq!(image.samples, [0x00, 0xFF]);
    assert!(
        doc.decisions().iter().any(|d| d.clause == "8.9.7" && d.found.contains("/Filter")),
        "{:?}",
        doc.decisions()
    );
}
