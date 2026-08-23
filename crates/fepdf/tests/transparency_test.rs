//! What clause 11 does, measured rather than cited (ISO 32000-2, 11.3 to 11.7).
//!
//! `ROADMAP.md` carried this clause as "blend modes, constant alpha and soft masks reach
//! the backend, and the transparency-group clauses are cited in the code", with a note
//! that it was **not re-measured** and that the row was "a statement about citations, not
//! about behaviour". Measured, three of those four are one thing and the fourth is
//! another: `/ca`, `/CA` and `/BM` do reach the backend, and a soft mask was read into the
//! interpreter's state and used by nothing.
//!
//! The backend records rather than rasterises, so this runs without a GPU: the question is
//! whether the interpreter *called* it.

use fepdf::PdfDocument;
use fepdf_content::{
    BlendMode, Color, FallbackFontType, PixelFormat, RenderBackend, SMaskData, StrokeStyle,
    TextGlyph, TextState, WindingRule,
};
use fepdf_model::graphics::TextRenderingMode;
use fepdf_model::interpretation::Severity;
use kurbo::{Affine, BezPath, Shape};
use std::sync::Arc;

/// What the interpreter asked the backend to do, in the order it asked.
#[derive(Default)]
struct Recorder {
    fills: Vec<(f64, f64)>,
    fill_alpha: Vec<f64>,
    stroke_alpha: Vec<f64>,
    blend: Vec<BlendMode>,
}

impl RenderBackend for Recorder {
    fn fill_path(&mut self, path: &BezPath, _color: &Color, _rule: WindingRule) {
        let bounds = path.bounding_box();
        self.fills.push((bounds.x0, bounds.y0));
    }
    fn set_fill_alpha(&mut self, alpha: f64) {
        self.fill_alpha.push(alpha);
    }
    fn set_stroke_alpha(&mut self, alpha: f64) {
        self.stroke_alpha.push(alpha);
    }
    fn set_blend_mode(&mut self, mode: BlendMode) {
        self.blend.push(mode);
    }
    fn transform(&mut self, _t: Affine) {}
    fn set_transform(&mut self, _t: Affine) {}
    fn push_state(&mut self) {}
    fn pop_state(&mut self) {}
    fn stroke_path(&mut self, _p: &BezPath, _c: &Color, _s: &StrokeStyle) {}
    fn set_fill_color(&mut self, _c: Color) {}
    fn set_stroke_color(&mut self, _c: Color) {}
    fn draw_image(&mut self, _d: &[u8], _w: u32, _h: u32, _f: PixelFormat, _s: Option<SMaskData>) {}
    fn push_clip(&mut self, _p: &BezPath, _r: WindingRule) {}
    fn pop_clip(&mut self) {}
    fn show_text(&mut self, _g: &[TextGlyph], _s: f64, _t: Affine, _ts: TextState, _o: usize) {}
    #[allow(clippy::too_many_arguments)]
    fn define_font(
        &mut self,
        _n: &str,
        _b: Option<&str>,
        _d: Option<Arc<Vec<u8>>>,
        _i: Option<usize>,
        _m: Option<std::collections::BTreeMap<u32, u32>>,
        _f: FallbackFontType,
        _c: bool,
    ) {
    }
    fn set_font(&mut self, _n: &str) {}
    fn set_text_render_mode(&mut self, _m: TextRenderingMode) {}
    fn set_char_spacing(&mut self, _s: f64) {}
    fn set_word_spacing(&mut self, _s: f64) {}
}

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

