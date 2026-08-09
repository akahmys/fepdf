//! PDF Content Stream Intermediate Representation (IR).
//!
//! This module defines a structured, normalized representation of PDF drawing commands,
//! allowing downstream components to render or analyze documents without parsing
//! raw PostScript-style tokens.

use kurbo::{Affine, Point};
use serde::{Deserialize, Serialize};
pub mod parser;
pub mod resurrection;
pub mod serializer;
use crate::graphics::{Color, StrokeStyle, TextRenderingMode, WindingRule};
use crate::object::PdfName;

/// A high-level, normalized drawing command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    // --- Graphics State ---
    /// Push the current graphics state onto the stack (q).
    PushState,
    /// Pop the graphics state from the stack (Q).
    PopState,
    /// Concatenate a transformation matrix (cm).
    Transform(Affine),

    // --- Path Construction ---
    /// Begin a new subpath (m).
    MoveTo(Point),
    /// Append a straight line segment (l).
    LineTo(Point),
    /// Append a cubic Bézier curve (c, v, y).
    CurveTo(Point, Point, Point),
    /// Close the current subpath (h).
    ClosePath,
    /// Append a rectangle (re).
    Rect(kurbo::Rect),

    // --- Painting ---
    /// Fill the current path (f, f*, F).
    Fill(WindingRule),
    /// Stroke the current path (S).
    Stroke(StrokeStyle),
    /// Fill and then stroke the current path (B, B*, b, b*).
    FillStroke(WindingRule, StrokeStyle),
    /// Use the current path as a clipping path (W, W*).
    Clip(WindingRule),

    // --- Text ---
    /// Begin a text object (BT).
    BeginText,
    /// End a text object (ET).
    EndText,
    /// Set the font and font size (Tf).
    SetFont {
        /// The resource name of the font (e.g., "F1").
        font: String,
        /// Optical size in points.
        size: f64,
    },
    /// Show a string of text (Tj, ', ").
    /// Holds the RAW bytes from the PDF content stream.
    ShowText(bytes::Bytes),
    /// Show an array of text strings and numeric offsets (TJ).
    ///
    /// Preserving these offsets is critical for correct vertical text layout and
    /// ruby character positioning in Japanese documents.
    ShowTextArray(Vec<TextArrayItem>),
    /// Move the text position (Td, TD, T*).
    MoveText(Point),
    /// Set the text matrix (Tm).
    SetTextMatrix(Affine),
    /// Set the character spacing (Tc).
    SetCharSpacing(f64),
    /// Set the word spacing (Tw).
    SetWordSpacing(f64),
    /// Set the horizontal scaling (Tz).
    SetHorizontalScaling(f64),
    /// Set the text rendering mode (Tr).
    SetTextRenderMode(TextRenderingMode),
    /// Set the text rise (Ts).
    SetTextRise(f64),
    /// Set the text leading (TL).
    SetTextLeading(f64),
    /// Move to the start of the next line (T*).
    MoveToNextLine,
    /// Set the writing mode (0 for horizontal, 1 for vertical).
    SetWritingMode(u8),

    // --- Color ---
    /// Set the fill color (sc, scn, g, rg, k).
    SetFillColor(Color),
    /// Set the stroke color (SC, SCN, G, RG, K).
    SetStrokeColor(Color),
    /// Set the fill color space (cs).
    SetFillColorSpace(String),
    /// Set the stroke color space (CS).
    SetStrokeColorSpace(String),

    // --- Graphics State Parameters ---
    /// Set the line width (w).
    SetLineWidth(f64),
    /// Set the line cap style (J).
    SetLineCap(crate::graphics::LineCap),
    /// Set the line join style (j).
    SetLineJoin(crate::graphics::LineJoin),
    /// Set the miter limit (M).
    SetMiterLimit(f64),
    /// Set the line dash pattern (d).
    SetDashPattern(Vec<f64>, f64),

    // --- XObjects & Images ---
    /// Draw an external object (Do).
    DrawXObject(String),
    /// Define an inline image (BI...ID...EI).
    DrawInlineImage { width: u32, height: u32, format: crate::graphics::PixelFormat, data: Vec<u8> },

    // --- Compatibility & Extensions ---
    /// Begin a marked-content sequence (BMC, BDC).
    BeginMarkedContent {
        tag: PdfName,
        properties: Option<IrObject>, // Tag name or resource name or inline dict
    },
    /// End a marked-content sequence (EMC).
    EndMarkedContent,

    // --- Type 3 Fonts ---
    /// Set the glyph width and bounding box for a Type 3 font (d0, d1).
    Type3SetMetrics {
        /// Horizontal advance.
        wx: f64,
        /// Vertical advance (only for vertical writing mode).
        wy: f64,
        /// Bounding box (llx, lly, urx, ury) if d1.
        bbox: Option<kurbo::Rect>,
    },

    /// A raw PDF operator that could not be sublimated.
    RawOperator { name: String, operands: Vec<IrObject> },
}

/// A self-contained, serializable representation of a PDF object within the IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IrObject {
    Boolean(bool),
    Integer(i64),
    Real(f64),
    String(bytes::Bytes),
    Hex(bytes::Bytes),
    Name(String),
    Array(Vec<IrObject>),
    Dictionary(std::collections::BTreeMap<String, IrObject>),
    Null,
}

impl IrObject {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            Self::Real(f) => Some(*f as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Real(f) => Some(*f),
            Self::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }
}

/// An item in a text array (TJ).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextArrayItem {
    /// A string of text (bytes).
    Text(bytes::Bytes),
    /// A numeric offset for kerning or precise positioning.
    ///
    /// Positive values move characters closer (UP in vertical mode),
    /// negative values move them further apart (DOWN in vertical mode).
    Offset(f64),
}
