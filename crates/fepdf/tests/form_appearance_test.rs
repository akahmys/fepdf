//! Setting a field value, and what the reader then sees (12.7.4.3).
//!
//! Writing the value used to be the whole of it: the value went in and
//! `/NeedAppearances true` went beside it, which is a producer telling the reader to work
//! the appearance out — using an entry PDF 2.0 lists among the features it **deprecates**
//! (0.3), under an engine whose own rule is not to write what 2.0 deprecates.
//!
//! So the test is a round trip and not an inspection of the dictionary: the value is set,
//! the page is drawn, and what decides is whether the text reached the backend.

use fepdf::{IngestionOptions, PdfDocument};
use fepdf_content::{
    BlendMode, Color, FallbackFontType, Paint, PixelFormat, RenderBackend, SMaskData, ShadingSpec,
    StrokeStyle, TextGlyph, TextState, WindingRule,
};
use fepdf_doc::operation::Operation;
use fepdf_model::document::extensions::{FormFieldSpec, FormValue};
use fepdf_model::graphics::TextRenderingMode;
use kurbo::{Affine, BezPath};
use std::sync::Arc;

/// The text the page drew, and where each run started.
#[derive(Default)]
struct Drawn {
    text: String,
    first_x: Option<f64>,
}

impl RenderBackend for Drawn {
    fn show_text(
        &mut self,
        glyphs: &[TextGlyph],
        _size: f64,
        transform: Affine,
        _state: TextState,
        _op_index: usize,
    ) {
        for glyph in glyphs {
            self.text.push_str(&glyph.unicode);
        }
        self.first_x.get_or_insert(transform.as_coeffs()[4]);
    }
    fn transform(&mut self, _transform: Affine) {}
    fn set_transform(&mut self, _transform: Affine) {}
    fn push_state(&mut self) {}
    fn pop_state(&mut self) {}
    fn fill_path(&mut self, _path: &BezPath, _color: &Color, _rule: WindingRule) {}
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
}

/// A one-page form with one text field, its widget on the page, and Helvetica in `/DR`.
fn form(extra_acro: &str, quadding: i64) -> Vec<u8> {
    let content = "";
    let bodies = [
        format!(
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] \
             /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 6 0 R >> >> {extra_acro} >> >>"
        ),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 100] /Annots [5 0 R] \
          /Contents 4 0 R >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        format!(
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (Name) /Q {quadding} \
             /Rect [20 40 280 70] /P 3 0 R >>"
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Name /Helv >>".to_string(),
    ];
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

/// Sets the field, then reports the document and what its widget's appearance draws.
fn fill(file: Vec<u8>, value: &str) -> (PdfDocument, Drawn) {
    let mut doc = PdfDocument::open_with_options(file.into(), &IngestionOptions::default())
        .expect("the fixture opens");
    doc.apply(Operation::SetFormFieldValue(FormFieldSpec {
        name: "Name".to_string(),
        value: FormValue::Text(value.to_string()),
    }))
    .expect("the value is set");

    // The appearance is a form XObject on the widget, and executing it is exactly what a
    // reader does with it — so the assertion is on marks reaching a backend, not on the
    // bytes of a dictionary.
    let mut drawn = Drawn::default();
    {
        let arena = doc.inner().arena();
        let widget = arena.get_object(fepdf_model::Handle::new(5)).expect("the widget");
        let stream = normal_appearance(arena, &widget).expect("an /AP /N was written");
        let resources = arena
            .get_object(stream)
            .and_then(|object| object.as_dict_handle())
            .and_then(|dh| arena.get_dict(dh))
            .and_then(|dict| dict.get(&arena.name("Resources")).cloned())
            .and_then(|entry| entry.resolve(arena).as_dict_handle())
            .expect("the appearance carries /DR as its resources");
        let mut interpreter =
            fepdf::Interpreter::new(&mut drawn, doc.inner(), resources, Affine::IDENTITY);
        interpreter.execute(stream).expect("the appearance draws");
    }
    (doc, drawn)
}

/// The widget's `/AP` `/N` stream handle.
fn normal_appearance(
    arena: &fepdf_model::arena::PdfArena,
    widget: &fepdf_model::Object,
) -> Option<fepdf_model::Handle<fepdf_model::Object>> {
    let dict = arena.get_dict(widget.as_dict_handle()?)?;
    let ap = arena.get_dict(dict.get(&arena.name("AP"))?.resolve(arena).as_dict_handle()?)?;
    ap.get(&arena.name("N"))?.as_reference()
}

/// **The round trip.** A value set into a text field is drawn by the appearance the
/// writer built, without `/NeedAppearances` and without a reader that runs scripts.
#[test]
fn a_value_set_into_a_text_field_is_drawn() {
    let (_, drawn) = fill(form("", 0), "HELLO");
    assert!(drawn.text.contains("HELLO"), "the appearance drew {:?}", drawn.text);
}

/// The deprecated entry is not written. 0.3 lists `/NeedAppearances` among the features
/// PDF 2.0 deprecates, and this engine's rule is not to write those.
#[test]
fn need_appearances_is_not_written() {
    let (doc, _) = fill(form("", 0), "HELLO");
    let form = doc.inner().catalog().expect("the catalogue").acro_form.expect("a form");
    assert_eq!(
        form.need_appearances, None,
        "a deprecated entry was written to avoid building the appearance"
    );
}

/// Quadding is honoured, which needs the width of the text and therefore the font from
/// `/DR`. Right-justified text starts further right than left-justified text does.
#[test]
fn quadding_moves_the_text_because_the_font_gives_its_width() {
    let (_, left) = fill(form("", 0), "HELLO");
    let (_, right) = fill(form("", 2), "HELLO");
    let (left_x, right_x) = (left.first_x.expect("drawn"), right.first_x.expect("drawn"));
    assert!(
        right_x > left_x + 50.0,
        "right-justified text should start well right of left-justified: {left_x} vs {right_x}"
    );
}

/// **Setting a value in a form that calculates is reported.** 12.6.3 says the effects of
/// a field action are limited only by the action itself and may modify anything, and
/// names this case: changing a value triggers calculations for other fields. This engine
/// does not run them, which 6.3.2.1 permits — and a caller that is not told gets a
/// document whose fields disagree.
#[test]
fn a_form_that_calculates_says_the_scripts_did_not_run() {
    let (doc, _) = fill(form("/CO [5 0 R]", 0), "HELLO");
    let said: Vec<String> =
        doc.decisions().iter().map(|d| format!("{} {}", d.clause, d.found)).collect();
    assert!(
        said.iter().any(|d| d.starts_with("12.6.3") && d.contains("calculation order")),
        "nothing said the calculation was skipped: {said:?}"
    );
}

/// A form with no calculation order says nothing, because nothing was skipped. A decision
/// that fires on every form would be a constant rather than a signal (ADR-0008).
#[test]
fn a_form_that_does_not_calculate_says_nothing() {
    let (doc, _) = fill(form("", 0), "HELLO");
    let said: Vec<String> = doc.decisions().iter().map(|d| d.clause.to_string()).collect();
    assert!(!said.iter().any(|c| c == "12.6.3"), "{said:?}");
}
