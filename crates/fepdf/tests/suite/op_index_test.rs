//! An index into bytes the document does not hold.
//!
//! `TextSpan::op_index` names an operator of the page's *bytes*, because the only thing
//! that uses it re-lexes them: redaction scrubs the strings the same operators carry, and
//! tagging attaches an `/MCID` to them. A document held as pre-sublimated commands has no
//! such bytes — which is the default, `active_refinement` being on.
//!
//! It reported **0 for every span there**. Measured on `samples/constitution.pdf`:
//! 1,007 spans carrying 1,007 distinct indices with refinement off, and one distinct index
//! with it on. `cargo run --release --example op_index_probe` re-derives it.
//!
//! **Nothing inside the engine was reading the zeros**, which is why it went unseen: all
//! three consumers — the reading-order tie-break, `op_to_mcid` and the redaction set —
//! reach the spans through `execute_raw`, where the index is real. What was wrong is what
//! the field said to a caller outside it.

use fepdf::{IngestionOptions, PdfDocument};

fn opened(refine: bool) -> PdfDocument {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/constitution.pdf");
    let Ok(bytes) = std::fs::read(&path) else {
        panic!("samples/constitution.pdf is not in this working copy");
    };
    let options = IngestionOptions { active_refinement: refine, ..IngestionOptions::default() };
    PdfDocument::open_with_options(bytes.into(), &options).expect("the sample opens")
}

/// **Read from bytes, every span names the operator that drew it.**
///
/// The indices being *distinct* is a property of this page and not of the field: it is one
/// content stream, and a form XObject drawn on a page counts again from its own bytes, so
/// a page and a form both report operator 4 for their first run. The consumers want that
/// — each re-lexes the stream it is rewriting — and
/// `a_form_is_sublimated_with_the_page_that_draws_it` is what holds the case where the two
/// would meet.
#[test]
fn a_page_read_from_bytes_gives_every_span_its_own_operator() {
    let spans = opened(false).extract_spans(0).expect("it extracts");
    assert!(spans.len() > 100, "the first page draws more than this: {}", spans.len());

    let mut seen: Vec<Option<usize>> = spans.iter().map(|s| s.op_index).collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), spans.len(), "two spans share an operator index");
    assert!(spans.iter().all(|s| s.op_index.is_some()), "a span read from bytes has no index");
}

/// **And read from commands, no span names one — rather than all of them naming zero.**
///
/// `None` is the true answer: there is no operator stream to index into. A caller could
/// not tell the old `0` from an index, and on this page 1,007 of them said it.
#[test]
fn a_page_read_from_commands_names_no_operator() {
    let spans = opened(true).extract_spans(0).expect("it extracts");
    assert!(spans.len() > 100, "the first page draws more than this: {}", spans.len());

    let named: Vec<usize> = spans.iter().filter_map(|s| s.op_index).collect();
    assert!(
        named.is_empty(),
        "{} spans claim an operator of bytes the document does not hold: {:?}",
        named.len(),
        &named[..named.len().min(6)]
    );
}

/// **The two paths see the same page**, so the difference above is the index and not the
/// reading.
#[test]
fn both_paths_read_the_same_text() {
    let from_bytes = opened(false).extract_spans(0).expect("it extracts");
    let from_commands = opened(true).extract_spans(0).expect("it extracts");

    assert_eq!(
        from_bytes.len(),
        from_commands.len(),
        "the two paths found different numbers of spans"
    );
    let text = |spans: &[fepdf::remediation::TextSpan]| {
        spans.iter().map(|s| s.text.clone()).collect::<String>()
    };
    assert_eq!(text(&from_bytes), text(&from_commands), "the two paths read different text");
}

/// **A form is held the same way as the page that draws it**, which is why the index
/// cannot go stale across one.
///
/// `execute_commands` clears the index before every command because a form drawn from
/// there might still be bytes: executing it would set the index, and the rest of the page
/// would carry the form's last operator. Measured 2026-09-24, that cannot happen —
/// refinement sublimates a form XObject along with the page — so the clearing is a guard
/// with nothing behind it, and this is the reason written down. **If a form ever comes
/// through as bytes under a page that did not, this fails and the guard starts earning
/// its place.**
#[test]
fn a_form_is_sublimated_with_the_page_that_draws_it() {
    let page = "BT /F1 12 Tf 1 0 0 1 40 700 Tm (PAGE) Tj ET\nq /Fm0 Do Q\n\
                BT /F1 12 Tf 1 0 0 1 40 600 Tm (AFTER) Tj ET";
    let form = "BT /F1 12 Tf 1 0 0 1 10 10 Tm (FORM) Tj ET";
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> /XObject << /Fm0 6 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{page}\nendstream", page.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /Length {} \
             /Resources << /Font << /F1 5 0 R >> >> >>\nstream\n{form}\nendstream",
            form.len()
        ),
    ];
    let bytes: Vec<u8> = fepdf_fixtures::assemble(&bodies).into_iter().collect();
    let doc = PdfDocument::open_with_options(bytes.into(), &IngestionOptions::default())
        .expect("the fixture opens");

    let spans = doc.extract_spans(0).expect("it extracts");
    assert_eq!(spans.len(), 3, "the page draws two runs and the form one: {spans:?}");
    assert!(
        spans.iter().all(|s| s.op_index.is_none()),
        "a form came through as bytes under a page that did not: {:?}",
        spans.iter().map(|s| (s.text.clone(), s.op_index)).collect::<Vec<_>>()
    );
}
