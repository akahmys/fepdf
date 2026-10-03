//! **An inline image is written back as the stream wrote it** (8.9.7, ROADMAP Y-F32).
//!
//! The parser kept an inline image's width, its height, a placeholder format and the
//! still-encoded bytes after `ID`, and the serializer wrote those out as `/CS /RGB /BPC 8`
//! with no filter. Measured 2026-10-04: an 8 by 2 one-bit gray image saved as 8 by 2 RGB
//! with two bytes of samples where 48 were needed, so any save of a page drawing an
//! inline image broke it.

use fepdf::{PdfDocument, SaveOptions};

/// The page's content after a save, read as bytes out of the file.
fn saved_with(content: &[u8], name: &str) -> Vec<u8> {
    let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream.extend_from_slice(content);
    stream.extend_from_slice(b"\nendstream");
    let bytes = fepdf_fixtures::assemble(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> \
           /Contents 4 0 R >>"
            .to_vec(),
        stream,
    ]);
    let doc = PdfDocument::open(bytes.into()).expect("the fixture opens");
    let path = std::env::temp_dir().join(format!("fepdf-inline-{name}-{}.pdf", std::process::id()));
    let options = SaveOptions { compress: false, obj_stm: false, ..SaveOptions::default() };
    let _ = doc.save_with_options(&path, "2.0", &options).expect("it saves");
    let file = std::fs::read(&path).expect("it was written");
    let _ = std::fs::remove_file(&path);
    file
}

fn holds(file: &[u8], needle: &[u8]) -> bool {
    file.windows(needle.len()).any(|w| w == needle)
}

/// What the file wrote from `BI` on, for a failure to show.
fn written(file: &[u8]) -> String {
    let at = file.windows(2).position(|w| w == b"BI").unwrap_or(0);
    String::from_utf8_lossy(&file[at..(at + 80).min(file.len())]).to_string()
}

/// A one-bit gray image keeps its colour space, its depth and its samples.
#[test]
fn a_one_bit_gray_image_survives_a_save() {
    let image = b"BI /W 8 /H 2 /CS /G /BPC 1 ID \xf0\x0f EI";
    let file = saved_with(&[b"q 100 0 0 100 50 50 cm ".as_slice(), image, b" Q"].concat(), "gray");
    assert!(holds(&file, image), "the image was rewritten: {:?}", written(&file));
}

/// A filtered image keeps its filter, and its encoded bytes untouched.
#[test]
fn a_filtered_image_keeps_its_filter() {
    let image = b"BI /W 2 /H 1 /CS /RGB /BPC 8 /F /AHx ID 00FF00FF0000> EI";
    let file = saved_with(&[b"q 100 0 0 100 50 50 cm ".as_slice(), image, b" Q"].concat(), "hex");
    assert!(holds(&file, image), "the image was rewritten: {:?}", written(&file));
}
