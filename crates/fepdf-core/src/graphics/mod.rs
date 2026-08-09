//! PDF Graphics State Constants & Types (ISO 32000-2:2020 Clause 8)

pub mod schema;

use crate::object::{FromPdfObject, Object};
use crate::{PdfArena, PdfError, PdfResult};
use kurbo::Affine;
use serde::{Deserialize, Serialize};

/// PDF Color representation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Color {
    Gray(f64),
    Rgb(f64, f64, f64),
    Cmyk(f64, f64, f64, f64),
    /// Lab color space (Placeholder)
    Lab(f64, f64, f64),
}

pub use crate::color::ColorSpaceKind;

impl Color {
    /// Normalizes the color to sRGB.
    pub fn to_rgb(&self) -> Self {
        match *self {
            Color::Rgb(..) => *self,
            Color::Gray(g) => Color::Rgb(g, g, g),
            Color::Cmyk(c, m, y, k) => {
                let r = (1.0 - c) * (1.0 - k);
                let g = (1.0 - m) * (1.0 - k);
                let b = (1.0 - y) * (1.0 - k);
                Color::Rgb(r, g, b)
            }
            Color::Lab(l, a, b) => {
                // 1. Convert CIELAB to XYZ using D65 Standard Illuminant
                let y = (l + 16.0) / 116.0;
                let x = a / 500.0 + y;
                let z = y - b / 200.0;

                let f = |val: f64| {
                    if val > 6.0 / 29.0 {
                        val.powi(3)
                    } else {
                        (3.0 * (6.0 / 29.0) * (6.0 / 29.0)) * (val - 4.0 / 29.0)
                    }
                };

                let x = 0.950489 * f(x);
                let y = 1.000000 * f(y);
                let z = 1.088840 * f(z);

                // 2. Transform XYZ to Linear sRGB using precise BT.709-6 matrix
                let r_lin = x * 3.2404542 + y * -1.5371385 + z * -0.4985314;
                let g_lin = x * -0.9692660 + y * 1.8760108 + z * 0.0415560;
                let b_lin = x * 0.0556434 + y * -0.2040259 + z * 1.0572252;

                // 3. Apply standard sRGB non-linear gamma companding
                let compand = |c: f64| {
                    let c_clamp = c.clamp(0.0, 1.0);
                    if c_clamp <= 0.0031308 {
                        12.92 * c_clamp
                    } else {
                        1.055 * c_clamp.powf(1.0 / 2.4) - 0.055
                    }
                };

                Color::Rgb(compand(r_lin), compand(g_lin), compand(b_lin))
            }
        }
    }
}

/// Standard PDF Blend Modes (ISO 32000-2 Table 141)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

/// Path Winding Rules (ISO 32000-2 Clause 8.5.3.3)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindingRule {
    NonZero,
    EvenOdd,
}

/// Line Cap Styles (ISO 32000-2 Table 53)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

impl LineCap {
    pub fn from_i64(val: i64) -> Self {
        match val {
            0 => Self::Butt,
            1 => Self::Round,
            2 => Self::Square,
            _ => Self::Butt,
        }
    }
}

/// Line Join Styles (ISO 32000-2 Table 54)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

impl LineJoin {
    pub fn from_i64(val: i64) -> Self {
        match val {
            0 => Self::Miter,
            1 => Self::Round,
            2 => Self::Bevel,
            _ => Self::Miter,
        }
    }
}

/// Stroke Style Parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrokeStyle {
    pub width: f64,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f64,
    pub dash_pattern: Option<(Vec<f64>, f64)>,
}

/// Image Pixel Formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    Gray8,
    Rgb8,
    Cmyk8,
    Rgba8,
    MonoMask,         // 1-bit stencil mask (0 means fill color, 1 means transparent)
    MonoMaskInverted, // 1-bit stencil mask inverted (1 means fill color, 0 means transparent)
}

/// Standard PDF 2D Transformation Matrix (ISO 32000-2 Clause 8.3.3)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Matrix(pub [f64; 6]);

