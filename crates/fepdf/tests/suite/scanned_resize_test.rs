//! Resizing the two things a scanned document is made of, neither of which this corpus
//! has: a page under a `/Rotate`, and a page that is one image.
//!
//! **Measured 2026-09-15 with `--example scanned_pages`: of 7,727 pages across the nine
//! samples, none carries a rotation and three have no text on them.** So these are
//! hand-built rather than taken from a file someone produced, and they are here because
//! the absence is the corpus's and not the world's — a scanner writes the page one way up
//! and `/Rotate` the other, and "put this on Letter" is what a person does with the
//! result.

use bytes::Bytes;
use fepdf::{ContentScale, Operation, PageResize, PageSelection, PdfDocument};

const A4: (f64, f64) = (595.0, 842.0);

/// A one-page document, built from the object bodies given.
fn document(bodies: &[String]) -> Bytes {
    use std::fmt::Write as _;
    let mut out = String::from("%PDF-2.0\n");
    let mut offsets = Vec::with_capacity(bodies.len());
    for (n, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{body}\nendobj\n", n + 1);
    }
    let start_xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", bodies.len() + 1);
    for offset in &offsets {
        let _ = writeln!(out, "{offset:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start_xref}\n%%EOF\n",
        bodies.len() + 1
    );
    Bytes::from(out.into_bytes())
}

/// A portrait page box under `/Rotate`, which is a landscape page to anyone looking.
fn turned_page(rotate: i64) -> Bytes {
    document(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Rotate {rotate} \
             /Contents 4 0 R >>"
        ),
        "<< /Length 44 >>\nstream\n0 0 1 rg 100 100 200 300 re f\nendstream".to_string(),
    ])
}

/// **A sheet is asked for as it is seen.**
///
/// The page box is 595 by 842 and `/Rotate 90` makes it a landscape page. Asking for A4
/// means A4 as displayed, so the box has to come out turned the other way — writing 595
/// by 842 into it would hand back the landscape page it started as.
#[test]
fn a_rotated_page_is_resized_as_it_is_seen() {
    let mut doc = PdfDocument::open(turned_page(90)).expect("it opens");
    assert_eq!(
        doc.get_page_size(0).map(|(w, h)| (w.round(), h.round())).expect("a size"),
        (842.0, 595.0),
        "the fixture is not a landscape page to begin with"
    );

    doc.apply(Operation::ResizePages(
        PageSelection::All,
        PageResize { sheet: Some(A4), scale: ContentScale::Fit, offset: (0.0, 0.0) },
    ))
    .expect("it resizes");

    let (w, h) = doc.get_page_size(0).expect("a size");
    assert!((w - A4.0).abs() < 0.5 && (h - A4.1).abs() < 0.5, "it came out {w} by {h}");
}

/// A page with no rotation is unaffected by the rule that handles one.
#[test]
fn an_unrotated_page_is_resized_the_same_way_it_always_was() {
    let mut doc = PdfDocument::open(turned_page(0)).expect("it opens");
    doc.apply(Operation::ResizePages(
        PageSelection::All,
        PageResize { sheet: Some(A4), scale: ContentScale::Fit, offset: (0.0, 0.0) },
    ))
    .expect("it resizes");
    let (w, h) = doc.get_page_size(0).expect("a size");
    assert!((w - A4.0).abs() < 0.5 && (h - A4.1).abs() < 0.5, "it came out {w} by {h}");
}

/// Half a turn does not swap the axes, so the sheet is written as it is asked for.
#[test]
fn half_a_turn_leaves_the_axes_alone() {
    let mut doc = PdfDocument::open(turned_page(180)).expect("it opens");
    doc.apply(Operation::ResizePages(
        PageSelection::All,
        PageResize { sheet: Some(A4), scale: ContentScale::Fit, offset: (0.0, 0.0) },
    ))
    .expect("it resizes");
    let (w, h) = doc.get_page_size(0).expect("a size");
    assert!((w - A4.0).abs() < 0.5 && (h - A4.1).abs() < 0.5, "it came out {w} by {h}");
}

/// A page whose only content is one image drawn to fill it, which is what a scan is.
///
/// The image is 2 by 2 pixels of raw RGB — small enough to write out here, and the only
/// thing that matters about it is that a `Do` is what puts it on the page.
fn scanned_page() -> Bytes {
    // Red, green, blue, white: four quarters, so which way up it lands is visible.
    let pixels: [u8; 12] = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    let raw: String = pixels.iter().map(|b| *b as char).collect();
    let draw = "q 595 0 0 842 0 0 cm /Im0 Do Q";
    document(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R \
          /Resources << /XObject << /Im0 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{draw}\nendstream", draw.len()),
        format!(
            "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Length 12 >>\nstream\n{raw}\nendstream"
        ),
    ])
}

/// **An image scales with everything else, because the wrap does not read the stream.**
///
/// A scanned page is one `Do` inside a content stream, and the `q <cm> … Q` this operation
/// puts around it transforms that `Do` exactly as it transforms a line of text. There is
/// nothing image-shaped to handle — which is worth a test rather than an assumption,
/// since "it should just work" is how a whole class of document goes unchecked.
#[test]
fn a_page_that_is_one_image_resizes_like_any_other() {
    let mut doc = PdfDocument::open(scanned_page()).expect("it opens");
    assert!(
        doc.extract_text(0).expect("it reads").trim().is_empty(),
        "the fixture has text on it, so it is not standing for a scan"
    );

    doc.apply(Operation::ResizePages(
        PageSelection::All,
        PageResize { sheet: Some((842.0, 1191.0)), scale: ContentScale::Fit, offset: (0.0, 0.0) },
    ))
    .expect("it resizes");

    let (w, h) = doc.get_page_size(0).expect("a size");
    assert!((w - 842.0).abs() < 0.5 && (h - 1191.0).abs() < 0.5, "it came out {w} by {h}");

    // It survives a write and a read with nothing to repair — an image stream is the part
    // most likely to be broken by a change to the page around it.
    let dir = std::env::temp_dir().join("fepdf-scan-resize");
    std::fs::create_dir_all(&dir).expect("a directory");
    let path = dir.join("scan.pdf");
    let _ = doc.save_as_version(&path, "2.0").expect("it writes");
    let back = PdfDocument::open(std::fs::read(&path).expect("on disk").into()).expect("it opens");
    let decisions = back.decisions();
    let size = back.get_page_size(0).expect("a size");
    let _ = std::fs::remove_dir_all(&dir);

    assert!(decisions.is_empty(), "the written file needed repairing: {decisions:?}");
    assert!((size.0 - 842.0).abs() < 0.5 && (size.1 - 1191.0).abs() < 0.5, "{size:?}");
}
