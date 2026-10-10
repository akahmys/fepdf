//! Page decorations, Bates numbering, annotations, measurement scale, and form fields.

use super::page::execute_single_op;
use crate::McpError;
use fepdf::{
    AnnotationKind, AnnotationSpec, DecorationPosition, FormFieldSpec, FormValue, MeasurementScale,
    Operation, ShapeForm,
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
    /// Selection of pages, **counting from 1**: "all", "1", "1-3". Default: "all".
    pub pages: Option<String>,
    /// Text to render.
    pub text: String,
    /// Position ("top_left", "top_center", "top_right", "bottom_left", "bottom_center", "bottom_right").
    pub position: String,
    /// Name of an existing optional content layer to put the decoration in, so a reader
    /// can turn it off. The layer must already exist — see `update_layers`.
    pub layer: Option<String>,
}

/// Arguments for applying Bates numbering.
#[derive(Deserialize, JsonSchema)]
pub struct ApplyBatesNumberingArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Selection of pages, **counting from 1**: "all", "1-5". Default: "all".
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
    /// Bounding rectangle `[x0, y0, x1, y1]`, in points from the page's lower left.
    pub rect: [f32; 4],
    /// What a note, text box, typewriter or callout says.
    #[serde(default)]
    pub contents: String,
    /// One of `note` (the default; `text` is the same), `highlight`, `underline`,
    /// `strike_out`, `squiggly`, `text_box`, `typewriter`, `callout`, `ink`, `rectangle`,
    /// `ellipse`, `line`, `stamp` or `link`. Any other name is refused.
    pub kind: Option<String>,
    /// RGB colour, each from 0 to 1, for the marks, ink and shapes. Default black, and
    /// yellow for a highlight.
    pub color: Option<[f32; 3]>,
    /// The size a text box, typewriter or callout is set at, in points. Default 12.
    pub font_size: Option<f32>,
    /// Where a callout's line points, on the page.
    pub points_at: Option<[f32; 2]>,
    /// An ink annotation's strokes, each the points it passes through on the page.
    pub strokes: Option<Vec<Vec<[f32; 2]>>>,
    /// A line's two ends, on the page.
    pub from: Option<[f32; 2]>,
    /// See `from`.
    pub to: Option<[f32; 2]>,
    /// The width of ink or a shape's outline, in points. Default 1.
    pub width: Option<f32>,
    /// Where a link goes on the web.
    pub url: Option<String>,
    /// Which page a link goes to, counting from zero, when it has no `url`.
    pub destination_page: Option<usize>,
    /// A JPEG file on disk to put in a stamp.
    pub stamp_path: Option<String>,
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
    /// How many units one point on the page stands for: a 1:100 drawing measured in
    /// metres is 0.0254 / 72 * 100, about 0.0353.
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
pub fn add_page_decoration_impl(args: AddPageDecorationArgs) -> Result<String, McpError> {
    let position = parse_pos(&args.position);
    let pages = super::parse_selection(args.pages.as_deref())?;
    let op = Operation::AddPageDecoration { pages, text: args.text, position, layer: args.layer };
    execute_single_op(&args.input_path, &args.output_path, op, "Page decoration added")
}

