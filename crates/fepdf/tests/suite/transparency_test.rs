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
use fepdf_content::{BlendMode, Color, SoftMaskKind};
use fepdf_model::interpretation::Severity;
use kurbo::{Affine, Shape};

use fepdf_fixtures::recorder::{Event, Recorder};

use fepdf_fixtures::assemble;

/// The mask bracket and the fills inside it, in order.
///
/// Filtered rather than taken whole: the shared recorder logs every call, and what this
/// file asserts is where the fills sit relative to the bracket — the `q`/`Q` and colour
/// calls between them are another test's subject.
fn bracket(drawn: &Recorder) -> Vec<String> {
    drawn
        .events
        .iter()
        .filter_map(|e| {
            if let Event::Fill { path, .. } = e {
                let b = path.bounding_box();
                return Some(format!("fill({}, {})", b.x0, b.y0));
            }
            matches!(e, Event::BeginMaskedContent | Event::BeginSoftMask(_) | Event::EndSoftMask)
                .then(|| e.name().to_string())
        })
        .collect()
}

/// The corner of each filled box, in the order they were filled.
fn fills(drawn: &Recorder) -> Vec<(f64, f64)> {
    drawn.fills().iter().map(|b| (b.x0, b.y0)).collect()
}

/// Each `/ca` the page set, in order.
fn fill_alpha(drawn: &Recorder) -> Vec<f64> {
    drawn
        .events
        .iter()
        .filter_map(|e| if let Event::FillAlpha(a) = e { Some(*a) } else { None })
        .collect()
}

/// Each `/CA` the page set, in order.
fn stroke_alpha(drawn: &Recorder) -> Vec<f64> {
    drawn
        .events
        .iter()
        .filter_map(|e| if let Event::StrokeAlpha(a) = e { Some(*a) } else { None })
        .collect()
}

/// Each `/BM` the page set, in order.
fn blend(drawn: &Recorder) -> Vec<BlendMode> {
    drawn
        .events
        .iter()
        .filter_map(|e| if let Event::Blend(m) = e { Some(*m) } else { None })
        .collect()
}

/// Every soft mask the page described.
fn masks(drawn: &Recorder) -> Vec<fepdf_content::SoftMaskSpec> {
    drawn
        .events
        .iter()
        .filter_map(|e| if let Event::BeginSoftMask(spec) = e { Some(spec.clone()) } else { None })
        .collect()
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

/// The same page with a different `/ExtGState` at `/GS0`, for the mask entries a plain
/// fixture cannot carry.
fn document_with_mask(gs: &str) -> PdfDocument {
    let base = document("");
    let _ = base;
    let mask_content = "0 0 0 rg 0 0 200 200 re f\n";
    let content = "q /GS0 gs 0 0 0 rg 0 100 100 100 re f Q\n";
    let bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
         /Resources << /ExtGState << /GS0 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        gs.to_string(),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 200 200] \
             /Group << /S /Transparency /CS /DeviceGray >> /Length {} >>\n\
             stream\n{mask_content}endstream",
            mask_content.len()
        ),
    ];
    PdfDocument::open(assemble(&bodies).into()).expect("the fixture opens")
}

fn draw(document: &PdfDocument) -> Recorder {
    let mut recorder = Recorder::new();
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
    assert_eq!(fill_alpha(&recorder), vec![0.5], "/ca");
    assert_eq!(stroke_alpha(&recorder), vec![0.25], "/CA");
    assert_eq!(blend(&recorder), vec![BlendMode::Multiply], "/BM");
}

#[test]
fn a_soft_mask_reaches_the_backend_as_a_bracket() {
    // The interpreter's whole contract for 11.6.5.2: open, draw the content, describe the
    // mask, replay the group that defines it, close. **The content comes before the
    // mask** because a mask modifies marks that have already been made — there is no way
    // to apply one to marks not yet drawn without holding them somewhere first.
    //
    // This used to be nothing at all: `/SMask` was read into `state.smask` and no backend
    // call followed, so a mask that should have hidden the square left it at full
    // strength and the log said nothing.
    let recorder = draw(&document(""));
    let inside: Vec<String> = bracket(&recorder)
        .into_iter()
        .skip_while(|e| e != "begin_masked_content")
        .take_while(|e| e != "end_soft_mask")
        .collect();
    assert_eq!(
        inside,
        vec!["begin_masked_content", "fill(0, 100)", "begin_soft_mask", "fill(0, 0)"],
        "content, then the mask that covers it: {:?}",
        bracket(&recorder)
    );
    assert!(recorder.count("end_soft_mask") > 0, "{:?}", bracket(&recorder));
}

#[test]
fn the_spec_says_how_the_group_becomes_an_alpha() {
    // One concept and not four: `/S`, `/BC` and `/TR` are three ways of saying how the
    // group's drawing turns into a number, so they travel together and the backend
    // decides which of them it can honour.
    let plain = draw(&document(""));
    let described = masks(&plain);
    let spec = described.first().expect("the mask was described");
    assert_eq!(spec.kind, SoftMaskKind::Luminosity, "the default when /S is absent");
    assert!(spec.backdrop.is_none(), "no /BC");
    assert!(spec.transfer.is_none(), "no /TR");
    assert!(spec.is_plain_luminosity(), "the case a luminance-mask renderer can take");
}

#[test]
fn an_alpha_mask_with_a_backdrop_arrives_whole() {
    // The three entries a renderer built on luminance masks cannot express still reach
    // it, because whether they can be honoured is a question about the backend and not
    // about the document.
    let doc = document_with_mask(
        "<< /Type /ExtGState /SMask << /S /Alpha /G 6 0 R /BC [0.25]          /TR << /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >> >> >>",
    );
    let spec = masks(&draw(&doc)).first().cloned().expect("described");
    assert_eq!(spec.kind, SoftMaskKind::Alpha);
    assert_eq!(spec.backdrop, Some(Color::Gray(0.25)));
    assert!(spec.transfer.is_some(), "/TR parsed through the 7.10 evaluator");
    assert!(!spec.is_plain_luminosity(), "and it says it is not the easy case");
}

#[test]
fn an_identity_transfer_is_the_absence_of_one() {
    // `/TR /Identity` changes nothing, and saying so in the type spares every backend
    // from evaluating a function to discover it.
    let doc = document_with_mask(
        "<< /Type /ExtGState /SMask << /S /Luminosity /G 6 0 R /TR /Identity >> >>",
    );
    let spec = masks(&draw(&doc)).first().cloned().expect("described");
    assert!(spec.transfer.is_none());
    assert!(spec.is_plain_luminosity());
}

#[test]
fn isolation_and_knockout_change_nothing_and_are_recorded() {
    // Not "the group is mishandled" — the entry is not read at all. The two runs produce
    // the same backend calls in the same order, which is the strongest form this can take.
    let plain = document("");
    let asked = document("/Group << /S /Transparency /I true /K true >>");
    assert_eq!(fills(&draw(&plain)), fills(&draw(&asked)), "the entry changes nothing drawn");

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
    // Six and not five: four squares, the form's own, and the mask group's, which is what
    // a soft mask being replayed at all looks like from here.
    assert_eq!(fills(&draw(&plain)).len(), 6, "and it still draws");
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
