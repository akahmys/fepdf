//! What a Form XObject's ceremony does, and that both implementations of it agree.
//!
//! `/Do` on a form pushes the graphics state, concatenates `/Matrix`, clips to `/BBox`
//! (8.10.1), runs the form's content, unwinds the clip and restores the state. That
//! That sequence stood **twice** in `fepdf-content` when these were written:
//! `execute_form_commands` took the content as parsed `Command`s and `render_form_xobject`
//! as raw bytes, and 66 of their 70 lines were identical. Nothing checked either, and
//! nothing checked that they agreed. These five held the contract while it was written
//! once, in `in_form_frame`.
//!
//! **They still guard a fork, and a smaller one.** The two entry points remain, because a
//! form's stream arrives either already refined or not; the bytes are sublimated on the
//! way in now, so both reach the same `Command`s and the same ceremony.
//! `crates/fepdf/tests/parser_twin_test.rs` is the general form of the fifth test here.

use fepdf::PdfDocument;
use fepdf_content::Color;
use fepdf_model::ingest::IngestionOptions;
use kurbo::{Affine, Shape};

use fepdf_fixtures::recorder::{Event, Recorder};

use fepdf_fixtures::assemble;

/// A page drawing one form through `/Do`, and one rectangle after it.
///
/// The form paints a 10x10 red square at the origin and sets the fill colour to red; the
/// page paints a 20x20 square in blue afterwards, which is what says whether the form's
/// state escaped.
fn page(matrix: &str, bbox: &str) -> Vec<u8> {
    let form = "1 0 0 rg 0 0 10 10 re f\n";
    let content = "q 1 0 0 1 50 50 cm /Fx Do Q\n0 0 1 rg 0 0 20 20 re f\n";
    assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
         /Resources << /XObject << /Fx 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox {bbox} {matrix} /Length {} >>\n\
             stream\n{form}endstream",
            form.len()
        ),
    ])
}

/// The calls the form made, with the detail this file asserts on.
///
/// A string rather than the [`Event`] itself so the two implementations can be compared
/// whole, and rounded to one decimal so a matrix that differs in the last bit of a f64
/// does not read as two implementations disagreeing.
fn calls(drawn: &Recorder) -> Vec<String> {
    drawn
        .events
        .iter()
        .filter_map(|e| {
            if let Event::Fill { path, color, .. } = e {
                let b = path.bounding_box();
                let shade = match *color {
                    Color::Rgb(red, green, blue) => format!("rgb {red:.1},{green:.1},{blue:.1}"),
                    Color::Gray(level) => format!("gray {level:.1}"),
                    Color::Cmyk(cyan, magenta, yellow, black) => {
                        format!("cmyk {cyan:.1},{magenta:.1},{yellow:.1},{black:.1}")
                    }
                    Color::Lab(lightness, green_red, blue_yellow) => {
                        format!("lab {lightness:.1},{green_red:.1},{blue_yellow:.1}")
                    }
                };
                return Some(format!(
                    "fill[{shade}]({:.1},{:.1},{:.1},{:.1})",
                    b.x0, b.y0, b.x1, b.y1
                ));
            }
            if let Event::PushClip { path, .. } = e {
                let b = path.bounding_box();
                return Some(format!("push_clip({:.1},{:.1},{:.1},{:.1})", b.x0, b.y0, b.x1, b.y1));
            }
            if let Event::Transform(t) = e {
                let c = t.as_coeffs();
                return Some(format!(
                    "transform({:.1},{:.1},{:.1},{:.1},{:.1},{:.1})",
                    c[0], c[1], c[2], c[3], c[4], c[5]
                ));
            }
            matches!(e, Event::PopClip | Event::PushState | Event::PopState)
                .then(|| e.name().to_string())
        })
        .collect()
}

/// Just the fills, in order.
fn fills(drawn: &Recorder) -> Vec<String> {
    calls(drawn).into_iter().filter(|e| e.starts_with("fill")).collect()
}

fn draw(bytes: Vec<u8>, refine: bool) -> Recorder {
    let options = IngestionOptions { active_refinement: refine, ..IngestionOptions::default() };
    let doc = PdfDocument::open_with_options(bytes.into(), &options).expect("the fixture opens");
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder
}

/// `/Matrix` is concatenated onto the CTM before the form's content draws.
///
/// The backend composes it; the path arrives in the form's own space either way, so the
/// fill's bounding box is the same with and without a `/Matrix` and says nothing. What
/// says it is the `transform` call the interpreter makes — which is where the first
/// version of this test looked in the wrong place and passed a wrong reading as a defect.
#[test]
fn the_forms_matrix_reaches_the_backend() {
    let plain = draw(page("", "[0 0 10 10]"), true);
    let scaled = draw(page("/Matrix [2 0 0 2 0 0]", "[0 0 10 10]"), true);
    assert!(
        !calls(&plain).iter().any(|e| e.starts_with("transform(2.0")),
        "a form with no /Matrix concatenates none: {:?}",
        calls(&plain)
    );
    assert!(
        calls(&scaled).iter().any(|e| e == "transform(2.0,0.0,0.0,2.0,0.0,0.0)"),
        "the /Matrix the form declares has to reach the backend: {:?}",
        calls(&scaled)
    );
}

/// `/BBox` clips the form's content (8.10.1).
#[test]
fn the_forms_bbox_becomes_a_clip() {
    let r = draw(page("", "[0 0 4 4]"), true);
    let clips: Vec<String> = calls(&r).into_iter().filter(|e| e.starts_with("push_clip")).collect();
    assert!(!clips.is_empty(), "a form with a /BBox has to clip to it: {:?}", calls(&r));
    assert!(
        clips.iter().any(|c| c.contains("4.0")),
        "the clip has to be the /BBox the form declared, not another rectangle: {clips:?}"
    );
}

/// The state the form changed does not survive it.
#[test]
fn the_forms_state_does_not_escape_it() {
    let r = draw(page("", "[0 0 10 10]"), true);
    let after = fills(&r).last().cloned().unwrap_or_default();
    assert!(
        after.contains("0.0,0.0,1.0"),
        "the square after the form is blue; the form set red and must not have kept it: {after}"
    );
}

/// The clip the form pushed is unwound with it.
#[test]
fn the_forms_clip_is_unwound_with_it() {
    let r = draw(page("", "[0 0 4 4]"), true);
    assert_eq!(
        r.count("push_clip"),
        r.count("pop_clip"),
        "every clip the form pushes is popped, or what follows draws inside it: {:?}",
        calls(&r)
    );
}

/// **The two implementations of the ceremony agree.**
///
/// `execute_form_commands` runs when ingestion refined the form's stream into `Command`s
/// and `render_form_xobject` when it did not, so the same document reaches a different
/// implementation depending only on `active_refinement`. They are 66 identical lines out
/// of 70, and nothing said so.
#[test]
fn both_form_implementations_produce_the_same_calls() {
    let bytes = page("/Matrix [2 0 0 2 0 0]", "[0 0 10 10]");
    let refined = draw(bytes.clone(), true);
    let raw = draw(bytes, false);
    assert_eq!(
        calls(&refined),
        calls(&raw),
        "the parsed-command path and the raw-byte path have to draw the same form"
    );
}