impl Default for Matrix {
    fn default() -> Self {
        Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    }
}

impl Matrix {
    pub fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self([a, b, c, d, e, f])
    }

    pub fn as_affine(&self) -> Affine {
        Affine::new(self.0)
    }

    pub fn concat(&self, other: &Self) -> Self {
        let res = self.as_affine() * other.as_affine();
        Self(res.as_coeffs())
    }

    pub fn pre_concat(&self, other: &Self) -> Self {
        let res = other.as_affine() * self.as_affine();
        Self(res.as_coeffs())
    }
}

/// A simple axis-aligned rectangle (ISO 32000-2 Clause 7.3.6)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl Rect {
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    pub fn union(&self, other: &Self) -> Self {
        Self {
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
            x2: self.x2.max(other.x2),
            y2: self.y2.max(other.y2),
        }
    }

    pub fn width(&self) -> f64 {
        (self.x2 - self.x1).abs()
    }

    pub fn height(&self) -> f64 {
        (self.y2 - self.y1).abs()
    }
}

/// Graphics State Parameters (ISO 32000-2 Table 52)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicsState {
    pub ctm: Matrix,
    pub stroke_color: Color,
    pub fill_color: Color,
    pub stroke_style: StrokeStyle,
    pub fill_alpha: f64,
    pub stroke_alpha: f64,
    pub blend_mode: BlendMode,
    pub text_state: TextState,
    pub fill_color_space: ColorSpaceKind,
    pub stroke_color_space: ColorSpaceKind,
    pub clip_count: usize,
    pub smask: Option<crate::object::Object>,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: Matrix::default(),
            stroke_color: Color::Gray(0.0),
            fill_color: Color::Gray(0.0),
            stroke_style: StrokeStyle {
                width: 1.0,
                cap: LineCap::Butt,
                join: LineJoin::Miter,
                miter_limit: 10.0,
                dash_pattern: None,
            },
            fill_alpha: 1.0,
            stroke_alpha: 1.0,
            blend_mode: BlendMode::Normal,
            text_state: TextState::default(),
            fill_color_space: ColorSpaceKind::DeviceGray,
            stroke_color_space: ColorSpaceKind::DeviceGray,
            clip_count: 0,
            smask: None,
        }
    }
}

/// Text State Parameters (ISO 32000-2 Table 105)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextState {
    pub char_spacing: f64,
    pub word_spacing: f64,
    pub horizontal_scaling: f64,
    pub leading: f64,
    pub wmode: u8,
    pub font: Option<crate::object::PdfName>,
    pub font_size: f64,
    pub rendering_mode: TextRenderingMode,
    pub rise: f64,
    pub knockout: bool,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 100.0,
            leading: 0.0,
            font: None,
            font_size: 1.0,
            wmode: 0,
            rendering_mode: TextRenderingMode::Fill,
            rise: 0.0,
            knockout: true,
        }
    }
}

/// Text Rendering Modes (ISO 32000-2 Table 106)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextRenderingMode {
    Fill = 0,
    Stroke = 1,
    FillStroke = 2,
    Invisible = 3,
    FillClip = 4,
    StrokeClip = 5,
    FillStrokeClip = 6,
    Clip = 7,
}

impl From<i64> for TextRenderingMode {
    fn from(val: i64) -> Self {
        match val {
            0 => Self::Fill,
            1 => Self::Stroke,
            2 => Self::FillStroke,
            3 => Self::Invisible,
            4 => Self::FillClip,
            5 => Self::StrokeClip,
            6 => Self::FillStrokeClip,
            7 => Self::Clip,
            _ => Self::Fill,
        }
    }
}

/// Text Object Matrices (BT/ET Scope)
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TextMatrices {
    pub tm: Matrix,
    pub tlm: Matrix,
}

