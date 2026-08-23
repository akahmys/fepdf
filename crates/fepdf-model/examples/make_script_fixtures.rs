//! Forms that calculate, which no file in either corpus does.
//!
//! **A thin corpus is a requirement on the builder, not a verdict.** Across 524 files
//! `/AA /C` — a field calculation, the thing form scripting exists for — occurs **zero**
//! times, and `/CO` occurs in four, all of them conformance-failure fixtures. So the
//! subset ADR-0026 takes cannot be validated against what exists, and these are the third
//! sibling of `make_scan_fixtures.rs` and `make_colour_fixtures.rs`.
//!
//! Each file is a form whose `total` field is computed from the others by an `/AA /C`
//! action, with `/CO` naming the order. Before scripts run, `SetFormFieldValue` records a
//! `Violation` of 12.6.3 — "wrote the value and did not run the scripts; fields computed
//! from it are now stale". That `Decision` is the measurement these fixtures exist to
//! move: when a run happens, it should stop appearing.
//!
//! ```text
//! cargo run --example make_script_fixtures -p fepdf-model
//! ```

use std::fmt::Write as _;

fn main() -> std::io::Result<()> {
    std::fs::create_dir_all("target/scripts")?;

    // The ordinary case: one field computed from two others.
    write(
        "sum",
        &[
            field("a", "2", None),
            field("b", "3", None),
            field(
                "total",
                "0",
                Some("event.value = this.getField(\"a\").value + this.getField(\"b\").value;"),
            ),
        ],
        &["total"],
        "a total computed from two inputs",
    )?;

    // A chain: `total` depends on `subtotal`, which depends on `a`. `/CO` is what says
    // which order to run them in, and getting it wrong gives a stale `total` rather than
    // an error — which is why the order is in the file rather than inferred.
    write(
        "chain",
        &[
            field("a", "5", None),
            field("subtotal", "0", Some("event.value = this.getField(\"a\").value * 2;")),
            field("total", "0", Some("event.value = this.getField(\"subtotal\").value + 1;")),
        ],
        &["subtotal", "total"],
        "a two-step chain whose order /CO decides",
    )?;

    // 12.6.3 permits A -> B -> A. The guard cannot be "do not calculate a field twice";
    // it has to be a bounded iteration count that records a `Decision` when it stops.
    write(
        "cycle",
        &[
            field("a", "1", Some("event.value = this.getField(\"b\").value + 1;")),
            field("b", "1", Some("event.value = this.getField(\"a\").value + 1;")),
        ],
        &["a", "b"],
        "a calculation that refers to itself, which 12.6.3 permits",
    )?;

    println!("\n  These carry what the corpora do not: /AA /C on a field and /CO on the form.");
    Ok(())
}

/// One text field with an optional `/AA /C` calculate action, as objects from 5.
fn field(name: &str, value: &str, calculate: Option<&str>) -> String {
    let actions = calculate.map_or_else(String::new, |js| {
        format!("/AA << /C << /S /JavaScript /JS ({}) >> >>", escape(js))
    });
    format!(
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) /V ({value}) \
         /Rect [0 0 100 20] /F 4 /DA (/Helv 9 Tf 0 g) {actions} >>"
    )
}

/// `(` and `)` end a literal string, so a script containing them must escape them
/// (7.3.4.2). Every calculation here contains both.
fn escape(text: &str) -> String {
    text.replace('\\', r"\\").replace('(', r"\(").replace(')', r"\)")
}

fn write(stem: &str, fields: &[String], order: &[&str], what: &str) -> std::io::Result<()> {
    let refs: Vec<String> = (0..fields.len()).map(|i| format!("{} 0 R", i + 5)).collect();
    // `/CO` names the fields by reference, in the order they are to be calculated.
    let co: Vec<String> = order
        .iter()
        .filter_map(|name| fields.iter().position(|f| f.contains(&format!("/T ({name})"))))
        .map(|i| format!("{} 0 R", i + 5))
        .collect();

    let mut bodies = vec![
        format!(
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [{}] /CO [{}] \
             /DA (/Helv 9 Tf 0 g) >> >>",
            refs.join(" "),
            co.join(" ")
        ),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [{}] \
             /Contents 4 0 R >>",
            refs.join(" ")
        ),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
    ];
    bodies.extend_from_slice(fields);

    let mut out = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let table_at = out.len();
    let size = bodies.len() + 1;
    let mut trailer = String::new();
    let _ = write!(trailer, "xref\n0 {size}\n0000000000 65535 f \n");
    for offset in &offsets {
        let _ = writeln!(trailer, "{offset:010} 00000 n ");
    }
    let _ =
        write!(trailer, "trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{table_at}\n%%EOF\n");
    out.extend_from_slice(trailer.as_bytes());

    let path = format!("target/scripts/{stem}.pdf");
    std::fs::write(&path, &out)?;
    println!("{path:<32} {:>5} bytes  — {what}", out.len());
    Ok(())
}
