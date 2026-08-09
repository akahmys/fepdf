use crate::graphics::{BlendMode, LineCap, LineJoin};
use crate::{FromPdfObject, Object};

/// PDF External Graphics State (ISO 32000-2:2020 Clause 8.4.5)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "8.4.5")]
pub struct PdfExtGState {
    #[pdf_key("LW")]
    pub line_width: Option<f64>,
    #[pdf_key("LC")]
    pub line_cap: Option<LineCap>,
    #[pdf_key("LJ")]
    pub line_join: Option<LineJoin>,
    #[pdf_key("ML")]
    pub miter_limit: Option<f64>,
    #[pdf_key("D")]
    pub dash: Option<Object>, // Array: [dash_array, dash_phase]
    #[pdf_key("BM")]
    pub blend_mode: Option<BlendMode>,
    #[pdf_key("CA")]
    pub stroke_alpha: Option<f64>,
    #[pdf_key("ca")]
    pub fill_alpha: Option<f64>,
    #[pdf_key("Font")]
    pub font: Option<Object>, // Array: [font_handle, size]
}