/// Implementation of the apply_bates_numbering tool.
pub fn apply_bates_numbering_impl(args: ApplyBatesNumberingArgs) -> Result<String, McpError> {
    let position = parse_pos(args.position.as_deref().unwrap_or("bottom_right"));
    let pages = super::parse_selection(args.pages.as_deref())?;
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
pub fn add_annotation_impl(args: AddAnnotationArgs) -> Result<String, McpError> {
    let kind = annotation_kind(&args)?;
    let spec =
        AnnotationSpec { page: args.page, rect: args.rect, kind, by: fepdf::Authorship::default() };
    let op = Operation::AddAnnotation(spec);
    execute_single_op(&args.input_path, &args.output_path, op, "Annotation added")
}

/// The kind the arguments name, with what it needs taken from them.
///
/// **A name this does not know is refused.** It was read as a note, so `"underline"`
/// wrote a sticky note and answered that it had added an annotation.
fn annotation_kind(args: &AddAnnotationArgs) -> Result<AnnotationKind, McpError> {
    let ink = args.color.unwrap_or([0.0, 0.0, 0.0]);
    let width = args.width.unwrap_or(1.0);
    let size = args.font_size.unwrap_or(12.0);
    let words = || args.contents.clone();
    let needs = |what: &str| {
        format!("a {} annotation needs `{what}`", args.kind.as_deref().unwrap_or("note"))
    };
    let shape = |form| AnnotationKind::Shape { form, color_rgb: ink, width };
    Ok(match args.kind.as_deref().unwrap_or("note") {
        "note" | "text" => AnnotationKind::TextComment { contents: words() },
        "highlight" => {
            AnnotationKind::Highlight { color_rgb: args.color.unwrap_or([1.0, 1.0, 0.0]) }
        }
        "underline" => AnnotationKind::Underline { color_rgb: ink },
        "strike_out" => AnnotationKind::StrikeOut { color_rgb: ink },
        "squiggly" => AnnotationKind::Squiggly { color_rgb: ink },
        "text_box" => AnnotationKind::TextBox { contents: words(), font_size: size },
        "typewriter" => AnnotationKind::Typewriter { contents: words(), font_size: size },
        "callout" => AnnotationKind::Callout {
            contents: words(),
            font_size: size,
            points_at: args.points_at.ok_or_else(|| needs("points_at"))?,
        },
        "ink" => AnnotationKind::Ink {
            strokes: args.strokes.clone().ok_or_else(|| needs("strokes"))?,
            color_rgb: ink,
            width,
        },
        "rectangle" => shape(ShapeForm::Rectangle),
        "ellipse" => shape(ShapeForm::Ellipse),
        "line" => shape(ShapeForm::Line {
            from: args.from.ok_or_else(|| needs("from"))?,
            to: args.to.ok_or_else(|| needs("to"))?,
        }),
        "stamp" => {
            let path = args.stamp_path.as_deref().ok_or_else(|| needs("stamp_path"))?;
            let picture = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            AnnotationKind::Stamp { stamp_image_bytes: picture }
        }
        "link" => AnnotationKind::Link {
            destination_page: args.destination_page.unwrap_or(args.page),
            url: args.url.clone(),
        },
        other => return Err(format!("no annotation kind is called {other:?}").into()),
    })
}

/// Implementation of the set_measurement_scale tool.
pub fn set_measurement_scale_impl(args: SetMeasurementScaleArgs) -> Result<String, McpError> {
    let scale = MeasurementScale {
        page: args.page,
        scale_ratio: args.scale_ratio,
        unit_label: args.unit_label,
    };
    let op = Operation::SetMeasurementScale(scale);
    execute_single_op(&args.input_path, &args.output_path, op, "Measurement scale (/Measure) set")
}

/// Implementation of the set_form_field_value tool.
pub fn set_form_field_value_impl(args: SetFormFieldValueArgs) -> Result<String, McpError> {
    let val = if let Some(b) = args.value_bool {
        FormValue::Boolean(b)
    } else {
        FormValue::Text(args.value_text.unwrap_or_default())
    };
    let spec = FormFieldSpec { name: args.field_name, value: val };
    let op = Operation::SetFormFieldValue(spec);
    execute_single_op(&args.input_path, &args.output_path, op, "Form field value updated")
}

/// Arguments for `add_form_field`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AddFormFieldArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// The page the widget goes on, counting from zero.
    pub page: usize,
    /// Where on it, as [left, bottom, right, top] in points from the bottom-left corner.
    pub rect: [f64; 4],
    /// The field's name, which is what filling it names.
    pub name: String,
    /// What a reader is told the field is for. Required: a field without one is a
    /// Matterhorn failure this engine reports.
    pub tooltip: String,
    /// One of `text`, `text_area`, `password`, `check_box`, `radio_button`,
    /// `push_button`, `combo_box`, `list_box`, `signature`.
    pub kind: String,
    /// What it holds to begin with, for a text or choice field; the caption for a push
    /// button; `true` or `false` for a box or a radio.
    pub value: Option<String>,
    /// What a choice field offers.
    pub options: Option<Vec<String>>,
}

/// Implementation of the add_form_field tool.
pub fn add_form_field_impl(args: AddFormFieldArgs) -> Result<String, McpError> {
    let value = args.value.clone().unwrap_or_default();
    let options = args.options.clone().unwrap_or_default();
    let on = value.eq_ignore_ascii_case("true");
    let kind = match args.kind.as_str() {
        "text" => fepdf::FieldKind::Text { value },
        "text_area" => fepdf::FieldKind::TextArea { value },
        "password" => fepdf::FieldKind::Password,
        "check_box" => fepdf::FieldKind::CheckBox { on },
        "radio_button" => fepdf::FieldKind::RadioButton { group: args.name.clone(), on },
        "push_button" => fepdf::FieldKind::PushButton { caption: value },
        "combo_box" => fepdf::FieldKind::ComboBox { options, value },
        "list_box" => fepdf::FieldKind::ListBox { options, value },
        "signature" => fepdf::FieldKind::Signature,
        other => return Err(format!("no field kind is called {other:?}").into()),
    };
    let op = Operation::AddFormField(fepdf::NewField {
        page: args.page,
        rect: (args.rect[0], args.rect[1], args.rect[2], args.rect[3]),
        name: args.name,
        tooltip: args.tooltip,
        kind,
    });
    execute_single_op(&args.input_path, &args.output_path, op, "Form field created")
}

/// Arguments for `set_calculation_order`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetCalculationOrderArgs {
    /// Path to input PDF file.
    pub input_path: String,
    /// Path to output PDF file.
    pub output_path: String,
    /// Every field that calculates, by fully qualified name, in the order they are to be
    /// recalculated — each once, and no field that does not calculate.
    pub fields: Vec<String>,
}

/// Implementation of the set_calculation_order tool.
pub fn set_calculation_order_impl(args: SetCalculationOrderArgs) -> Result<String, McpError> {
    let op = Operation::SetCalculationOrder(args.fields);
    execute_single_op(&args.input_path, &args.output_path, op, "Calculation order set")
}
