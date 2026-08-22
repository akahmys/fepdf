//! Drawing the appearance streams annotations carry (12.5.5), and the flags that stop it.
//!
//! **6.3.2.2 is not a subset a processor chooses.** It says that when a PDF processor
//! renders a page it shall render the appropriate appearance stream for all annotations
//! that have one, unless the annotation flags say otherwise — and this engine drew none
//! at all. A page whose only mark is an annotation came out blank while every other
//! reader painted it; `pdf20examples/PDF 2.0 UTF-8 string and annotation.pdf` is exactly
//! that page and is now in `crosscheck_image.sh`.
//!
//! What is asserted here is the part a picture cannot show: which annotations are skipped
//! and why, and that the appearance lands where `/Rect` says rather than where its own
//! coordinates would have put it.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_content::{
    BlendMode, Color, FallbackFontType, Paint, PixelFormat, RenderBackend, SMaskData, ShadingSpec,
    StrokeStyle, TextGlyph, TextState, WindingRule,
};
use fepdf_model::graphics::TextRenderingMode;
use kurbo::{Affine, BezPath, Shape};
use std::sync::Arc;

/// Where each filled path landed on the **page**.
///
/// The interpreter hands a backend the path in the coordinates the content stream wrote,
/// and the transform separately — so a backend that ignores the transform sees every
/// appearance at its own origin and would have called this feature working when it was
/// not. `current` is what a real backend keeps.
#[derive(Default)]
struct Marks {
    placed: Vec<(f64, f64, f64, f64)>,
    current: Option<Affine>,
}

impl Marks {
    fn len(&self) -> usize {
        self.placed.len()
    }
    fn is_empty(&self) -> bool {
        self.placed.is_empty()
    }
}

impl RenderBackend for Marks {
    fn fill_path(&mut self, path: &BezPath, _color: &Color, _rule: WindingRule) {
        let b = (self.current.unwrap_or(Affine::IDENTITY) * path.clone()).bounding_box();
        self.placed.push((b.x0, b.y0, b.x1, b.y1));
    }
    fn transform(&mut self, transform: Affine) {
        let current = self.current.unwrap_or(Affine::IDENTITY);
        self.current = Some(current * transform);
    }
    fn set_transform(&mut self, transform: Affine) {
        self.current = Some(transform);
    }
    fn push_state(&mut self) {}
    fn pop_state(&mut self) {}
    fn stroke_path(&mut self, _path: &BezPath, _color: &Color, _style: &StrokeStyle) {}
    fn push_clip(&mut self, _path: &BezPath, _rule: WindingRule) {}
    fn pop_clip(&mut self) {}
    fn set_fill_alpha(&mut self, _alpha: f64) {}
    fn set_stroke_alpha(&mut self, _alpha: f64) {}
    fn set_fill_color(&mut self, _color: Color) {}
    fn set_stroke_color(&mut self, _color: Color) {}
    fn set_fill_paint(&mut self, _paint: &Paint) {}
    fn set_stroke_paint(&mut self, _paint: &Paint) {}
    fn paint_shading(&mut self, _shading: &ShadingSpec) {}
    fn set_blend_mode(&mut self, _mode: BlendMode) {}
    fn draw_image(
        &mut self,
        _image: &[u8],
        _width: u32,
        _height: u32,
        _format: PixelFormat,
        _smask: Option<SMaskData>,
    ) {
    }
    #[allow(clippy::too_many_arguments)]
    fn define_font(
        &mut self,
        _name: &str,
        _base_name: Option<&str>,
        _data: Option<Arc<Vec<u8>>>,
        _index: Option<usize>,
        _cid_to_gid_map: Option<std::collections::BTreeMap<u32, u32>>,
        _fallback_type: FallbackFontType,
        _is_cid_keyed: bool,
    ) {
    }
    fn set_font(&mut self, _name: &str) {}
    fn set_text_render_mode(&mut self, _mode: TextRenderingMode) {}
    fn set_char_spacing(&mut self, _spacing: f64) {}
    fn set_word_spacing(&mut self, _spacing: f64) {}
    fn show_text(
        &mut self,
        _glyphs: &[TextGlyph],
        _size: f64,
        _transform: Affine,
        _state: TextState,
        _op_index: usize,
    ) {
    }
}

/// A page with **no content of its own** and one annotation, so anything drawn came from
/// the appearance. `annot` is merged into the annotation dictionary; `extra` are objects
/// from 6 onward.
fn page_with_annotation(annot: &str, extra: &[String]) -> Vec<u8> {
    // The appearance paints its whole bounding box, so where it lands is measurable.
    let appearance = "0 0 0 rg 0 0 10 10 re f\n";
    let mut bodies = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Annots [4 0 R] >>".to_string(),
        format!("<< /Type /Annot /Subtype /Square /Rect [20 40 120 90] {annot} >>"),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Resources << >> \
             /Length {} >>\nstream\n{appearance}endstream",
            appearance.len()
        ),
    ];
    bodies.extend_from_slice(extra);

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

/// Draws page 1 and reports the marks and the decisions taken.
fn draw(file: Vec<u8>) -> (Marks, Vec<String>) {
    let doc = PdfDocument::open_with_options(file.into(), &IngestionOptions::default())
        .expect("the fixture opens");
    let mut marks = Marks::default();
    doc.render_page(0, &mut marks, Affine::IDENTITY).expect("the page renders");
    let decisions = doc.decisions().iter().map(|d| format!("{} {}", d.clause, d.found)).collect();
    (marks, decisions)
}

