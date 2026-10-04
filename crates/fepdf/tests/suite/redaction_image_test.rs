//! **What a redaction region lies over in an image is not in the file** (ROADMAP Y-10).
//!
//! A fill drawn over a picture leaves every pixel under it in the stream. Here the image
//! is replaced, for the drawing the region meets, by a copy with those pixels blanked, and
//! the original — drawn nowhere else — is not written.

use fepdf::{Operation, PdfDocument, Redaction};
use fepdf_fixtures::recorder::{Event, Recorder};
use kurbo::Affine;

/// Four columns of red, 10 to 40, over two rows, the second with green in it.
fn pixels() -> Vec<u8> {
    let mut out = Vec::new();
    for green in [0u8, 99] {
        for red in [10u8, 20, 30, 40] {
            out.extend_from_slice(&[red, green, 0]);
        }
    }
    out
}

/// A 200-point page with the image drawn over x 0–200, y 50–150, so each column is 50
/// points wide and each row 50 high; `drawn_twice` draws it again in the top corner.
fn page_with_image(drawn_twice: bool) -> PdfDocument {
    let again = if drawn_twice { " q 20 0 0 10 180 190 cm /Im0 Do Q" } else { "" };
    let content = format!("q 200 0 0 100 0 50 cm /Im0 Do Q{again}");
    let image = pixels();
    let mut stream = format!(
        "<< /Type /XObject /Subtype /Image /Width 4 /Height 2 /ColorSpace /DeviceRGB \
           /BitsPerComponent 8 /Length {} >>\nstream\n",
        image.len()
    )
    .into_bytes();
    stream.extend_from_slice(&image);
    stream.extend_from_slice(b"\nendstream");
    let bytes = fepdf_fixtures::assemble(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
           /Resources << /XObject << /Im0 5 0 R >> >> >>"
            .to_vec(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
        stream,
    ]);
    PdfDocument::open(bytes.into()).expect("the fixture opens")
}

fn redact(doc: &mut PdfDocument, region: (f64, f64, f64, f64)) {
    let redaction = Redaction { page: 0, regions: vec![region], fill: Some(vec![]) };
    doc.apply(Operation::Redact(redaction)).expect("it redacts");
}

/// The samples of every image the page draws, in order.
fn drawn(doc: &PdfDocument) -> Vec<Vec<u8>> {
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page renders");
    recorder
        .events
        .into_iter()
        .filter_map(|e| if let Event::Image { samples, .. } = e { Some(samples) } else { None })
        .collect()
}

/// The samples of every image the saved file holds.
fn written(doc: &PdfDocument, name: &str) -> Vec<Vec<u8>> {
    let path =
        std::env::temp_dir().join(format!("fepdf-redact-image-{name}-{}.pdf", std::process::id()));
    let options = fepdf::SaveOptions { compress: false, ..fepdf::SaveOptions::default() };
    doc.save_with_options(&path, "2.0", &options).expect("it writes");
    let back =
        PdfDocument::open(std::fs::read(&path).expect("it is there").into()).expect("it reads");
    let _ = std::fs::remove_file(&path);
    let arena = back.inner().arena();
    (0..arena.object_count())
        .filter_map(|i| match arena.get_object(arena.handle(i))? {
            fepdf_model::Object::Stream(dict, data)
                if arena.dict_entry(dict, arena.name("Subtype")).and_then(|s| s.as_name())
                    == Some(arena.name("Image")) =>
            {
                arena.get_stream_bytes(&data).ok().map(|b| b.to_vec())
            }
            _ => None,
        })
        .collect()
}

/// **The two left columns are blanked and the two right ones kept**, in what the page
/// draws and in what the file holds; the original is not in the file.
#[test]
fn the_pixels_under_the_region_are_blanked_in_the_file() {
    let mut doc = page_with_image(false);
    let said = doc
        .what_redaction_removes(&Redaction {
            page: 0,
            regions: vec![(0.0, 0.0, 100.0, 200.0)],
            fill: None,
        })
        .expect("it reads");
    assert_eq!(said.images, [(0.0, 50.0, 100.0, 150.0)], "the area said to be blanked");
    redact(&mut doc, (0.0, 0.0, 100.0, 200.0));

    let expected = [0, 0, 0, 0, 0, 0, 30, 0, 0, 40, 0, 0, 0, 0, 0, 0, 0, 0, 30, 99, 0, 40, 99, 0];
    assert_eq!(drawn(&doc), [expected.to_vec()], "what the page draws");
    assert_eq!(written(&doc, "half"), [expected.to_vec()], "the file holds the original as well");
}