/// One page holding all four questions at once, so an assertion about any of them also
/// says what the others did.
///
/// The mask group paints solid black over the whole page. 11.6.5.2 derives a luminosity
/// mask from that group's luminance, so the mask is **0 everywhere** and the content it
/// covers contributes nothing — a conforming renderer draws no top-left square.
fn document(group_entry: &str) -> PdfDocument {
    let mask_content = "0 0 0 rg 0 0 200 200 re f\n";
    let form_content = "0 0 0 rg 0 0 60 60 re f\n";
    let content = "q /GS0 gs 0 0 0 rg 0 100 100 100 re f Q\n\
                   q /GA gs 0 0 0 rg 100 100 100 100 re f Q\n\
                   q /GB gs 0 0 0 rg 100 0 100 100 re f Q\n\
                   q 1 0 0 1 140 140 cm /Fx Do Q\n\
                   0 0 0 rg 0 0 100 100 re f\n";
    let bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
         /Resources << /ExtGState << /GS0 5 0 R /GA 7 0 R /GB 8 0 R >> \
         /XObject << /Fx 9 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /ExtGState /SMask << /S /Luminosity /G 6 0 R >> >>".to_string(),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 200 200] \
             /Group << /S /Transparency /CS /DeviceGray >> /Length {} >>\n\
             stream\n{mask_content}endstream",
            mask_content.len()
        ),
        "<< /Type /ExtGState /ca 0.5 /CA 0.25 >>".to_string(),
        "<< /Type /ExtGState /BM /Multiply >>".to_string(),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 60 60] {group_entry} /Length {} >>\n\
             stream\n{form_content}endstream",
            form_content.len()
        ),
    ];
    PdfDocument::open(assemble(&bodies).into()).expect("the fixture opens")
}

fn draw(document: &PdfDocument) -> Recorder {
    let mut recorder = Recorder::default();
    document.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder
}

/// Decisions on one clause, after a page has been drawn.
fn decisions_on(document: &PdfDocument, clause: &str) -> Vec<String> {
    document
        .decisions()
        .into_iter()
        .filter(|d| d.clause == clause)
        .map(|d| format!("[{:?}] {} -> {}", d.severity, d.found, d.action))
        .collect()
}

#[test]
fn constant_alpha_and_blend_mode_reach_the_backend() {
    // The half of the row that was true, pinned so that "not re-measured" cannot happen
    // to it twice.
    let recorder = draw(&document(""));
    assert_eq!(recorder.fill_alpha, vec![0.5], "/ca");
    assert_eq!(recorder.stroke_alpha, vec![0.25], "/CA");
    assert_eq!(recorder.blend, vec![BlendMode::Multiply], "/BM");
}

#[test]
fn a_soft_mask_does_not_mask_and_says_so() {
    // A mask that is 0 everywhere must leave the top-left square undrawn. It is drawn.
    // The assertion is on the *silence*, not on the masking: this pins the defect and the
    // record of it, so that implementing 11.6.5.2 has to change this test on purpose.
    let doc = document("");
    let recorder = draw(&doc);
    assert!(
        recorder.fills.contains(&(0.0, 100.0)),
        "measured: the masked square is painted at full strength — {:?}",
        recorder.fills
    );

    let recorded = decisions_on(&doc, "11.6.5.2");
    assert_eq!(recorded.len(), 1, "exactly one decision, not one per fill: {recorded:?}");
    assert!(recorded[0].contains("Violation"), "{recorded:?}");
    assert!(recorded[0].contains("unmasked"), "it says what the caller gets: {recorded:?}");
}

#[test]
fn isolation_and_knockout_change_nothing_and_are_recorded() {
    // Not "the group is mishandled" — the entry is not read at all. The two runs produce
    // the same backend calls in the same order, which is the strongest form this can take.
    let plain = document("");
    let asked = document("/Group << /S /Transparency /I true /K true >>");
    assert_eq!(draw(&plain).fills, draw(&asked).fills, "the entry changes nothing drawn");

    let recorded = decisions_on(&asked, "11.6.6");
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    assert!(recorded[0].contains("isolated and knockout"), "{recorded:?}");
}

#[test]
fn a_group_that_asks_for_neither_is_not_recorded() {
    // Illustrator and InDesign wrap almost every form in a plain transparency group. A
    // decision on each would put one on most pages of most files and say nothing by
    // saying it everywhere; what changes the result is /I and /K.
    let plain = document("/Group << /S /Transparency /CS /DeviceRGB >>");
    assert!(decisions_on(&plain, "11.6.6").is_empty(), "a plain group is not a departure");
    assert_eq!(draw(&plain).fills.len(), 5, "and it still draws: four squares and the form's own");
}

#[test]
fn the_severity_says_which_kind_of_departure_each_is() {
    let doc = document("/Group << /S /Transparency /I true >>");
    let _ = draw(&doc);
    for decision in doc.decisions() {
        if decision.clause == "11.6.5.2" || decision.clause == "11.6.6" {
            assert_eq!(
                decision.severity,
                Severity::Violation,
                "content is lost, not merely read one of several ways: {decision:?}"
            );
        }
    }
}