impl FromPdfObject for BlendMode {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let name = obj.resolve(arena).as_name().ok_or_else(|| PdfError::Parse {
            pos: 0,
            message: "Expected name for BlendMode".into(),
        })?;
        let name_owned = arena
            .get_name(name)
            .map(|n| n.as_str().to_string())
            .unwrap_or_else(|| "Normal".to_string());
        match name_owned.as_str() {
            "Normal" | "Compatible" => Ok(Self::Normal),
            "Multiply" => Ok(Self::Multiply),
            "Screen" => Ok(Self::Screen),
            "Overlay" => Ok(Self::Overlay),
            "Darken" => Ok(Self::Darken),
            "Lighten" => Ok(Self::Lighten),
            "ColorDodge" => Ok(Self::ColorDodge),
            "ColorBurn" => Ok(Self::ColorBurn),
            "HardLight" => Ok(Self::HardLight),
            "SoftLight" => Ok(Self::SoftLight),
            "Difference" => Ok(Self::Difference),
            "Exclusion" => Ok(Self::Exclusion),
            "Hue" => Ok(Self::Hue),
            "Saturation" => Ok(Self::Saturation),
            "Color" => Ok(Self::Color),
            "Luminosity" => Ok(Self::Luminosity),
            _ => Ok(Self::Normal),
        }
    }
}

impl FromPdfObject for LineCap {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let val = obj.resolve(arena).as_integer().ok_or_else(|| PdfError::Parse {
            pos: 0,
            message: "Expected integer for LineCap".into(),
        })?;
        match val {
            0 => Ok(Self::Butt),
            1 => Ok(Self::Round),
            2 => Ok(Self::Square),
            _ => Ok(Self::Butt),
        }
    }
}

impl FromPdfObject for LineJoin {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let val = obj.resolve(arena).as_integer().ok_or_else(|| PdfError::Parse {
            pos: 0,
            message: "Expected integer for LineJoin".into(),
        })?;
        match val {
            0 => Ok(Self::Miter),
            1 => Ok(Self::Round),
            2 => Ok(Self::Bevel),
            _ => Ok(Self::Miter),
        }
    }
}

impl FromPdfObject for TextRenderingMode {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let val = obj.resolve(arena).as_integer().ok_or_else(|| PdfError::Parse {
            pos: 0,
            message: "Expected integer for TextRenderingMode".into(),
        })?;
        Ok(Self::from(val))
    }
}

impl FromPdfObject for Matrix {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let handle = obj.resolve(arena).as_array().ok_or_else(|| PdfError::Parse {
            pos: 0,
            message: "Expected array for Matrix".into(),
        })?;
        let arr = arena
            .get_array(handle)
            .ok_or_else(|| PdfError::Arena("Missing array in arena".into()))?;
        if arr.len() != 6 {
            return Err(PdfError::Parse {
                pos: 0,
                message: format!("Expected 6 elements for Matrix, got {}", arr.len()).into(),
            });
        }
        let mut coeffs = [0.0; 6];
        for (i, item) in arr.iter().enumerate() {
            coeffs[i] = item.resolve(arena).as_f64().ok_or_else(|| PdfError::Parse {
                pos: 0,
                message: "Matrix element must be a number".into(),
            })?;
        }
        Ok(Self(coeffs))
    }
}

impl FromPdfObject for Rect {
    fn from_pdf_object(obj: Object, arena: &PdfArena) -> PdfResult<Self> {
        let handle = obj
            .resolve(arena)
            .as_array()
            .ok_or_else(|| PdfError::Parse { pos: 0, message: "Expected array for Rect".into() })?;
        let arr = arena
            .get_array(handle)
            .ok_or_else(|| PdfError::Arena("Missing array in arena".into()))?;
        if arr.len() != 4 {
            return Err(PdfError::Parse {
                pos: 0,
                message: format!("Expected 4 elements for Rect, got {}", arr.len()).into(),
            });
        }
        let mut coords = [0.0; 4];
        for (i, item) in arr.iter().enumerate() {
            coords[i] = item.resolve(arena).as_f64().ok_or_else(|| PdfError::Parse {
                pos: 0,
                message: "Rect element must be a number".into(),
            })?;
        }
        Ok(Self::new(coords[0], coords[1], coords[2], coords[3]))
    }
}
