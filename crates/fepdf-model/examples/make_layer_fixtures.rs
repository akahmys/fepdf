//! Writes the optional-content fixtures a second renderer can be asked about.
//!
//! Clause 8.11 has more constructions than any corpus this project can reach presents —
//! one file of the 251 carries `/OCProperties` at all, and its `/OCGs` array is empty. So
//! the semantics are held by `crates/fepdf/tests/optional_content_test.rs`, which asks
//! whether the interpreter *called* the backend, and these three files are the part that
//! can be checked against something this project did not write.
//!
//! **Three, and not the thirteen the test covers.** PDFKit was asked all thirteen
//! (`docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md`
//! records the results): it honours a group listed in `/OFF` and a `/BaseState /OFF`, and
//! paints the other eleven — an `/OC` on an XObject, every OCMD policy, a `/VE`
//! expression, a `/Usage` applied through `/AS`, and a section nested inside a hidden one.
//! Putting those in here would make `crosscheck_image.sh` red against a defect that is
//! not this engine's, and moving this engine to agree with them would be following an
//! implementation over the standard. They are covered by the test instead.
//!
//! The third file is the control: the same page with the group **on**, which must be
//! painted. Without it, "hides the layer" and "draws nothing" produce the same four
//! numbers.
//!
//! ```text
//! cargo run --example make_layer_fixtures -p fepdf-model
//! ```
//!
//! Lands in `target/layers/`, out of `samples/`, whose count several measurements quote.

/// The square under test, in the top-left quarter of the page.
const CONDITIONAL: &str = "0 0 0 rg 0 100 100 100 re f\n";
/// The square that carries no condition, in the bottom-right. Both renderers must paint
/// it in every fixture, so a page that came out blank is told apart from one that hid the
/// right quarter.
const UNCONDITIONAL: &str = "0 0 0 rg 100 0 100 100 re f\n";

fn main() -> std::io::Result<()> {
    std::fs::create_dir_all("target/layers")?;

    let fixtures = [
        (
            "layer_off",
            "<< /OCGs [5 0 R] /D << /BaseState /ON /OFF [5 0 R] >> >>",
            "the group is listed in the default configuration's /OFF",
        ),
        (
            "layer_basestate_off",
            "<< /OCGs [5 0 R] /D << /BaseState /OFF >> >>",
            "/BaseState turns every declared group off",
        ),
        (
            "layer_on",
            "<< /OCGs [5 0 R] /D << /BaseState /ON /ON [5 0 R] >> >>",
            "the control: the group is on, so the square is painted",
        ),
    ];

    for (name, properties, what) in fixtures {
        let file = page_with_layer(properties);
        let path = format!("target/layers/{name}.pdf");
        std::fs::write(&path, &file)?;
        println!("{path:<38} {:>5} bytes  — {what}", file.len());
    }

    println!(
        "\n  200×200, black in the top-left quarter under a /OC section and in the\n  \
         bottom-right unconditionally.\n  \
         Compare with: ./scripts/test/crosscheck_image.sh"
    );
    Ok(())
}

/// A one-page file whose top-left square sits inside `/OC /MC0 BDC … EMC`.
fn page_with_layer(properties: &str) -> Vec<u8> {
    let content = format!("/OC /MC0 BDC\n{CONDITIONAL}EMC\n{UNCONDITIONAL}");
    let bodies = [
        format!("<< /Type /Catalog /Pages 2 0 R /OCProperties {properties} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
         /Resources << /Properties << /MC0 5 0 R >> >> /Contents 4 0 R >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /OCG /Name (Conditional) >>".to_string(),
    ];
    assemble(&bodies)
}

/// Object bodies numbered from 1, with a cross-reference table over them.
fn assemble(bodies: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let table_at = out.len();
    let size = bodies.len() + 1;
    out.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n")
            .as_bytes(),
    );
    out
}
