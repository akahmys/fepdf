//! A page whose only mark is text in a font the file does not embed.
//!
//! **Nothing in the fixture set could see this, and it was badly broken.** Every other
//! fixture here is deliberately font-free — `make_colour_fixtures` says so in its header,
//! because a fixture whose answer depends on the host is a fixture that fails for reasons
//! that are not defects. The cost of that discipline was a blind spot exactly the size of
//! this file: a standard-14 font rendered **nothing at all**, on every page, and the
//! quadrant comparator never looked at a page that used one.
//!
//! `/Helvetica` with no `/FontFile` is the commonest thing in PDF that is not a rectangle.
//! The model answers such a font with `SYSTEM_FALLBACK_BASE + a character` rather than a
//! glyph index; the renderer did not know that convention and passed the marker to
//! `skrifa` as a literal glyph, which had no outline. It drew blank and said nothing,
//! because the caller discarded the success flag.
//!
//! **This one does depend on the host**, unlike its neighbours. It needs a substitute
//! face to exist, and PDFKit needs one too — the same machine supplies both, which is
//! what makes the comparison fair rather than exact. `crosscheck_image.sh` is macOS-only
//! regardless, so the dependence is bounded to a platform that always has Helvetica.
//!
//! ```text
//! cargo run --example make_font_fixtures -p fepdf-model
//! ./scripts/test/crosscheck_image.sh
//! ```

use std::fmt::Write as _;

fn main() -> std::io::Result<()> {
    std::fs::create_dir_all("target/fonts")?;

    // Four lines of 24pt text filling the top-left quadrant, so the first of the four
    // numbers is the one that moves and the other three stay paper-white.
    let mut content = String::from("BT /F1 24 Tf 0 g\n");
    for (i, line) in ["Hamburg", "efonstiv", "STANDARD", "14 FONT"].iter().enumerate() {
        let y = 170 - i32::try_from(i).unwrap_or(0) * 26;
        let _ = writeln!(content, "1 0 0 1 6 {y} Tm ({line}) Tj");
    }
    content.push_str("ET\n");

    let pdf = page(
        "/Font << /F1 5 0 R >>",
        &content,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    std::fs::write("target/fonts/standard14.pdf", &pdf)?;
    println!("target/fonts/standard14.pdf  {} bytes  — /Helvetica, not embedded", pdf.len());
    println!("\n  Compare with: ./scripts/test/crosscheck_image.sh");
    Ok(())
}

/// A one-page 200×200 file with `resources` on the page and `extra` as object 5.
fn page(resources: &str, content: &str, extra: &str) -> Vec<u8> {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
             /Resources << {resources} >> /Contents 4 0 R >>"
        ),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        extra.to_string(),
    ];

    let mut out = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let table_at = out.len();
    let count = bodies.len() + 1;
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
