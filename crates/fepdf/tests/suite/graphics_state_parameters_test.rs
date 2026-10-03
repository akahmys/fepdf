//! `gs` sets the line parameters of Table 57, and `w`/`J`/`j`/`M`/`d` are not the only
//! way to reach them.
//!
//! **Measured 2026-09-06, before this existed**: a page that set its stroke width through
//! an `/ExtGState` drew every stroke at the initial width, and was indistinguishable from
//! a page that set nothing at all.
//!
//! | how the parameters were set | width | cap / join |
//! | :--- | ---: | :--- |
//! | `10 w 1 J 1 j` | 10 | Round / Round |
//! | `/G1 gs` with `/LW 10 /LC 1 /LJ 1` | **1** | **Butt / Miter** |
//! | nothing | 1 | Butt / Miter |
//!
//! The struct knew: `StrokeStyle`'s fields are documented as `/LC`, `/LJ`, `/ML` and
//! `/D` — the ExtGState keys — and `handle_gs_operator` read `/ca`, `/CA`, `/BM`,
//! `/SMask` and `/Font` and stopped.

use fepdf::PdfDocument;
use fepdf_content::StrokeStyle;
use kurbo::Affine;

use fepdf_fixtures::recorder::{Event, Recorder};

use fepdf_fixtures::assemble;

/// The strokes a page draws, given a content stream and one `/ExtGState` named `/G1`.
fn strokes(content: &str, ext_g_state: &str) -> (Vec<StrokeStyle>, Vec<fepdf::Decision>) {
    let bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
          /Resources << /ExtGState << /G1 5 0 R >> >> /Contents 4 0 R >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        ext_g_state.to_string(),
    ];
    let doc = PdfDocument::open(bytes::Bytes::from(assemble(&bodies))).expect("the fixture opens");
    let mut backend = Recorder::new();
    doc.render_page(0, &mut backend, Affine::IDENTITY).expect("the page interprets");
    let pens = backend
        .events
        .iter()
        .filter_map(
            |e| if let Event::Stroke { style, .. } = e { Some(style.clone()) } else { None },
        )
        .collect();
    (pens, doc.decisions())
}

const LINE: &str = "10 10 m 100 100 l S";
const FULL: &str = "<< /Type /ExtGState /LW 10 /LC 1 /LJ 1 /ML 7 /D [[3 2] 1] >>";

/// `gs` reaches the same state the operators do, parameter for parameter.
///
/// Asserted against the operator form rather than against literals, because the question
/// is whether the two routes agree — not what either of them happens to produce.
#[test]
fn gs_sets_what_the_operators_set() {
    let (by_operator, _) = strokes(&format!("10 w 1 J 1 j 7 M [3 2] 1 d {LINE}"), FULL);
    let (by_ext_g_state, _) = strokes(&format!("/G1 gs {LINE}"), FULL);

    assert_eq!(by_operator.len(), 1, "the operator form draws one stroke");
    assert_eq!(by_ext_g_state.len(), 1, "and so does the ExtGState form");
    assert_eq!(
        format!("{:?}", by_ext_g_state[0]),
        format!("{:?}", by_operator[0]),
        "an ExtGState must set the same state the operators do"
    );
}

/// And it is not simply that both routes leave the initial state alone.
///
/// Without this, an implementation that ignored `gs` *and* the operators would pass the
/// test above.
#[test]
fn the_state_the_two_routes_reach_is_not_the_initial_one() {
    let (set, _) = strokes(&format!("/G1 gs {LINE}"), FULL);
    let (untouched, _) = strokes(LINE, FULL);

    assert_ne!(format!("{:?}", set[0]), format!("{:?}", untouched[0]));
    assert!((set[0].width - 10.0).abs() < 1e-9, "width came from /LW: {:?}", set[0]);
    assert!(set[0].dash_pattern.is_some(), "dash came from /D: {:?}", set[0]);
}

/// A `/LC` an ExtGState may not carry is recorded, as `J` with the same value is.
///
/// Rule 20's ground: an enumerant the standard does not define gets a substitute, and the
/// substitution has to be said. `J 7` has recorded since 2026-08-30; `/LC 7` did not
/// exist as a path at all.
#[test]
fn an_undefined_enumerant_in_an_ext_g_state_is_recorded() {
    let (_, decisions) = strokes(&format!("/G1 gs {LINE}"), "<< /Type /ExtGState /LC 7 >>");
    let found = decisions.iter().find(|d| d.clause == "8.4.3.3").unwrap_or_else(|| {
        panic!("an /LC of 7 must be recorded against Table 53; got {decisions:?}")
    });
    assert!(found.found.contains('7'), "{}", found.found);
}