/// **A region over part of a pixel takes the pixel.** A point of the region inside the
/// third column is enough for that column to go.
#[test]
fn a_region_over_part_of_a_pixel_takes_it() {
    let mut doc = page_with_image(false);
    redact(&mut doc, (99.0, 120.0, 101.0, 121.0));
    // The region is in the top row, across the second and third columns.
    let expected =
        [10, 0, 0, 0, 0, 0, 0, 0, 0, 40, 0, 0, 10, 99, 0, 20, 99, 0, 30, 99, 0, 40, 99, 0];
    assert_eq!(drawn(&doc), [expected.to_vec()]);
}

/// **An image the page also draws outside the region keeps that drawing whole**: only the
/// drawing the region meets is replaced, and the original stays in the file for the other.
#[test]
fn another_drawing_of_the_same_image_is_left_whole() {
    let mut doc = page_with_image(true);
    redact(&mut doc, (0.0, 0.0, 100.0, 160.0));
    let images = drawn(&doc);
    assert_eq!(images.len(), 2, "{images:?}");
    assert_ne!(images[0], pixels(), "the drawing under the region was not blanked");
    assert_eq!(images[1], pixels(), "the drawing outside the region changed");
}

/// A region that misses the image leaves it, and its stream, as they were.
#[test]
fn a_region_that_misses_the_image_leaves_it() {
    let mut doc = page_with_image(false);
    redact(&mut doc, (0.0, 160.0, 200.0, 200.0));
    assert_eq!(drawn(&doc), [pixels()]);
    assert_eq!(written(&doc, "miss"), [pixels()]);
}

/// A page drawing one image object 5 over the whole of it, with `extra` from object 6.
fn page_drawing_image(image: &[u8], dict: &str, extra: &[Vec<u8>]) -> PdfDocument {
    let content = "q 200 0 0 200 0 0 cm /Im0 Do Q";
    let mut stream =
        format!("<< /Type /XObject /Subtype /Image {dict} /Length {} >>\nstream\n", image.len())
            .into_bytes();
    stream.extend_from_slice(image);
    stream.extend_from_slice(b"\nendstream");
    let mut bodies = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
           /Resources << /XObject << /Im0 5 0 R >> >> >>"
            .to_vec(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
        stream,
    ];
    bodies.extend_from_slice(extra);
    PdfDocument::open(fepdf_fixtures::assemble(&bodies).into()).expect("the fixture opens")
}

/// **A soft mask is blanked under the region too**: it is the picture's shape, and a
/// mask left whole draws the outline of what was removed.
#[test]
fn the_soft_mask_is_blanked_with_the_image() {
    let mask = vec![255u8; 4];
    let mut smask =
        b"<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray \
           /BitsPerComponent 8 /Length 4 >>\nstream\n"
            .to_vec();
    smask.extend_from_slice(&mask);
    smask.extend_from_slice(b"\nendstream");
    let mut doc = page_drawing_image(
        &[200u8; 4],
        "/Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /SMask 6 0 R",
        &[smask],
    );
    redact(&mut doc, (0.0, 100.0, 100.0, 200.0));
    let written = written(&doc, "smask");
    assert!(
        written.contains(&vec![0, 255, 255, 255]),
        "the soft mask kept its top left: {written:?}"
    );
    assert!(!written.contains(&mask), "the original soft mask is in the file: {written:?}");
}

/// **A stencil is blanked to what paints nothing**: 1 under the default `/Decode`, since 0
/// is what paints (8.9.6.2), and 0 under `/Decode [1 0]`.
#[test]
fn a_stencil_is_blanked_to_what_paints_nothing() {
    for (decode, painted, blank) in [("", 0x00u8, 0xc0u8), ("/Decode [1 0]", 0xffu8, 0x3fu8)] {
        let mut doc = page_drawing_image(
            &[painted],
            &format!("/Width 8 /Height 1 /ImageMask true {decode}"),
            &[],
        );
        redact(&mut doc, (0.0, 0.0, 50.0, 200.0));
        let written = written(&doc, "stencil");
        assert_eq!(written, [vec![blank]], "{decode:?}: the first two pixels");
    }
}

/// A page drawing `image`, an inline image's operator, over x 0–200, y 50–150, with
/// `resources` as its resource dictionary.
fn page_with_inline(image: &[u8], resources: &str) -> PdfDocument {
    let mut content = b"q 200 0 0 100 0 50 cm ".to_vec();
    content.extend_from_slice(image);
    content.extend_from_slice(b" Q");
    let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream.extend_from_slice(&content);
    stream.extend_from_slice(b"\nendstream");
    let bytes = fepdf_fixtures::assemble(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
               /Resources {resources} >>"
        )
        .into_bytes(),
        stream,
    ]);
    PdfDocument::open(bytes.into()).expect("the fixture opens")
}

