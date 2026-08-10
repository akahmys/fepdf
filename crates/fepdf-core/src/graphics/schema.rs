use crate::graphics::{BlendMode, LineCap, LineJoin};
use crate::{FromPdfObject, Object};

/// PDF External Graphics State (ISO 32000-2:2020 Clause 8.4.5)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "8.4.5")]
pub struct PdfExtGState {
    #[pdf_key("LW")]
    /// `/LW`: stroke width.
    pub line_width: Option<f64>,
    #[pdf_key("LC")]
    /// `/LC`: line cap style.
    pub line_cap: Option<LineCap>,
    #[pdf_key("LJ")]
    /// `/LJ`: line join style.
    pub line_join: Option<LineJoin>,
    #[pdf_key("ML")]
    /// `/ML`: miter limit.
    pub miter_limit: Option<f64>,
    #[pdf_key("D")]
    /// `/D`: dash array and phase.
    pub dash: Option<Object>, // Array: [dash_array, dash_phase]
    #[pdf_key("BM")]
    /// `/BM`: blend mode.
    pub blend_mode: Option<BlendMode>,
    #[pdf_key("CA")]
    /// `/CA`: stroking alpha constant.
    pub stroke_alpha: Option<f64>,
    #[pdf_key("ca")]
    /// `/ca`: non-stroking alpha constant.
    pub fill_alpha: Option<f64>,
    #[pdf_key("Font")]
    /// `/Font`: font resource and size.
    pub font: Option<Object>, // Array: [font_handle, size]
}
