//! PDF Graphics State Constants & Types (ISO 32000-2:2020 Clause 8)

/// Typed schema helpers for graphics dictionaries.
pub mod schema;

pub mod mesh;

pub use mesh::{FlatTriangle, MeshTriangle, TriangleMesh};

use crate::color::ResolvedColorSpace;
use crate::object::{FromPdfObject, Object};
use crate::{PdfArena, PdfError, PdfResult};
use kurbo::Affine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A color stop in a gradient or shading, specifying an offset in [0, 1] and a Color.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColorStop {
    /// Offset along the gradient axis in [0.0, 1.0].
    pub offset: f32,
    /// Color at this stop.
    pub color: Color,
}

impl ColorStop {
    /// Creates a new ColorStop.
    pub fn new(offset: f32, color: Color) -> Self {
        Self { offset: offset.clamp(0.0, 1.0), color }
    }
}

/// PDF Type 2 Axial Shading (ISO 32000-2 Section 8.7.4.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxialShading {
    /// Start and end coordinates `[x0, y0, x1, y1]`.
    pub coords: [f64; 4],
    /// Color stops defining the linear transition.
    pub stops: Vec<ColorStop>,
    /// Whether to extend the shading beyond the start and end points `[extend_start, extend_end]`.
    pub extend: [bool; 2],
}

/// PDF Type 3 Radial Shading (ISO 32000-2 Section 8.7.4.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadialShading {
    /// Starting circle center/radius and ending circle center/radius `[x0, y0, r0, x1, y1, r1]`.
    pub coords: [f64; 6],
    /// Color stops defining the radial transition.
    pub stops: Vec<ColorStop>,
    /// Whether to extend the shading beyond the start and end circles `[extend_start, extend_end]`.
    pub extend: [bool; 2],
}

/// Shading specification (ISO 32000-2 Section 8.7.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShadingSpec {
    /// Type 2 Axial (Linear) Shading.
    Axial(AxialShading),
    /// Type 3 Radial Shading.
    Radial(RadialShading),
    /// Types 4 to 7, decoded to triangles with a colour at each corner (8.7.4.5.5).
    ///
    /// **Not `MeshShadingSpec`**, which this variant used to hold: that is the argument
    /// to the `Operation` that *writes* a mesh — a type, a colour space name and the raw
    /// bytes — and reusing it here meant the read side had a variant it could never fill.
    /// Nothing constructed this before Phase P.
    Mesh(TriangleMesh),
}

/// Pattern specification (ISO 32000-2 Section 8.7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PatternSpec {
    /// Type 2 Shading Pattern.
    Shading(ShadingSpec),
    /// Type 1 Tiling Pattern.
    Tiling {
        /// Pattern cell bounding box `[min_x, min_y, max_x, max_y]`.
        bbox: [f64; 4],
        /// Horizontal spacing between pattern cells.
        x_step: f64,
        /// Vertical spacing between pattern cells.
        y_step: f64,
        /// Optional pattern transformation matrix.
        matrix: Option<Matrix>,
        /// Pattern content stream instructions/bytes.
        content_bytes: Vec<u8>,
    },
}

/// General paint style (Solid color or Pattern/Shading).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Paint {
    /// Solid color.
    Solid(Color),
    /// Pattern or Shading.
    Pattern(PatternSpec),
}

impl From<Color> for Paint {
    fn from(color: Color) -> Self {
        Paint::Solid(color)
    }
}