/// The four-by-two picture as an unfiltered inline image.
fn inline_rgb() -> Vec<u8> {
    [b"BI /W 4 /H 2 /CS /RGB /BPC 8 ID ".as_slice(), &pixels(), b" EI"].concat()
}

/// The left two columns blanked, as `the_pixels_under_the_region_are_blanked_in_the_file`
/// has them.
const LEFT_BLANKED: [u8; 24] =
    [0, 0, 0, 0, 0, 0, 30, 0, 0, 40, 0, 0, 0, 0, 0, 0, 0, 0, 30, 99, 0, 40, 99, 0];

/// **An inline image is blanked like an image object**: it is lifted into one, and the
/// pixels under the region are not in the file.
#[test]
fn an_inline_image_is_blanked_under_the_region() {
    let mut doc = page_with_inline(&inline_rgb(), "<< >>");
    let said = doc
        .what_redaction_removes(&Redaction {
            page: 0,
            regions: vec![(0.0, 0.0, 100.0, 200.0)],
            fill: None,
        })
        .expect("it reads");
    assert_eq!(said.images, [(0.0, 50.0, 100.0, 150.0)], "the area said to be blanked");
    redact(&mut doc, (0.0, 0.0, 100.0, 200.0));
    assert_eq!(drawn(&doc), [LEFT_BLANKED.to_vec()]);
    assert_eq!(written(&doc, "inline"), [LEFT_BLANKED.to_vec()]);
}

/// **Samples that happen to read ` EI ` do not end the image**: an unfiltered one is as
/// long as its size says, and is lifted whole.
#[test]
fn samples_reading_ei_do_not_end_an_inline_image() {
    let samples =
        [32u8, 69, 73, 32, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];
    let image = [b"BI /W 4 /H 2 /CS /RGB /BPC 8 ID ".as_slice(), &samples, b" EI"].concat();
    let mut doc = page_with_inline(&image, "<< >>");
    redact(&mut doc, (0.0, 160.0, 10.0, 170.0));
    assert_eq!(drawn(&doc), [samples.to_vec()], "the image was cut at the EI in its samples");
}

/// A filtered inline image is lifted with its filter spelled out, and blanked.
#[test]
fn a_filtered_inline_image_is_blanked() {
    let hex = hex_of(&pixels());
    let image = format!("BI /W 4 /H 2 /CS /RGB /BPC 8 /F /AHx ID {hex}> EI").into_bytes();
    let mut doc = page_with_inline(&image, "<< >>");
    redact(&mut doc, (0.0, 0.0, 100.0, 200.0));
    assert_eq!(drawn(&doc), [LEFT_BLANKED.to_vec()]);
}

/// **A colour space named from the resources is taken from them**: an image object names
/// a space, not a resource.
#[test]
fn an_inline_images_named_space_is_resolved() {
    let image = [b"BI /W 4 /H 2 /CS /CS0 /BPC 8 ID ".as_slice(), &pixels(), b" EI"].concat();
    let mut doc = page_with_inline(&image, "<< /ColorSpace << /CS0 /DeviceRGB >> >>");
    redact(&mut doc, (0.0, 0.0, 100.0, 200.0));
    assert_eq!(drawn(&doc), [LEFT_BLANKED.to_vec()]);
}

/// **An inline image the region misses is still lifted, and with whole names**:
/// abbreviations (Table 92) and a resource's name are an inline image's alone, and an
/// image object carrying them names a filter and a space no reader knows.
#[test]
fn a_lifted_image_carries_whole_names() {
    let hex = hex_of(&pixels());
    let image = format!("BI /W 4 /H 2 /CS /CS0 /BPC 8 /F /AHx ID {hex}> EI").into_bytes();
    let mut doc = page_with_inline(&image, "<< /ColorSpace << /CS0 /DeviceRGB >> >>");
    redact(&mut doc, (0.0, 160.0, 10.0, 170.0));
    assert_eq!(drawn(&doc), [pixels()], "the lifted image draws as the inline one did");

    let arena = doc.inner().arena();
    let name_of = |dict, key: &str| {
        arena
            .dict_entry(dict, arena.name(key))
            .and_then(|v| v.as_name())
            .and_then(|n| arena.get_name_str(n))
    };
    let lifted: Vec<_> = (0..arena.object_count())
        .filter_map(|i| match arena.get_object(arena.handle(i))? {
            fepdf_model::Object::Stream(dict, _)
                if name_of(dict, "Subtype").as_deref() == Some("Image") =>
            {
                Some((name_of(dict, "Filter"), name_of(dict, "ColorSpace")))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        lifted,
        [(Some("ASCIIHexDecode".to_string()), Some("DeviceRGB".to_string()))],
        "the lifted image's filter and space"
    );
}

/// `bytes` as hexadecimal digits, as `/ASCIIHexDecode` reads them.
fn hex_of(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02X}");
        out
    })
}
