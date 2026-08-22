//! Four files, one per mesh shading type (ISO 32000-2, 8.7.4.5.5 to 8.7.4.5.8).
//!
//! Each paints the **top-left quadrant** of a 200×200 page with the same picture: a
//! horizontal ramp from black on the left to white on the right. Four types, one
//! expected answer, so `crosscheck_image.sh` compares like with like and a type that
//! decodes wrongly stands out against the other three rather than against nothing.
//!
//! **The same picture from four different encodings is the point.** Types 4 and 5 give
//! the triangles outright; types 6 and 7 give a bicubic patch that has to be evaluated,
//! and 6 has to have its four interior control points computed before it can be. If the
//! surface maths is wrong the ramp bends or the quadrant moves, and the four numbers say
//! which.
//!
//! Every field is a whole number of bytes by construction — 8-bit flags, 16-bit
//! coordinates, 8-bit components — so these fixtures do **not** exercise the padding rule
//! in 8.7.4.5.5. `mesh_tests.rs` covers that, because a fixture that cannot fail a rule
//! is not evidence about it.
//!
//! ```text
//! cargo run --example make_mesh_fixtures -p fepdf-model
//! ./scripts/test/crosscheck_image.sh
//! ```

use std::fmt::Write as _;

/// The quadrant the mesh covers: x in [0,100], y in [100,200] on a 200×200 page.
const X0: f64 = 0.0;
const X1: f64 = 100.0;
const Y0: f64 = 100.0;
const Y1: f64 = 200.0;

/// `/Decode` maps 16-bit coordinates over the whole page and 8-bit components over [0,1].
const RANGE: f64 = 200.0;

fn main() -> std::io::Result<()> {
    std::fs::create_dir_all("target/mesh")?;
    for (shading_type, extra, data) in [
        (4, String::from("/BitsPerFlag 8"), free_form()),
        (5, String::from("/VerticesPerRow 2"), lattice()),
        (6, String::from("/BitsPerFlag 8"), coons()),
        (7, String::from("/BitsPerFlag 8"), tensor()),
    ] {
        let dict = format!(
            "<< /ShadingType {shading_type} /ColorSpace /DeviceRGB /BitsPerCoordinate 16 \
             /BitsPerComponent 8 {extra} \
             /Decode [0 {RANGE} 0 {RANGE} 0 1 0 1 0 1] /Length {} >>",
            data.len()
        );
        let pdf = page("/Shading << /Sh0 5 0 R >>", b"/Sh0 sh\n", &dict, &data);
        let path = format!("target/mesh/type{shading_type}.pdf");
        std::fs::write(&path, &pdf)?;
        println!("{path}  {} bytes  — shading type {shading_type}", pdf.len());
    }
    println!(
        "\n  All four paint the same top-left ramp, black at the left edge.\n  \
         Compare with: ./scripts/test/crosscheck_image.sh"
    );
    Ok(())
}

/// A 16-bit coordinate, as `/Decode [0 200 …]` encodes it.
fn coord(v: f64, out: &mut Vec<u8>) {
    let raw = ((v / RANGE) * f64::from(u16::MAX)).round().clamp(0.0, f64::from(u16::MAX));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    out.extend_from_slice(&(raw as u16).to_be_bytes());
}

/// A grey level as three 8-bit components: 0.0 is black, 1.0 is white.
fn grey(level: f64, out: &mut Vec<u8>) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = (level.clamp(0.0, 1.0) * 255.0).round() as u8;
    out.extend_from_slice(&[byte, byte, byte]);
}

/// The ramp: black at the left edge of the quadrant, white at the right.
fn ramp_at(x: f64) -> f64 {
    (x - X0) / (X1 - X0)
}

fn vertex(flag: Option<u8>, x: f64, y: f64, out: &mut Vec<u8>) {
    if let Some(f) = flag {
        out.push(f);
    }
    coord(x, out);
    coord(y, out);
    grey(ramp_at(x), out);
}

