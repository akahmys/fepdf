//! Page decorations, Bates numbering, annotations, measurement scale, and form fields.

use super::page::execute_single_op;
use fepdf::{
    AnnotationKind, AnnotationSpec, DecorationPosition, FormFieldSpec, FormValue, MeasurementScale,
    Operation, PageSelection,
};
use schemars::JsonSchema;
use serde::Deserialize;

/// Arguments for adding a page decoration (header/footer/watermark).
#[derive(Deserialize, JsonSchema)]
pub struct AddPageDecorationArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Selection of pages (e.g. "all", "1", "1-3"). Default: "all".
    pub pages: Option<String>,
    /// Text to render.
    pub text: String,
    /// Position ("top_left", "top_center", "top_right", "bottom_left", "bottom_center", "bottom_right").
    pub position: String,
}

/// Arguments for applying Bates numbering.
#[derive(Deserialize, JsonSchema)]
pub struct ApplyBatesNumberingArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Selection of pages (e.g. "all", "1-5"). Default: "all".
    pub pages: Option<String>,
    /// Prefix string (e.g. "CONFIDENTIAL-").
    pub prefix: Option<String>,
    /// Starting integer number.
    pub start_number: Option<u64>,
    /// Digit width with zero-padding (e.g. 6).
    pub digits: Option<usize>,
    /// Position ("top_left", "top_center", "top_right", "bottom_left", "bottom_center", "bottom_right").
    pub position: Option<String>,
}

/// Arguments for adding an annotation.
#[derive(Deserialize, JsonSchema)]
pub struct AddAnnotationArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Target 0-based page index.
    pub page: usize,
    /// Bounding rectangle `[x0, y0, x1, y1]`.
    pub rect: [f32; 4],
    /// Text content or comment.
    pub contents: String,
    /// Annotation type ("link", "highlight", "text").
    pub kind: Option<String>,
}

/// Arguments for setting a measurement scale.
#[derive(Deserialize, JsonSchema)]
pub struct SetMeasurementScaleArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Target 0-based page index.
    pub page: usize,
    /// Scale ratio factor (e.g. 0.01).
    pub scale_ratio: f32,
    /// Unit label (e.g. "mm", "m", "in").
    pub unit_label: String,
}

/// Arguments for setting an AcroForm field value.
#[derive(Deserialize, JsonSchema)]
pub struct SetFormFieldValueArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Form field name.
    pub field_name: String,
    /// String value to set.
    pub value_text: Option<String>,
    /// Boolean checkbox value to set.
    pub value_bool: Option<bool>,
}

fn parse_pos(pos: &str) -> DecorationPosition {
    match pos.to_lowercase().as_str() {
        "top_left" | "header_left" => DecorationPosition::TopLeft,
        "top_right" | "header_right" => DecorationPosition::TopRight,
        "top_center" | "header_center" => DecorationPosition::TopCenter,
        "bottom_left" | "footer_left" => DecorationPosition::BottomLeft,
        "bottom_right" | "footer_right" => DecorationPosition::BottomRight,
        _ => DecorationPosition::BottomCenter,
    }
}

/// Implementation of the add_page_decoration tool.
pub fn add_page_decoration_impl(args: AddPageDecorationArgs) -> Result<String, String> {
    let position = parse_pos(&args.position);
    let pages = match args.pages.as_deref() {
        Some(s) if s.contains('-') => {
            let (start, end) = s.split_once('-').unwrap_or(("1", "1"));
            let s_idx: usize = start.parse().unwrap_or(1);
            let e_idx: usize = end.parse().unwrap_or(1);
            PageSelection::Indices((s_idx.saturating_sub(1)..=e_idx.saturating_sub(1)).collect())
        }
        Some(s) if s.parse::<usize>().is_ok() => {
            PageSelection::Single(s.parse::<usize>().unwrap_or(1).saturating_sub(1))
        }
        _ => PageSelection::All,
    };
    let op = Operation::AddPageDecoration { pages, text: args.text, position };
    execute_single_op(&args.input_path, &args.output_path, op, "Page decoration added")
}

/// Implementation of the apply_bates_numbering tool.
pub fn apply_bates_numbering_impl(args: ApplyBatesNumberingArgs) -> Result<String, String> {
    let position = parse_pos(args.position.as_deref().unwrap_or("bottom_right"));
    let pages = PageSelection::All;
    let op = Operation::ApplyBatesNumbering {
        pages,
        prefix: args.prefix.unwrap_or_default(),
        start_number: args.start_number.unwrap_or(1),
        digits: args.digits.unwrap_or(6),
        position,
    };
    execute_single_op(&args.input_path, &args.output_path, op, "Bates numbering applied")
}

/// Implementation of the add_annotation tool.
pub fn add_annotation_impl(args: AddAnnotationArgs) -> Result<String, String> {
    let kind = match args.kind.as_deref() {
        Some("link") => AnnotationKind::Link { destination_page: args.page, url: None },
        Some("highlight") => AnnotationKind::Highlight { color_rgb: [1.0, 1.0, 0.0] },
        _ => AnnotationKind::TextComment { contents: args.contents },
    };
    let spec = AnnotationSpec { page: args.page, rect: args.rect, kind };
    let op = Operation::AddAnnotation(spec);
    execute_single_op(&args.input_path, &args.output_path, op, "Annotation added")
}

/// Implementation of the set_measurement_scale tool.
pub fn set_measurement_scale_impl(args: SetMeasurementScaleArgs) -> Result<String, String> {
    let scale = MeasurementScale {
        page: args.page,
        scale_ratio: args.scale_ratio,
        unit_label: args.unit_label,
    };
    let op = Operation::SetMeasurementScale(scale);
    execute_single_op(&args.input_path, &args.output_path, op, "Measurement scale (/Measure) set")
}

/// Implementation of the set_form_field_value tool.
pub fn set_form_field_value_impl(args: SetFormFieldValueArgs) -> Result<String, String> {
    let val = if let Some(b) = args.value_bool {
        FormValue::Boolean(b)
    } else {
        FormValue::Text(args.value_text.unwrap_or_default())
    };
    let spec = FormFieldSpec { name: args.field_name, value: val };
    let op = Operation::SetFormFieldValue(spec);
    execute_single_op(&args.input_path, &args.output_path, op, "Form field value updated")
}
