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
use kurbo::Affine;

use fepdf_fixtures::recorder::Recorder;

use fepdf_fixtures::assemble;

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

/// Where the interpreter asked for a fill: the corner of each filled box, in user space.
fn painted_top_left(drawn: &Recorder) -> bool {
    drawn.fills().iter().any(|b| b.x0 < 50.0 && b.y0 > 50.0)
}

fn painted_bottom_right(drawn: &Recorder) -> bool {
    drawn.fills().iter().any(|b| b.x0 > 50.0 && b.y0 < 50.0)
}

fn draw(document: &PdfDocument) -> Recorder {
    let mut recorder = Recorder::new();
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
    assert!(painted_top_left(&draw(&doc)), "the layer is on to start with");

    let panel = doc.layers();
    assert!(doc.set_layer_visible(&panel, only_layer(&panel), false));

    let after = draw(&doc);
    assert!(!painted_top_left(&after), "the toggle has to reach the renderer");
    assert!(painted_bottom_right(&after), "and take nothing else with it");
}

#[test]
fn turning_a_layer_back_on_restores_it() {
    let doc = document("/D << /BaseState /ON /OFF [5 0 R] /Order [5 0 R] >>");
    assert!(!painted_top_left(&draw(&doc)), "/OFF hides it to start with");

    let panel = doc.layers();
    assert!(doc.set_layer_visible(&panel, only_layer(&panel), true));
    assert!(painted_top_left(&draw(&doc)), "a viewer may overrule the configuration");
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
    assert!(!painted_top_left(&draw(&doc)));

    doc.reset_layer_visibility();
    assert!(painted_top_left(&draw(&doc)), "the document's own answer is still there");
}