/// PDF Color representation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Color {
    /// DeviceGray: a single intensity in [0,1].
    Gray(f64),
    /// DeviceRGB: red, green and blue, each in [0,1].
    Rgb(f64, f64, f64),
    /// DeviceCMYK: cyan, magenta, yellow and black, each in [0,1].
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
                // 10.4.2.5: "red = 1.0 − min(1.0, cyan + black)", and the same for the
                // other two. The black component is *added* to each of the others and the
                // sum complemented.
                //
                // **This was `(1 − c) × (1 − k)`**, which is the textbook naive
                // conversion and is not what the standard says. The two agree only where
                // one of the pair is 0 or 1: at c = 0.5, k = 0.5 the clause gives 0 and
                // the product gives 0.25. 10.4.2.1 offers these algorithms to a processor
                // that is not ICC-enabled, which this one is not, so they are the
                // conformant answer rather than a stopgap.
                let channel = |ink: f64| 1.0 - (ink + k).clamp(0.0, 1.0);
                Color::Rgb(channel(c), channel(m), channel(y))
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
    /// Selects the source colour, ignoring the backdrop.
    Normal,
    /// Multiplies backdrop and source; always darkens.
    Multiply,
    /// Multiplies the complements; always lightens.
    Screen,
    /// Multiply or screen per backdrop, preserving highlights and shadows.
    Overlay,
    /// Selects the darker of backdrop and source.
    Darken,
    /// Selects the lighter of backdrop and source.
    Lighten,
    /// Brightens the backdrop to reflect the source.
    ColorDodge,
    /// Darkens the backdrop to reflect the source.
    ColorBurn,
    /// Multiply or screen per source; like a harsh spotlight.
    HardLight,
    /// Darkens or lightens per source; like a diffuse spotlight.
    SoftLight,
    /// Absolute difference of backdrop and source.
    Difference,
    /// Like `Difference` but lower in contrast.
    Exclusion,
    /// Source hue with backdrop saturation and luminosity.
    Hue,
    /// Source saturation with backdrop hue and luminosity.
    Saturation,
    /// Source hue and saturation with backdrop luminosity.
    Color,
    /// Source luminosity with backdrop hue and saturation.
    Luminosity,
}

/// Path Winding Rules (ISO 32000-2 Clause 8.5.3.3)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindingRule {
    /// Non-zero winding: inside where the crossing count is non-zero.
    NonZero,
    /// Even-odd: inside where the crossing count is odd.
    EvenOdd,
}

/// Line Cap Styles (ISO 32000-2 Table 53)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineCap {
    /// Butt caps: the stroke ends square at the endpoint.
    Butt,
    /// Round caps: a semicircle of stroke width is drawn past the endpoint.
    Round,
    /// Projecting square caps: the stroke extends half a width past the endpoint.
    Square,
}

impl LineCap {
    /// Maps the PDF `/LC` integer to a cap style, defaulting to `Butt`.
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
    /// Mitered joins, subject to `miter_limit`.
    Miter,
    /// Round joins: an arc of stroke width fills the corner.
    Round,
    /// Bevel joins: the corner is closed with a straight segment.
    Bevel,
}

impl LineJoin {
    /// Maps the PDF `/LJ` integer to a join style, defaulting to `Miter`.
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
    /// Stroke width in user-space units.
    pub width: f64,
    /// How stroke ends are terminated (`/LC`).
    pub cap: LineCap,
    /// How stroke corners are joined (`/LJ`).
    pub join: LineJoin,
    /// Ratio at which a mitered join is converted to a bevel (`/ML`).
    pub miter_limit: f64,
    /// Dash array and phase (`/D`), when the stroke is dashed.
    pub dash_pattern: Option<(Vec<f64>, f64)>,
}

/// Image Pixel Formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    /// One byte per pixel, grayscale.
    Gray8,
    /// Three bytes per pixel, RGB.
    Rgb8,
    /// Four bytes per pixel, CMYK.
    Cmyk8,
    /// Four bytes per pixel, RGB with alpha.
    Rgba8,
    /// One bit per pixel stencil; 0 paints the fill colour.
    MonoMask, // 1-bit stencil mask (0 means fill color, 1 means transparent)
    /// One bit per pixel stencil; 1 paints the fill colour.
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
    /// Builds a matrix from the six PDF coefficients `[a b c d e f]`.
    pub fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self([a, b, c, d, e, f])
    }

    /// Converts to a `kurbo` affine transform.
    pub fn as_affine(&self) -> Affine {
        Affine::new(self.0)
    }

    /// Returns `self` followed by `other`.
    pub fn concat(&self, other: &Self) -> Self {
        let res = self.as_affine() * other.as_affine();
        Self(res.as_coeffs())
    }

    /// Returns `other` followed by `self`.
    pub fn pre_concat(&self, other: &Self) -> Self {
        let res = other.as_affine() * self.as_affine();
        Self(res.as_coeffs())
    }
}

/// A simple axis-aligned rectangle (ISO 32000-2 Clause 7.3.6)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    /// Left edge.
    pub x1: f64,
    /// Bottom edge.
    pub y1: f64,
    /// Right edge.
    pub x2: f64,
    /// Top edge.
    pub y2: f64,
}

impl Rect {
    /// Builds a rectangle from two opposite corners.
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    /// Returns the smallest rectangle containing both.
    pub fn union(&self, other: &Self) -> Self {
        Self {
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
            x2: self.x2.max(other.x2),
            y2: self.y2.max(other.y2),
        }
    }

