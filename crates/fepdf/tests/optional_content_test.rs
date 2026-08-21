//! What a page marked with `/OC` actually draws (8.11).
//!
//! Thirteen constructions, each a page that paints a square in the **top-left** quarter
//! under some optional-content condition and a square in the **bottom-right** with none —
//! so every case says both "the right thing was hidden" and "the rest of the page
//! survived". The same thirteen were put to PDFKit as rendered pages
//! (`scripts/test/crosscheck_image.sh`, and see
//! `docs/adr/0021-optional-content-hides-only-what-the-document-unambiguously-turns-off.md`):
//! it agrees on two of them and paints the other eleven, so these assertions are held to
//! the clause rather than to a second implementation.
//!
//! The backend records rather than rasterises, which is why this runs without a GPU: the
//! question is whether the interpreter *called* the backend, and a pixel is a slower way
//! of asking.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_content::{
    BlendMode, Color, FallbackFontType, Paint, PixelFormat, RenderBackend, SMaskData, ShadingSpec,
    StrokeStyle, TextGlyph, TextState, WindingRule,
};
use fepdf_model::graphics::TextRenderingMode;
use kurbo::{Affine, BezPath, Shape};
use std::sync::Arc;

/// Everything the interpreter asked for, in the order it asked.
#[derive(Default)]
struct Recorder {
    /// The origin of each filled path, in user space.
    fills: Vec<(f64, f64)>,
    /// How many images were drawn.
    images: usize,
    /// How many glyph runs were shown.
    text_runs: usize,
    /// How many state operations arrived — these must *not* be suppressed.
    state_calls: usize,
}

impl Recorder {
    /// Whether the square in the top-left quarter reached the page.
    fn painted_top_left(&self) -> bool {
        self.fills.iter().any(|(x, y)| *x < 50.0 && *y > 50.0)
    }

    /// Whether the unconditional square in the bottom-right did.
    fn painted_bottom_right(&self) -> bool {
        self.fills.iter().any(|(x, y)| *x > 50.0 && *y < 50.0)
    }
}

impl RenderBackend for Recorder {
    fn fill_path(&mut self, path: &BezPath, _color: &Color, _rule: WindingRule) {
        let box_ = path.bounding_box();
        self.fills.push((box_.x0, box_.y0));
    }
    fn draw_image(
        &mut self,
        _image: &[u8],
        _width: u32,
        _height: u32,
        _format: PixelFormat,
        _smask: Option<SMaskData>,
    ) {
        self.images += 1;
    }
    fn show_text(
        &mut self,
        _glyphs: &[TextGlyph],
        _size: f64,
        _transform: Affine,
        _state: TextState,
        _op_index: usize,
    ) {
        self.text_runs += 1;
    }
    fn push_state(&mut self) {
        self.state_calls += 1;
    }
    fn pop_state(&mut self) {
        self.state_calls += 1;
    }
    fn set_fill_color(&mut self, _color: Color) {
        self.state_calls += 1;
    }
    fn transform(&mut self, _transform: Affine) {
        self.state_calls += 1;
    }
    fn stroke_path(&mut self, _path: &BezPath, _color: &Color, _style: &StrokeStyle) {}
    fn set_transform(&mut self, _transform: Affine) {}
    fn push_clip(&mut self, _path: &BezPath, _rule: WindingRule) {}
    fn pop_clip(&mut self) {}
    fn set_fill_alpha(&mut self, _alpha: f64) {}
    fn set_stroke_alpha(&mut self, _alpha: f64) {}
    fn set_stroke_color(&mut self, _color: Color) {}
    fn set_fill_paint(&mut self, _paint: &Paint) {}
    fn set_stroke_paint(&mut self, _paint: &Paint) {}
    fn paint_shading(&mut self, _shading: &ShadingSpec) {}
    fn set_blend_mode(&mut self, _mode: BlendMode) {}
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
}

/// The square under test, in the top-left quarter of a 200×200 page.
const TOP_LEFT: &str = "0 0 0 rg 0 100 100 100 re f\n";
/// The square that is never conditional, in the bottom-right.
const BOTTOM_RIGHT: &str = "0 0 0 rg 100 0 100 100 re f\n";

/// Assembles a one-page file from object bodies, numbered from 1.
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

/// A stream object with `extra` merged into its dictionary.
fn stream(extra: &str, data: &str) -> String {
    format!("<< {extra} /Length {} >>\nstream\n{data}endstream", data.len())
}

/// One 200×200 page: `oc` goes in the catalogue, `resources` in the page, `content` is
/// the stream, and `extra` are objects 5 onward.
fn page(oc: &str, resources: &str, content: &str, extra: &[String]) -> Vec<u8> {
    let mut bodies = vec![
        format!("<< /Type /Catalog /Pages 2 0 R {oc} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
             /Resources << {resources} >> /Contents 4 0 R >>"
        ),
        stream("", content),
    ];
    bodies.extend_from_slice(extra);
    assemble(&bodies)
}

