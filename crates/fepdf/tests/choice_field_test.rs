//! Choice fields (/FT /Ch) reading, value setting, and appearance generation (ISO 32000-2 12.7.4.4).

use fepdf::{ChoiceOption, FormFieldSpec, FormValue, InteractiveReport, PdfDocument};
use fepdf_content::{
    BlendMode, Color, FallbackFontType, Paint, PixelFormat, RenderBackend, SMaskData, ShadingSpec,
    StrokeStyle, TextGlyph, TextState, WindingRule,
};
use fepdf_doc::operation::Operation;
use fepdf_model::graphics::TextRenderingMode;
use kurbo::{Affine, BezPath};
use std::sync::Arc;

#[derive(Default)]
struct Drawn {
    text: String,
}

impl RenderBackend for Drawn {
    fn show_text(
        &mut self,
        glyphs: &[TextGlyph],
        _size: f64,
        _transform: Affine,
        _state: TextState,
        _op_index: usize,
    ) {
        for glyph in glyphs {
            self.text.push_str(&glyph.unicode);
        }
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

fn choice_form(field_extra: &str) -> Vec<u8> {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] \
         /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 6 0 R >> >> >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 100] /Annots [5 0 R] \
          /Contents 4 0 R >>"
            .to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
        format!(
            "<< /Type /Annot /Subtype /Widget /FT /Ch /T (Colors) \
             /Rect [20 40 280 70] /P 3 0 R {field_extra} >>"
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Name /Helv >>".to_string(),
    ];
    assemble(&bodies)
}

#[test]
fn test_read_combo_box_simple_options() {
    let pdf = choice_form("/Ff 131072 /Opt [(Red) (Green) (Blue)] /V (Green) /I [1]");
    let report = InteractiveReport::survey(&pdf).expect("reads");
    assert_eq!(report.form.fields, 1);
    let field = &report.form.terminal[0];
    assert_eq!(field.name.as_deref(), Some("Colors"));
    assert_eq!(field.field_type.as_deref(), Some("Ch"));
    assert!(field.is_combo());
    assert!(!field.is_multiselect());
    assert_eq!(
        field.options,
        vec![
            ChoiceOption::simple("Red"),
            ChoiceOption::simple("Green"),
            ChoiceOption::simple("Blue"),
        ]
    );
    assert_eq!(field.value.as_deref(), Some("Green"));
    assert_eq!(field.selected_indices, vec![1]);
}

#[test]
fn test_read_combo_box_export_display_pairs() {
    let pdf = choice_form(
        "/Ff 131072 /Opt [[(CA) (California)] [(NY) (New York)] [(TX) (Texas)]] /V (NY)",
    );
    let report = InteractiveReport::survey(&pdf).expect("reads");
    let field = &report.form.terminal[0];
    assert_eq!(
        field.options,
        vec![
            ChoiceOption::pair("CA", "California"),
            ChoiceOption::pair("NY", "New York"),
            ChoiceOption::pair("TX", "Texas"),
        ]
    );
    assert_eq!(field.value.as_deref(), Some("NY"));
    assert_eq!(field.selected_indices, vec![1]);
}

#[test]
fn test_read_list_box_multiselect() {
    let pdf = choice_form("/Ff 2097152 /Opt [(Item0) (Item1) (Item2) (Item3)] /I [0 2]");
    let report = InteractiveReport::survey(&pdf).expect("reads");
    let field = &report.form.terminal[0];
    assert!(!field.is_combo());
    assert!(field.is_multiselect());
    assert_eq!(field.options.len(), 4);
    assert_eq!(field.selected_indices, vec![0, 2]);
}

#[test]
fn test_choice_field_inheritance() {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R] \
         /DA (/Helv 10 Tf 0 g) >> >>"
            .to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 100] /Annots [6 0 R] \
          /Contents 4 0 R >>"
            .to_string(),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
        // Parent non-terminal node carrying /FT and /Opt
        "<< /T (Parent) /FT /Ch /Ff 131072 /Opt [(First) (Second) (Third)] /Kids [6 0 R] >>"
            .to_string(),
        // Child terminal widget node
        "<< /Type /Annot /Subtype /Widget /T (Child) /V (Second) \
         /Rect [20 40 280 70] /P 3 0 R >>"
            .to_string(),
    ];
    let pdf = assemble(&bodies);
    let report = InteractiveReport::survey(&pdf).expect("reads");
    assert_eq!(report.form.fields, 1);
    let field = &report.form.terminal[0];
    assert_eq!(field.qualified_name.as_deref(), Some("Parent.Child"));
    assert_eq!(field.field_type.as_deref(), Some("Ch"));
    assert_eq!(field.options.len(), 3);
    assert_eq!(field.value.as_deref(), Some("Second"));
    assert_eq!(field.selected_indices, vec![1]);
}

#[test]
fn test_set_choice_field_value_and_appearance() {
    let pdf = choice_form("/Ff 131072 /Opt [(Small) (Medium) (Large)] /V (Small)");
    let mut doc = PdfDocument::open(pdf.into()).expect("opens");
    doc.apply(Operation::SetFormFieldValue(FormFieldSpec {
        name: "Colors".to_string(),
        value: FormValue::Choice("Large".to_string()),
    }))
    .expect("sets value");

    let val = fepdf::field_value(doc.inner(), "Colors");
    assert_eq!(val.as_deref(), Some("Large"));

    let out_dir = std::path::Path::new("target/tmp");
    let _ = std::fs::create_dir_all(out_dir);
    let out_path = out_dir.join("test_choice_set.pdf");
    doc.save_as_version(&out_path, "2.0").expect("saves");
    let saved = std::fs::read(&out_path).expect("reads saved file");
    let report = InteractiveReport::survey(&saved).expect("reads");
    let field = &report.form.terminal[0];
    assert_eq!(field.value.as_deref(), Some("Large"));
    assert_eq!(field.selected_indices, vec![2]);

    let mut drawn = Drawn::default();
    doc.render_page(0, &mut drawn, Affine::IDENTITY).expect("renders");
    assert_eq!(drawn.text, "Large");
}

#[test]
fn test_set_choice_field_paired_display_appearance() {
    let pdf = choice_form(
        "/Ff 131072 /Opt [[(US) (United States)] [(JP) (Japan)] [(DE) (Germany)]] /V (US)",
    );
    let mut doc = PdfDocument::open(pdf.into()).expect("opens");
    doc.apply(Operation::SetFormFieldValue(FormFieldSpec {
        name: "Colors".to_string(),
        value: FormValue::Choice("JP".to_string()),
    }))
    .expect("sets value");

    let val = fepdf::field_value(doc.inner(), "Colors");
    assert_eq!(val.as_deref(), Some("JP"));

    let out_dir = std::path::Path::new("target/tmp");
    let _ = std::fs::create_dir_all(out_dir);
    let out_path = out_dir.join("test_choice_pair.pdf");
    doc.save_as_version(&out_path, "2.0").expect("saves");
    let saved = std::fs::read(&out_path).expect("reads saved file");
    let report = InteractiveReport::survey(&saved).expect("reads");
    let field = &report.form.terminal[0];
    assert_eq!(field.selected_indices, vec![1]);

    let mut drawn = Drawn::default();
    doc.render_page(0, &mut drawn, Affine::IDENTITY).expect("renders");
    assert_eq!(drawn.text, "Japan");
}
