//! Writes the two files Phase P's numbers come from.
//!
//! **They were red, and they are what the evaluator was built against.** When these were
//! written there was no PDF function evaluator (7.10), so a `/Separation` tint was read
//! as a grey level and a shading read only `/C0` and `/C1`; `crosscheck_image.sh`
//! reported `DISAGREE by 229` on the first and `by 101` on the second. Both defects are
//! fixed and both numbers moved:
//!
//! | | before | after | PDFKit |
//! | :--- | :--- | :--- | :--- |
//! | `separation.pdf` | `254 254 254 254` | `0 254 254 254` | `25 255 255 255` |
//! | `gradient.pdf` | `62 190 62 190` | `111 89 111 89` | `112 89 112 89` |
//!
//! The gradient agrees. The separation is **pinned** in `crosscheck_image.sh` rather than
//! agreeing, and the remaining gap is not this defect: the quadrant went white → black,
//! which is the tint transform running, and the 25 that is left is `/DeviceCMYK` → RGB
//! being the naive formula here and colour managed in PDFKit. That is its own entry in
//! `ROADMAP.md` Phase P and its own defect.
//!
//! They stay here because `ROADMAP.md` quotes their numbers, and a number in this
//! project carries the command that re-derives it:
//!
//! ```text
//! cargo run --example make_colour_fixtures -p fepdf-model
//! ./scripts/test/crosscheck_image.sh
//! ```
//!
//! Both are 200×200 with the paint in the **top-left** quadrant, so the first of the four
//! numbers is the one that moves. Neither needs a font, so nothing about the host can
//! change the answer.

use std::fmt::Write as _;

fn main() -> std::io::Result<()> {
    std::fs::create_dir_all("target/colour")?;

    // A `/Separation` at full tint. The tint transform is a type 2 exponential taking
    // 1.0 to full black in `/DeviceCMYK`; a reader that evaluates it paints black, and
    // one that reads the tint as a grey level paints white — 1.0 being white in
    // `/DeviceGray` and full ink in a separation is the whole of the defect.
    //
    // Two things had to be true to paint it, and only one of them was 7.10: `/Spot` is a
    // *resource name*, and `cs` never looked in `/ColorSpace` for it, so the space was
    // `Unknown` before any function could have been reached. A fixture that exercises
    // both is why this is a named space rather than an inline array.
    let spot = page(
        "/ColorSpace << /Spot 5 0 R >>",
        "/Spot cs 1 scn 0 100 100 100 re f\n",
        &["[/Separation /Black /DeviceCMYK << /FunctionType 2 /Domain [0 1] \
           /C0 [0 0 0 0] /C1 [0 0 0 1] /N 1 >>]"
            .to_string()],
    );
    std::fs::write("target/colour/separation.pdf", &spot)?;
    println!("target/colour/separation.pdf  {} bytes  — a spot colour at full tint", spot.len());

    // A stitching function: red to green to blue across the page. This is how a gradient
    // with more than two stops is written, and a reader that takes `/C0` and `/C1` off
    // the outermost dictionary finds neither and falls back to black-to-white.
    let stitching = "<< /FunctionType 3 /Domain [0 1] /Bounds [0.5] /Encode [0 1 0 1] \
         /Functions [<< /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 1 0] /N 1 >> \
         << /FunctionType 2 /Domain [0 1] /C0 [0 1 0] /C1 [0 0 1] /N 1 >>] >>";
    let shading = format!(
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 200 0] \
         /Extend [true true] /Function {stitching} >>"
    );
    let gradient =
        page(&format!("/Shading << /Sh0 {shading} >>"), "q 0 0 200 200 re W n /Sh0 sh Q\n", &[]);
    std::fs::write("target/colour/gradient.pdf", &gradient)?;
    println!("target/colour/gradient.pdf    {} bytes  — a three-stop gradient", gradient.len());

    println!(
        "\n  The gradient agrees with PDFKit; the separation is pinned, on a \
         /DeviceCMYK\n  conversion that is not colour managed rather than on 7.10.\n  \
         Compare with: ./scripts/test/crosscheck_image.sh"
    );
    Ok(())
}

/// A one-page 200×200 file with `resources` on the page and `extra` as objects from 5.
fn page(resources: &str, content: &str, extra: &[String]) -> Vec<u8> {
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
             /Resources << {resources} >> /Contents 4 0 R >>"
        ),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ];
    bodies.extend_from_slice(extra);

    let mut out = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
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