    /// Absolute horizontal extent.
    pub fn width(&self) -> f64 {
        (self.x2 - self.x1).abs()
    }

    /// Absolute vertical extent.
    pub fn height(&self) -> f64 {
        (self.y2 - self.y1).abs()
    }
}

/// Graphics State Parameters (ISO 32000-2 Table 52)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicsState {
    /// Current transformation matrix (`cm`).
    pub ctm: Matrix,
    /// Colour used for stroking.
    pub stroke_color: Color,
    /// Colour used for filling.
    pub fill_color: Color,
    /// Pen parameters used for stroking.
    pub stroke_style: StrokeStyle,
    /// Constant alpha applied to fills (`/ca`).
    pub fill_alpha: f64,
    /// Constant alpha applied to strokes (`/CA`).
    pub stroke_alpha: f64,
    /// Blend mode applied when compositing (`/BM`).
    pub blend_mode: BlendMode,
    /// Text-related parameters, valid inside `BT`/`ET`.
    pub text_state: TextState,
    /// Colour space in which `fill_color` is expressed.
    pub fill_color_space: ColorSpaceKind,
    /// Colour space in which `stroke_color` is expressed.
    pub stroke_color_space: ColorSpaceKind,
    /// The fill space resolved far enough to paint in, when `cs` named one whose
    /// components are not a colour on their own — `/Separation` and `/DeviceN` (8.6.6).
    ///
    /// **Not serialised.** A tint transform is a program reached through the page's
    /// resources; the sublimated form of a graphics state is the state, not the
    /// resources it looked through. `Arc` because `q`/`Q` clone this on every push.
    #[serde(skip)]
    pub fill_space: Option<Arc<ResolvedColorSpace>>,
    /// The stroke space, as `fill_space`.
    #[serde(skip)]
    pub stroke_space: Option<Arc<ResolvedColorSpace>>,
    /// Number of clip regions pushed at this state level.
    pub clip_count: usize,
    /// Soft mask applied when compositing (`/SMask`).
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
            fill_space: None,
            stroke_space: None,
            clip_count: 0,
            smask: None,
        }
    }
}

/// Text State Parameters (ISO 32000-2 Table 105)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextState {
    /// Extra space added after each glyph (`Tc`).
    pub char_spacing: f64,
    /// Extra space added after each single-byte space (`Tw`).
    pub word_spacing: f64,
    /// Horizontal scaling as a percentage (`Tz`).
    pub horizontal_scaling: f64,
    /// Distance between baselines (`TL`).
    pub leading: f64,
    /// Writing mode: 0 horizontal, 1 vertical.
    pub wmode: u8,
    /// Resource name of the selected font (`Tf`).
    pub font: Option<crate::object::PdfName>,
    /// The font dictionary itself, when an `ExtGState` selected it rather than `Tf`.
    ///
    /// Table 57's `/Font` entry is `[font size]`, where the font is an **indirect
    /// reference to a font dictionary** and not a resource name — so it cannot be held
    /// in the field above, and a page that sets its font this way had none at all. It
    /// lives in the text state rather than beside the interpreter because `gs` changes
    /// the graphics state, which means `q` and `Q` must save and restore it.
    ///
    /// At most one of this and `font` is set: whichever of `Tf` and `gs` came last wins,
    /// and each clears the other.
    pub font_ref: Option<crate::handle::Handle<crate::object::Object>>,
    /// Font size in text-space units (`Tf`).
    pub font_size: f64,
    /// How glyphs are painted (`Tr`).
    pub rendering_mode: TextRenderingMode,
    /// Baseline displacement for super/subscript (`Ts`).
    pub rise: f64,
    /// Whether text knockout applies to the group.
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
            font_ref: None,
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
    /// Fill the glyphs.
    Fill = 0,
    /// Stroke the glyph outlines.
    Stroke = 1,
    /// Fill, then stroke.
    FillStroke = 2,
    /// Paint nothing; the text still contributes to extraction.
    Invisible = 3,
    /// Fill and add the glyphs to the clip path.
    FillClip = 4,
    /// Stroke and add the glyphs to the clip path.
    StrokeClip = 5,
    /// Fill, stroke, and add the glyphs to the clip path.
    FillStrokeClip = 6,
    /// Add the glyphs to the clip path only.
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
    /// Text matrix (`Tm`).
    pub tm: Matrix,
    /// Text line matrix, the start of the current line.
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