/// The group every fixture below refers to as object 5.
fn group(extra: &str) -> String {
    format!("<< /Type /OCG /Name (Layer) {extra} >>")
}

/// Interprets page 1 and reports what the backend was asked to draw.
fn draw(file: Vec<u8>) -> Recorder {
    let document = PdfDocument::open_with_options(file.into(), &IngestionOptions::default())
        .expect("the fixture opens");
    let mut recorder = Recorder::default();
    document.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder
}

/// Asserts the conditional square was withheld and the unconditional one was not.
fn assert_hidden(what: &str, recorder: &Recorder) {
    assert!(!recorder.painted_top_left(), "{what}: the hidden square was painted");
    assert!(recorder.painted_bottom_right(), "{what}: the rest of the page went with it");
}

/// Asserts both squares reached the page.
fn assert_drawn(what: &str, recorder: &Recorder) {
    assert!(recorder.painted_top_left(), "{what}: the visible square was withheld");
    assert!(recorder.painted_bottom_right(), "{what}: the rest of the page went with it");
}

/// A `/OC` section wrapping the content, with `/MC0` naming object 5.
fn marked(content: &str) -> String {
    format!("/OC /MC0 BDC\n{content}EMC\n{BOTTOM_RIGHT}")
}

// --- the default configuration's own entries (8.11.4.4, Table 100) -------------------

#[test]
fn a_group_in_the_off_array_is_not_drawn() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /BaseState /ON /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("")],
    );
    assert_hidden("a group in /OFF", &draw(file));
}

#[test]
fn a_group_in_the_on_array_is_drawn() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /BaseState /ON /ON [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("")],
    );
    assert_drawn("a group in /ON", &draw(file));
}

#[test]
fn base_state_off_turns_off_every_declared_group() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /BaseState /OFF >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("")],
    );
    assert_hidden("/BaseState /OFF", &draw(file));
}

#[test]
fn the_on_array_rescues_a_group_from_base_state_off() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /BaseState /OFF /ON [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("")],
    );
    assert_drawn("/BaseState /OFF with /ON", &draw(file));
}

/// 8.11.2.2: a group whose `/Intent` does not meet the configuration's is "not
/// considered", so its state never applies and its content is drawn — even from `/OFF`.
#[test]
fn a_group_whose_intent_the_configuration_does_not_share_is_not_considered() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /Intent /View /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("/Intent /Design")],
    );
    assert_drawn("a /Design group under a /View configuration", &draw(file));
}

/// And the same group *is* considered where the configuration asks for its intent.
#[test]
fn a_configuration_that_shares_the_intent_does_consider_it() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /Intent [/View /Design] /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("/Intent /Design")],
    );
    assert_hidden("a /Design group under a /Design configuration", &draw(file));
}

// --- usage, applied through /AS (8.11.4.5) -------------------------------------------

#[test]
fn a_view_usage_named_by_as_turns_a_group_that_is_on_off() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /ON [5 0 R] \
         /AS [<< /Event /View /Category [/View] /OCGs [5 0 R] >>] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("/Usage << /View << /ViewState /OFF >> >>")],
    );
    assert_hidden("a /View usage applied by /AS", &draw(file));
}

/// The same usage without an `/AS` entry does nothing: 8.11.4.5 makes the application the
/// thing that acts, and a `/Usage` alone is a description.
#[test]
fn a_view_usage_no_as_entry_names_changes_nothing() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /ON [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("/Usage << /View << /ViewState /OFF >> >>")],
    );
    assert_drawn("a /View usage with no /AS", &draw(file));
}

/// `/Print` is read and not applied — nothing here prints, and hiding a layer on a screen
/// because a printer would omit it is a different document from the one that was opened.
#[test]
fn a_print_usage_does_not_hide_anything_on_screen() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /ON [5 0 R] \
         /AS [<< /Event /Print /Category [/Print] /OCGs [5 0 R] >>] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("/Usage << /Print << /PrintState /OFF >> >>")],
    );
    assert_drawn("a /Print usage", &draw(file));
}

// --- membership dictionaries (8.11.2.3, Table 97) ------------------------------------

#[test]
fn an_ocmd_defaults_to_any_on() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R 7 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /OCGs [5 0 R 7 0 R] >>".to_string(), group("")],
    );
    assert_drawn("an OCMD with one group on", &draw(file));
}

#[test]
fn an_ocmd_with_all_on_needs_every_group() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R 7 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /OCGs [5 0 R 7 0 R] /P /AllOn >>".to_string(), group("")],
    );
    assert_hidden("an OCMD with /AllOn", &draw(file));
}