/// Type 4: three vertices with flag 0 start a triangle, then flag 1 continues on side
/// `vbc` — so the fourth vertex completes the square with `(vb, vc, vd)`.
fn free_form() -> Vec<u8> {
    let mut out = Vec::new();
    vertex(Some(0), X0, Y0, &mut out);
    vertex(Some(0), X1, Y0, &mut out);
    vertex(Some(0), X0, Y1, &mut out);
    vertex(Some(1), X1, Y1, &mut out);
    out
}

/// Type 5: two rows of two, no flags.
fn lattice() -> Vec<u8> {
    let mut out = Vec::new();
    for y in [Y0, Y1] {
        for x in [X0, X1] {
            vertex(None, x, y, &mut out);
        }
    }
    out
}

/// The twelve boundary control points, in the spiral order 8.7.4.5.8 gives:
/// `p00 p01 p02 p03 p13 p23 p33 p32 p31 p30 p20 p10`.
fn boundary() -> Vec<(f64, f64)> {
    let (tx, ty) = ((X1 - X0) / 3.0, (Y1 - Y0) / 3.0);
    vec![
        (X0, Y0),
        (X0, Y0 + ty),
        (X0, Y0 + 2.0 * ty),
        (X0, Y1),
        (X0 + tx, Y1),
        (X0 + 2.0 * tx, Y1),
        (X1, Y1),
        (X1, Y0 + 2.0 * ty),
        (X1, Y0 + ty),
        (X1, Y0),
        (X0 + 2.0 * tx, Y0),
        (X0 + tx, Y0),
    ]
}

/// The corner colours, in the order the corners come: `c1` at `p00`, `c2` at `p03`,
/// `c3` at `p33`, `c4` at `p30`. Black on the left edge, white on the right.
fn corners(out: &mut Vec<u8>) {
    for x in [X0, X0, X1, X1] {
        grey(ramp_at(x), out);
    }
}

fn coons() -> Vec<u8> {
    let mut out = vec![0_u8];
    for (x, y) in boundary() {
        coord(x, &mut out);
        coord(y, &mut out);
    }
    corners(&mut out);
    out
}

/// Type 7 is type 6 plus the four interior points, in stream order `p11 p12 p22 p21`.
/// For a flat square they sit on the thirds, which is exactly what 8.7.4.5.8's equations
/// produce for the same boundary — so types 6 and 7 must render identically here.
fn tensor() -> Vec<u8> {
    let mut out = vec![0_u8];
    let (tx, ty) = ((X1 - X0) / 3.0, (Y1 - Y0) / 3.0);
    let interior = [
        (X0 + tx, Y0 + ty),
        (X0 + tx, Y0 + 2.0 * ty),
        (X0 + 2.0 * tx, Y0 + 2.0 * ty),
        (X0 + 2.0 * tx, Y0 + ty),
    ];
    for (x, y) in boundary().into_iter().chain(interior) {
        coord(x, &mut out);
        coord(y, &mut out);
    }
    corners(&mut out);
    out
}

/// A one-page 200×200 file whose object 5 is a binary stream.
fn page(resources: &str, content: &[u8], stream_dict: &str, stream_data: &[u8]) -> Vec<u8> {
    let heads = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
             /Resources << {resources} >> /Contents 4 0 R >>"
        ),
        format!("<< /Length {} >>", content.len()),
    ];

    let mut out = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in heads.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\n", i + 1).as_bytes());
        if i == 3 {
            out.extend_from_slice(b"stream\n");
            out.extend_from_slice(content);
            out.extend_from_slice(b"endstream\n");
        }
        out.extend_from_slice(b"endobj\n");
    }
    offsets.push(out.len());
    out.extend_from_slice(format!("5 0 obj\n{stream_dict}\nstream\n").as_bytes());
    out.extend_from_slice(stream_data);
    out.extend_from_slice(b"\nendstream\nendobj\n");

    let table_at = out.len();
    let count = offsets.len() + 1;
    let mut trailer = String::new();
    let _ = write!(trailer, "xref\n0 {count}\n0000000000 65535 f \n");
    for offset in &offsets {
        let _ = writeln!(trailer, "{offset:010} 00000 n ");
    }
    let _ =
        write!(trailer, "trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n");
    out.extend_from_slice(trailer.as_bytes());
    out
}
