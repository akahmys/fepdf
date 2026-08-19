//! Selecting the font through an `ExtGState` rather than `Tf` (Table 57, 8.4.5).
//!
//! `/Font` is `[font size]`, where the font is an **indirect reference to a font
//! dictionary** and not a resource name — so it cannot be held in the field `Tf` fills,
//! and the interpreter ignored the entry entirely. A page that selected its font this way
//! had none at all: `show_text` failed, and because a failing operator aborts the content
//! stream, everything after it was lost too.
//!
//! `NegativeFontSize.pdf` from `pdf-association/pdf-differences` is the file that found
//! it. Six of its twelve runs use `gs`, and they come after a `Q` that pops the state the
//! earlier `Tf` set — so the engine reached them with no font. PDFKit read 327 characters
//! from that page and this engine read none.

use fepdf::{IngestionOptions, PdfDocument};

/// One page, one text run, and the font selected however the caller asks.
fn page_selecting_its_font(resources: &str, content: &str) -> Vec<u8> {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources {resources} \
             /Contents 4 0 R >>"
        ),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
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

fn text_of(bytes: Vec<u8>) -> String {
    let document = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the file opens");
    document.extract_text(0).expect("the page extracts")
}

/// A page whose only font selection is an `ExtGState` still shows its text.
#[test]
fn a_font_selected_by_an_extgstate_is_the_font_in_force() {
    let text = text_of(page_selecting_its_font(
        "<< /Font << /F1 5 0 R >> /ExtGState << /G1 << /Type /ExtGState /Font [5 0 R 24] >> >> >>",
        "BT /G1 gs 1 0 0 1 20 100 Tm (SELECTED BY GS) Tj ET",
    ));
    assert!(text.contains("SELECTED BY GS"), "the ExtGState font was ignored: {text:?}");
}

/// `gs` outside the text object works too: it changes the *graphics* state, which is why
/// the reference lives there rather than beside the interpreter — `q` and `Q` have to
/// save and restore it.
#[test]
fn the_selection_holds_from_outside_the_text_object() {
    let text = text_of(page_selecting_its_font(
        "<< /ExtGState << /G1 << /Type /ExtGState /Font [5 0 R 24] >> >> >>",
        "q /G1 gs BT 1 0 0 1 20 100 Tm (OUTSIDE BT) Tj ET Q",
    ));
    assert!(text.contains("OUTSIDE BT"), "{text:?}");
}

/// Both orders of `Tf` and `gs` leave a usable font, and neither clears the other into
/// nothing.
///
/// It does **not** check which of the two wins, and it is named for what it checks
/// because an earlier version was named for what it does not. Removing the line in `Tf`
/// that clears the `ExtGState` selection — so `gs` would always win — leaves this passing,
/// because both selections here resolve to the same font object and `extract_text`
/// returns text without the size or the font identity. Precedence is real (the two carry
/// sizes 24 and 8) and is not observable through this surface; it would need a rendering
/// check, which is a different harness.
#[test]
fn both_orders_of_tf_and_gs_leave_a_usable_font() {
    let resources =
        "<< /Font << /F1 5 0 R >> /ExtGState << /G1 << /Type /ExtGState /Font [5 0 R 8] >> >> >>";

    let gs_last = text_of(page_selecting_its_font(
        resources,
        "BT /F1 24 Tf /G1 gs 1 0 0 1 20 100 Tm (GS AFTER TF) Tj ET",
    ));
    assert!(gs_last.contains("GS AFTER TF"), "{gs_last:?}");

    let tf_last = text_of(page_selecting_its_font(
        resources,
        "BT /G1 gs /F1 24 Tf 1 0 0 1 20 100 Tm (TF AFTER GS) Tj ET",
    ));
    assert!(tf_last.contains("TF AFTER GS"), "{tf_last:?}");
}