#[test]
fn an_ocmd_with_any_off_is_visible_when_one_is_off() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R 7 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /OCGs [5 0 R 7 0 R] /P /AnyOff >>".to_string(), group("")],
    );
    assert_drawn("an OCMD with /AnyOff", &draw(file));
}

#[test]
fn a_visibility_expression_negates() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /ON [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /VE [/Not 5 0 R] >>".to_string()],
    );
    assert_hidden("/VE [/Not <on>]", &draw(file));
}

#[test]
fn a_visibility_expression_nests() {
    // `/And` of "5 is on" and "not (7 is on)". 5 is on and 7 is off, so both hold.
    let file = page(
        "/OCProperties << /OCGs [5 0 R 7 0 R] /D << /ON [5 0 R] /OFF [7 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /VE [/And 5 0 R [/Not 7 0 R]] >>".to_string(), group("")],
    );
    assert_drawn("/VE [/And <on> [/Not <off>]]", &draw(file));
}

/// `/VE` wins over `/P` and `/OCGs`, which Table 97 says shall be ignored beside it.
#[test]
fn a_visibility_expression_overrides_the_policy() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /ON [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /OCGs [5 0 R] /P /AnyOn /VE [/Not 5 0 R] >>".to_string()],
    );
    assert_hidden("/VE beside /P", &draw(file));
}

/// An expression that refers to itself terminates, and the content is drawn rather than
/// hidden on an answer the evaluator never reached.
#[test]
fn a_visibility_expression_that_loops_does_not_hang_or_hide() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /VE 7 0 R >>".to_string(), "[/Not 7 0 R]".to_string()],
    );
    assert_drawn("a /VE that contains itself", &draw(file));
}

// --- 8.11.3.2: the /OC entry on an XObject -------------------------------------------

#[test]
fn a_form_xobject_with_a_hidden_oc_is_not_executed() {
    let form = stream("/Type /XObject /Subtype /Form /BBox [0 0 200 200] /OC 5 0 R", TOP_LEFT);
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/XObject << /Fm0 6 0 R >>",
        &format!("/Fm0 Do\n{BOTTOM_RIGHT}"),
        &[group(""), form],
    );
    assert_hidden("a form XObject with /OC", &draw(file));
}

#[test]
fn an_image_xobject_with_a_hidden_oc_is_not_drawn() {
    let image = stream(
        "/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray \
         /BitsPerComponent 8 /OC 5 0 R",
        "\u{0}\u{0}\u{0}\u{0}",
    );
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/XObject << /Im0 6 0 R >>",
        &format!("q 100 0 0 100 0 100 cm /Im0 Do Q\n{BOTTOM_RIGHT}"),
        &[group(""), image],
    );
    let recorder = draw(file);
    assert_eq!(recorder.images, 0, "the hidden image was drawn");
    assert!(recorder.painted_bottom_right(), "the rest of the page went with it");
}

// --- the two forms a real file used, and this engine did not read -------------------

/// **A membership dictionary may be written in place.** 8.11.2 requires the *group* to be
/// an indirect object, because only a reference gives it the identity `/OFF` names; an
/// OCMD needs none, since it reaches its groups through `/OCGs`. Conflating the two made
/// every inline OCMD unreadable, and unreadable means drawn.
///
/// Found on `pdf20examples/pdf20-utf8-test.pdf`, whose form XObjects carry
/// `/OC << /Type /OCMD /OCGs 3 0 R >>` with both layers off. PDFKit hides them; this
/// engine drew them until the corpus that contains the file was fetched.
#[test]
fn an_ocmd_written_in_place_is_read_rather_than_refused() {
    let form = stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 200 200] \
         /OC << /Type /OCMD /OCGs [5 0 R] >>",
        TOP_LEFT,
    );
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/XObject << /Fm0 6 0 R >>",
        &format!("/Fm0 Do\n{BOTTOM_RIGHT}"),
        &[group(""), form],
    );
    assert_hidden("an inline OCMD", &draw(file));
}

/// Table 97 lets `/OCGs` be one group instead of an array, and the same file writes it
/// that way. Reading only the array form made an OCMD with one group look like an OCMD
/// with none — which is visible, so the layer was drawn for the second reason as well.
#[test]
fn an_ocmd_naming_a_single_group_rather_than_an_array_is_read() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /OCGs 5 0 R >>".to_string()],
    );
    assert_hidden("an OCMD whose /OCGs is one reference", &draw(file));
}

/// Both at once, which is the shape the file actually writes.
#[test]
fn an_inline_ocmd_naming_a_single_group_is_read() {
    let form = stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 200 200] \
         /OC << /Type /OCMD /OCGs 5 0 R >>",
        TOP_LEFT,
    );
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/XObject << /Fm0 6 0 R >>",
        &format!("/Fm0 Do\n{BOTTOM_RIGHT}"),
        &[group(""), form],
    );
    assert_hidden("pdf20-utf8-test's shape", &draw(file));
}

