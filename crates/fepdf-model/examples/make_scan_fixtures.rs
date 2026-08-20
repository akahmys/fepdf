//! Writes the scanned-page fixtures this project has no sample of.
//!
//! Neither corpus holds a scan. `JBIG2Decode` occurs in none of the 251 files and
//! `CCITTFaxDecode` in two, so the codecs Phase M builds have almost nothing to be
//! checked against — and a codec checked only against a fixture its own author wrote is
//! checked against an opinion.
//!
//! These are one half of the answer. The images are encoded by implementations that are
//! **not** the decoders under test: `fax` for Group 4, and JBIG2 segments assembled by
//! hand from T.88 §7.2. The other half is `scripts/test/crosscheck_image.sh`, which asks
//! PDFKit what it sees in the same files — the standard the other five cross-checks
//! hold to, and the only one available while no real scan exists.
//!
//! ```text
//! cargo run --example make_scan_fixtures -p fepdf-model
//! ```
//!
//! Lands in `target/scans/`, beside the other generated corpora and out of `samples/`,
//! whose count several measurements quote.

use std::fmt::Write as _;

/// The image every fixture carries: a black square in the top-left quarter, which is
/// asymmetric in both directions so a flip, a transpose or an inversion all show.
const COLUMNS: u16 = 256;
const ROWS: u16 = 128;

fn black_at(x: u16, y: u16) -> bool {
    x < COLUMNS / 2 && y < ROWS / 2
}

fn main() -> std::io::Result<()> {
    std::fs::create_dir_all("target/scans")?;

    let ccitt = page_with_image("CCITTFaxDecode", "<< /K -1 /Columns 256 /Rows 128 >>", &group4());
    std::fs::write("target/scans/ccitt.pdf", &ccitt)?;
    println!("target/scans/ccitt.pdf     {} bytes", ccitt.len());

    let jbig2 = page_with_image("JBIG2Decode", "<< >>", &jbig2_page());
    std::fs::write("target/scans/jbig2.pdf", &jbig2)?;
    println!("target/scans/jbig2.pdf     {} bytes", jbig2.len());

    println!(
        "\n  {COLUMNS}×{ROWS}, black in the top-left quarter.\n  \
         Compare with: ./scripts/test/crosscheck_image.sh"
    );
    Ok(())
}

/// The image as Group 4, encoded by `fax` — not by the decoder under test.
fn group4() -> Vec<u8> {
    use fax::Color;
    let mut encoder = fax::encoder::Encoder::new(fax::VecWriter::new());
    for y in 0..ROWS {
        let line = (0..COLUMNS).map(|x| if black_at(x, y) { Color::Black } else { Color::White });
        encoder.encode_line(line, COLUMNS).expect("encodes");
    }
    encoder.finish().expect("finishes").finish()
}

/// The image as an embedded JBIG2 page (Annex D.3): a page information segment and one
/// generic region, MMR-coded — which is T.6, the same coding Group 4 uses.
fn jbig2_page() -> Vec<u8> {
    let mut page_info = Vec::new();
    page_info.extend_from_slice(&u32::from(COLUMNS).to_be_bytes());
    page_info.extend_from_slice(&u32::from(ROWS).to_be_bytes());
    page_info.extend_from_slice(&0_u32.to_be_bytes()); // x resolution, unstated
    page_info.extend_from_slice(&0_u32.to_be_bytes()); // y resolution, unstated
    page_info.push(0x01); // lossless, default pixel white
    page_info.extend_from_slice(&0_u16.to_be_bytes()); // not striped

    let mut region = Vec::new();
    region.extend_from_slice(&u32::from(COLUMNS).to_be_bytes());
    region.extend_from_slice(&u32::from(ROWS).to_be_bytes());
    region.extend_from_slice(&0_u32.to_be_bytes()); // at x = 0
    region.extend_from_slice(&0_u32.to_be_bytes()); // at y = 0
    region.push(0x00); // combine by OR
    region.push(0x01); // generic region flags: MMR
    region.extend_from_slice(&group4()); // MMR is T.6

    let mut out = segment(0, 48, &page_info);
    out.extend(segment(1, 38, &region));
    out
}

/// One segment header and its data (T.88 §7.2): number, flags, no referred-to segments,
/// page association, length.
fn segment(number: u32, kind: u8, data: &[u8]) -> Vec<u8> {
    let mut out = number.to_be_bytes().to_vec();
    out.push(kind);
    out.push(0x00);
    out.push(0x01);
    out.extend_from_slice(&u32::try_from(data.len()).expect("fits").to_be_bytes());
    out.extend_from_slice(data);
    out
}

/// A one-page PDF whose page *is* the image, one point per pixel, so a renderer has no
/// scaling to disagree about.
fn page_with_image(filter: &str, parms: &str, data: &[u8]) -> Vec<u8> {
    let content = format!("q {COLUMNS} 0 0 {ROWS} 0 0 cm /Im0 Do Q");
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {COLUMNS} {ROWS}] \
             /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>"
        ),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        format!(
            "<< /Type /XObject /Subtype /Image /Width {COLUMNS} /Height {ROWS} \
             /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /{filter} \
             /DecodeParms {parms} /Length {} >>\nstream\n\u{0}endstream",
            data.len()
        ),
    ];

    let mut out = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        // The image's bytes are binary and go in verbatim; every other body is text.
        if let Some(head) = body.strip_suffix("\u{0}endstream") {
            out.extend_from_slice(format!("{} 0 obj\n{head}", i + 1).as_bytes());
            out.extend_from_slice(data);
            out.extend_from_slice(b"\nendstream\nendobj\n");
        } else {
            out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
        }
    }

    let table_at = out.len();
    let mut trailer = String::new();
    let _ = write!(trailer, "xref\n0 {}\n0000000000 65535 f \n", bodies.len() + 1);
    for offset in &offsets {
        let _ = writeln!(trailer, "{offset:010} 00000 n ");
    }
    let _ = write!(
        trailer,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n",
        bodies.len() + 1
    );
    out.extend_from_slice(trailer.as_bytes());
    out
}