/// **The gap.** A page with no `/Contents` and one annotation drew nothing at all.
#[test]
fn an_annotation_with_an_appearance_is_drawn() {
    let (marks, _) = draw(page_with_annotation("/AP << /N 5 0 R >>", &[]));
    assert_eq!(marks.len(), 1, "the appearance was not drawn: {:?}", marks.placed);
}

/// And it lands on `/Rect`, not on its own coordinates. The appearance's box is 10x10 at
/// the origin and the rectangle is 100x50 at (20, 40); 12.5.5's algorithm scales and
/// translates the first onto the second.
#[test]
fn the_appearance_is_placed_on_the_annotations_rectangle() {
    let (marks, _) = draw(page_with_annotation("/AP << /N 5 0 R >>", &[]));
    let (x0, y0, x1, y1) = marks.placed[0];
    for (got, want, what) in
        [(x0, 20.0, "left"), (y0, 40.0, "bottom"), (x1, 120.0, "right"), (y1, 90.0, "top")]
    {
        assert!(
            (got - want).abs() < 0.01,
            "the {what} edge is at {got}, and /Rect puts it at {want}"
        );
    }
}

/// Bit 2 is `Hidden`: do not render it, whatever its type (12.5.3, Table 167).
#[test]
fn the_hidden_flag_stops_it() {
    let (marks, _) = draw(page_with_annotation("/F 2 /AP << /N 5 0 R >>", &[]));
    assert!(marks.is_empty(), "a hidden annotation was drawn: {:?}", marks.placed);
}

/// Bit 6 is `NoView`: not on a screen, which is what this renders to.
#[test]
fn the_noview_flag_stops_it() {
    let (marks, _) = draw(page_with_annotation("/F 32 /AP << /N 5 0 R >>", &[]));
    assert!(marks.is_empty(), "a no-view annotation was drawn: {:?}", marks.placed);
}

/// Bit 3 is `Print`, which says nothing about the screen. An annotation carrying only
/// that flag is still drawn — reading it as "print only" would hide half a document.
#[test]
fn the_print_flag_alone_does_not_stop_it() {
    let (marks, _) = draw(page_with_annotation("/F 4 /AP << /N 5 0 R >>", &[]));
    assert_eq!(marks.len(), 1, "the print flag hid an annotation: {:?}", marks.placed);
}

/// `/N` may be a dictionary of states, and `/AS` says which is current — that is how a
/// checkbox keeps `/Off` and `/Yes` in one place.
#[test]
fn the_state_named_by_as_is_the_one_drawn() {
    let off = "\n"; // draws nothing
    let file = page_with_annotation(
        "/AS /Yes /AP << /N << /Yes 5 0 R /Off 6 0 R >> >>",
        &[format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Resources << >> \
             /Length {} >>\nstream\n{off}endstream",
            off.len()
        )],
    );
    let (marks, _) = draw(file);
    assert_eq!(marks.len(), 1, "the /Yes state should have drawn its square");
}

/// A dictionary of states with no `/AS` and more than one to choose from draws **nothing**
/// and says so. Picking one would be this engine deciding what the document did not.
#[test]
fn a_state_dictionary_with_no_as_draws_nothing_and_says_so() {
    let off = "\n";
    let file = page_with_annotation(
        "/AP << /N << /Yes 5 0 R /Off 6 0 R >> >>",
        &[format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Resources << >> \
             /Length {} >>\nstream\n{off}endstream",
            off.len()
        )],
    );
    let (marks, decisions) = draw(file);
    assert!(marks.is_empty(), "{:?}", marks.placed);
    assert!(
        decisions.iter().any(|d| d.starts_with("12.5.5") && d.contains("2 states")),
        "the omission was not reported: {decisions:?}"
    );
}

/// One state and no `/AS` is not ambiguous, so it is drawn and the omission recorded as a
/// repair rather than a refusal.
#[test]
fn a_single_state_with_no_as_is_drawn() {
    let (marks, decisions) = draw(page_with_annotation("/AP << /N << /Yes 5 0 R >> >>", &[]));
    assert_eq!(marks.len(), 1, "{:?}", marks.placed);
    assert!(decisions.iter().any(|d| d.starts_with("12.5.5")), "{decisions:?}");
}

/// An annotation carries `/OC` too (8.11.3.2), and a group that is off hides it. Nothing
/// exercised this before, because nothing drew annotations at all.
#[test]
fn an_annotation_in_a_layer_that_is_off_is_not_drawn() {
    let mut file = page_with_annotation(
        "/OC 6 0 R /AP << /N 5 0 R >>",
        &["<< /Type /OCG /Name (Hidden) >>".to_string()],
    );
    // The catalogue needs the configuration that turns it off.
    let patched = String::from_utf8_lossy(&file).replace(
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [6 0 R] \
             /D << /OFF [6 0 R] >> >> >>",
    );
    file = patched.into_bytes();
    // The offsets moved, and the reader recovers by scanning — which is what makes this
    // fixture legible instead of arithmetic.
    let (marks, _) = draw(file);
    assert!(marks.is_empty(), "an annotation in a layer that is off was drawn: {:?}", marks.placed);
}

/// An annotation with no appearance at all is not drawn and is not an error: a `/Link` is
/// the commonest annotation in the corpus and 30,016 of them carry none.
#[test]
fn an_annotation_with_no_appearance_is_skipped_quietly() {
    let (marks, decisions) = draw(page_with_annotation("", &[]));
    assert!(marks.is_empty());
    assert!(decisions.is_empty(), "{decisions:?}");
}