/// The half of the old rule that was right, and stays: a *group* written in place names
/// nothing `/OCProperties` could have turned off, so it is drawn and recorded.
#[test]
fn a_group_written_in_place_still_names_nothing() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 << /Type /OCG /Name (Layer) >> >>",
        &marked(TOP_LEFT),
        &[group("")],
    );
    assert_drawn("an OCG written in place", &draw(file));
}

// --- nesting, and what may not follow a section out ----------------------------------

/// The `EMC` of a section opened *inside* a hidden one must not bring the page back.
/// PDFKit paints this fixture, which is how the case was found.
#[test]
fn a_section_nested_inside_a_hidden_one_stays_hidden() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &format!(
            "/OC /MC0 BDC\n/Span << /ActualText (x) >> BDC\nEMC\n{TOP_LEFT}EMC\n{BOTTOM_RIGHT}"
        ),
        &[group("")],
    );
    assert_hidden("a /Span inside a hidden /OC", &draw(file));
}

/// An `EMC` with nothing open is ignored rather than revealing what follows.
#[test]
fn an_unbalanced_emc_does_not_reveal_a_hidden_section() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &format!("EMC\n/OC /MC0 BDC\n{TOP_LEFT}EMC\n{BOTTOM_RIGHT}"),
        &[group("")],
    );
    assert_hidden("a stray EMC before the section", &draw(file));
}

/// A form XObject that closes a section it never opened must not reveal the section the
/// `Do` was inside.
#[test]
fn a_form_cannot_close_a_section_it_did_not_open() {
    let form = stream("/Type /XObject /Subtype /Form /BBox [0 0 200 200]", "EMC\n");
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >> /XObject << /Fm0 6 0 R >>",
        &format!("/OC /MC0 BDC\n/Fm0 Do\n{TOP_LEFT}EMC\n{BOTTOM_RIGHT}"),
        &[group(""), form],
    );
    assert_hidden("a form with a stray EMC", &draw(file));
}

// --- nothing is hidden on a doubt ----------------------------------------------------

#[test]
fn an_oc_naming_a_property_the_page_does_not_carry_is_drawn() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC1 5 0 R >>",
        &marked(TOP_LEFT),
        &[group("")],
    );
    assert_drawn("/OC naming an absent property", &draw(file));
}

#[test]
fn content_marked_oc_in_a_document_with_no_ocproperties_is_drawn() {
    let file = page("", "/Properties << /MC0 5 0 R >>", &marked(TOP_LEFT), &[group("")]);
    assert_drawn("/OC with no /OCProperties", &draw(file));
}

#[test]
fn a_policy_table_97_does_not_define_draws_rather_than_guesses() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 6 0 R >>",
        &marked(TOP_LEFT),
        &[group(""), "<< /Type /OCMD /OCGs [5 0 R] /P /Sometimes >>".to_string()],
    );
    assert_drawn("an OCMD with an undefined /P", &draw(file));
}

// --- what a hidden section still does ------------------------------------------------

/// Marks are withheld; the graphics state is not. A viewer that skipped `q`, `Q` and `cm`
/// inside a hidden section would leave it with the wrong transformation, so the wrapper
/// forwards everything that is not a mark.
#[test]
fn a_hidden_section_still_changes_the_graphics_state() {
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >>",
        &format!("/OC /MC0 BDC\nq 2 0 0 2 0 0 cm 1 0 0 rg Q\n{TOP_LEFT}EMC\n{BOTTOM_RIGHT}"),
        &[group("")],
    );
    let recorder = draw(file);
    assert!(!recorder.painted_top_left(), "the hidden square was painted");
    assert!(
        recorder.state_calls >= 3,
        "state operators inside the hidden section did not reach the backend: {} calls",
        recorder.state_calls
    );
}

/// Text inside a hidden section is not shown, and therefore is not extracted either —
/// what is drawn and what comes out of `inspect text` are the same content stream, and a
/// layer that is off is not on the page in either sense.
#[test]
fn text_inside_a_hidden_section_is_not_shown() {
    let content =
        format!("/OC /MC0 BDC\nBT /F1 24 Tf 1 0 0 1 20 60 Tm (HIDDEN) Tj ET\nEMC\n{BOTTOM_RIGHT}");
    let file = page(
        "/OCProperties << /OCGs [5 0 R] /D << /OFF [5 0 R] >> >>",
        "/Properties << /MC0 5 0 R >> /Font << /F1 6 0 R >>",
        &content,
        &[group(""), "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()],
    );
    let recorder = draw(file);
    assert_eq!(recorder.text_runs, 0, "text in a hidden layer reached the backend");
    assert!(recorder.painted_bottom_right(), "the rest of the page went with it");
}
