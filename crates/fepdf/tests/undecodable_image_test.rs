//! An image XObject the engine cannot decode, and the page's text.
//!
//! Every remaining text failure in the external corpus was of this shape: a
//! `/CCITTFaxDecode` or `/JPXDecode` image, and in one file `/XXXDecode` — a filter
//! invented for the test suite, which no codec will ever handle. The roadmap had these
//! filed under "implement the three image codecs", and the measurement says otherwise: an
//! image carries no text, so decoding one produces none. What the failure did was abort
//! the content stream and take the page's real text with it.

use fepdf::{IngestionOptions, PdfDocument};

/// A page that draws an image whose filter cannot be decoded, and then shows text.
fn page_drawing_an_undecodable_image(filter: &str) -> Vec<u8> {
    let content = "q 100 0 0 67 20 120 cm /ImgX Do Q BT /F1 24 Tf 1 0 0 1 20 60 Tm (TEXT AFTER THE IMAGE) Tj ET";
    let image_data = "\x01\x02\x03\x04\x05\x06\x07\x08";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources << /Font << /F1 6 0 R >> \
         /XObject << /ImgX 5 0 R >> >> /Contents 4 0 R >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        format!(
            "<< /Type /XObject /Subtype /Image /Width 100 /Height 67 /ColorSpace /DeviceRGB \
             /BitsPerComponent 8 /Filter /{filter} /Length {} >>\nstream\n{image_data}\nendstream",
            image_data.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let table_at = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", bodies.len() + 1).as_bytes(),
    );
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n",
            bodies.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// The text after the image still comes out, whichever filter the image used.
///
/// `XXXDecode` is in the list deliberately. It is not a filter the standard defines and
/// never will be, so a file using it cannot be fixed by implementing a codec — which is
/// the argument for this change being the right one rather than a stopgap.
#[test]
fn an_image_that_will_not_decode_does_not_take_the_page_with_it() {
    for filter in ["CCITTFaxDecode", "JPXDecode", "JBIG2Decode", "XXXDecode"] {
        let document = PdfDocument::open_with_options(
            page_drawing_an_undecodable_image(filter).into(),
            &IngestionOptions::default(),
        )
        .unwrap_or_else(|e| panic!("{filter}: the file opens: {e:?}"));

        let text = document
            .extract_text(0)
            .unwrap_or_else(|e| panic!("{filter}: the page should still extract: {e:?}"));
        assert!(
            text.contains("TEXT AFTER THE IMAGE"),
            "{filter}: the image took the page's text with it: {text:?}"
        );
    }
}

/// And an image the engine *can* decode is still drawn, so the skip is not a blanket one.
#[test]
fn a_decodable_image_is_not_skipped_along_with_them() {
    let document = PdfDocument::open_with_options(
        page_drawing_an_undecodable_image("FlateDecode").into(),
        &IngestionOptions::default(),
    )
    .expect("the file opens");
    // The eight bytes are not valid Flate either, so this exercises the same path; what
    // matters is that a recognised filter reaches the decoder rather than being refused
    // by name. The text is the observable part.
    let text = document.extract_text(0).expect("extracts");
    assert!(text.contains("TEXT AFTER THE IMAGE"));
}
