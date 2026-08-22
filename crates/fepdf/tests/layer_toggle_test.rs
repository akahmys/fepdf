//! Toggling a layer changes what the page draws (ISO 32000-2, 6.3.2.3).
//!
//! `layer_panel_tests.rs` holds the panel to 8.11.4.3's rules about *presentation* —
//! which groups appear, which are locked, how a radio set behaves. This asks the question
//! those cannot: whether a toggle reaches the renderer at all. A panel that lists layers
//! correctly and changes nothing on the page is the same to a reader as no panel.
//!
//! The backend records rather than rasterises, so this runs without a GPU: the question
//! is whether the interpreter *called* it.

use fepdf::{LayerPanel, LayerRow, PdfDocument};
use fepdf_content::{
    BlendMode, Color, FallbackFontType, Paint, PixelFormat, RenderBackend, SMaskData, ShadingSpec,
    StrokeStyle, TextGlyph, TextState, WindingRule,
};
use fepdf_model::graphics::TextRenderingMode;
use kurbo::{Affine, BezPath, Shape};
use std::sync::Arc;

/// Where the interpreter asked for a fill, in user space.
#[derive(Default)]
struct Recorder {
    fills: Vec<(f64, f64)>,
}

impl Recorder {
    fn painted_top_left(&self) -> bool {
        self.fills.iter().any(|(x, y)| *x < 50.0 && *y > 50.0)
    }
    fn painted_bottom_right(&self) -> bool {
        self.fills.iter().any(|(x, y)| *x > 50.0 && *y < 50.0)
    }
}

impl RenderBackend for Recorder {
    fn fill_path(&mut self, path: &BezPath, _color: &Color, _rule: WindingRule) {
        let bounds = path.bounding_box();
        self.fills.push((bounds.x0, bounds.y0));
    }
    fn transform(&mut self, _t: Affine) {}
    fn set_transform(&mut self, _t: Affine) {}
    fn push_state(&mut self) {}
    fn pop_state(&mut self) {}
    fn stroke_path(&mut self, _p: &BezPath, _c: &Color, _s: &StrokeStyle) {}
    fn set_fill_color(&mut self, _c: Color) {}
    fn set_stroke_color(&mut self, _c: Color) {}
    fn set_fill_paint(&mut self, _p: &Paint) {}
    fn set_stroke_paint(&mut self, _p: &Paint) {}
    fn set_fill_alpha(&mut self, _a: f64) {}
    fn set_stroke_alpha(&mut self, _a: f64) {}
    fn paint_shading(&mut self, _s: &ShadingSpec) {}
    fn set_blend_mode(&mut self, _m: BlendMode) {}
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

/// One page: a square in the top-left under `/OC`, and one in the bottom-right under
/// nothing, so every assertion says both what was hidden and what survived.
fn document(configuration: &str) -> PdfDocument {
    let content = "/OC /MC0 BDC\n0 0 0 rg 0 100 100 100 re f\nEMC\n\
                   0 0 0 rg 100 0 100 100 re f\n";
    let bodies = vec![
        format!(
            "<< /Type /Catalog /Pages 2 0 R \
             /OCProperties << /OCGs [5 0 R] {configuration} >> >>"
        ),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
         /Resources << /Properties << /MC0 5 0 R >> >> /Contents 4 0 R >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /OCG /Name (Detail) >>".to_string(),
    ];
    PdfDocument::open(assemble(&bodies).into()).expect("the fixture opens")
}

fn draw(document: &PdfDocument) -> Recorder {
    let mut recorder = Recorder::default();
    document.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder
}

fn only_layer(panel: &LayerPanel) -> fepdf::LayerId {
    match panel.rows.first().expect("the panel presents the layer") {
        LayerRow::Group { id, .. } => *id,
        LayerRow::Label(_) | LayerRow::Nested(_) => panic!("expected a group row"),
    }
}

#[test]
fn turning_a_layer_off_withholds_its_content() {
    let doc = document("/D << /BaseState /ON /Order [5 0 R] >>");
    assert!(draw(&doc).painted_top_left(), "the layer is on to start with");

    let panel = doc.layers();
    assert!(doc.set_layer_visible(&panel, only_layer(&panel), false));

    let after = draw(&doc);
    assert!(!after.painted_top_left(), "the toggle has to reach the renderer");
    assert!(after.painted_bottom_right(), "and take nothing else with it");
}

#[test]
fn turning_a_layer_back_on_restores_it() {
    let doc = document("/D << /BaseState /ON /OFF [5 0 R] /Order [5 0 R] >>");
    assert!(!draw(&doc).painted_top_left(), "/OFF hides it to start with");

    let panel = doc.layers();
    assert!(doc.set_layer_visible(&panel, only_layer(&panel), true));
    assert!(draw(&doc).painted_top_left(), "a viewer may overrule the configuration");
}

#[test]
fn a_toggle_leaves_the_saved_bytes_alone() {
    let dir = std::env::temp_dir().join("fepdf-layer-toggle-test");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let doc = document("/D << /BaseState /ON /Order [5 0 R] >>");

    let before_path = dir.join("before.pdf");
    doc.save_as_version(&before_path, "2.0").expect("saves");
    let before = std::fs::read(&before_path).expect("reads back");

    let panel = doc.layers();
    doc.set_layer_visible(&panel, only_layer(&panel), false);

    let after_path = dir.join("after.pdf");
    doc.save_as_version(&after_path, "2.0").expect("saves");
    let after = std::fs::read(&after_path).expect("reads back");

    // 6.3.2.3 asks an interactive processor to let a person change what they see. It
    // does not ask it to edit their file, which is why this is not an `Operation` — and
    // why the bytes are identical either side of the toggle.
    assert_eq!(before, after, "viewing is not editing");
}

#[test]
fn resetting_returns_to_what_the_configuration_says() {
    let doc = document("/D << /BaseState /ON /Order [5 0 R] >>");
    let panel = doc.layers();
    doc.set_layer_visible(&panel, only_layer(&panel), false);
    assert!(!draw(&doc).painted_top_left());

    doc.reset_layer_visibility();
    assert!(draw(&doc).painted_top_left(), "the document's own answer is still there");
}
