use crate::handle::Handle;
use crate::{FromPdfObject, Object, PdfName};

/// PDF Font Descriptor (ISO 32000-2:2020 Clause 9.8)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.8")]
pub struct PdfFontDescriptor {
    #[pdf_key("FontName")]
    pub font_name: PdfName,
    #[pdf_key("Flags")]
    pub flags: i64,
    #[pdf_key("FontBBox")]
    pub font_bbox: crate::graphics::Rect,
    #[pdf_key("ItalicAngle")]
    pub italic_angle: f64,
    #[pdf_key("Ascent")]
    pub ascent: f64,
    #[pdf_key("Descent")]
    pub descent: f64,
    #[pdf_key("CapHeight")]
    pub cap_height: Option<f64>,
    #[pdf_key("StemV")]
    pub stem_v: Option<f64>,
    #[pdf_key("FontFile")]
    pub font_file: Option<Handle<Object>>,
    #[pdf_key("FontFile2")]
    pub font_file2: Option<Handle<Object>>,
    #[pdf_key("FontFile3")]
    pub font_file3: Option<Handle<Object>>,
}

/// Base PDF Font Dictionary (Clause 9.2)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.2")]
pub struct PdfFont {
    #[pdf_key("Subtype")]
    pub subtype: PdfName,
    #[pdf_key("BaseFont")]
    pub base_font: PdfName,
    #[pdf_key("FirstChar")]
    pub first_char: Option<i64>,
    #[pdf_key("LastChar")]
    pub last_char: Option<i64>,
    #[pdf_key("Widths")]
    pub widths: Option<Handle<Vec<Object>>>,
    #[pdf_key("FontDescriptor")]
    pub font_descriptor: Option<Handle<Object>>,
    #[pdf_key("Encoding")]
    pub encoding: Option<Object>,
}

/// Type 1 Font Dictionary (Clause 9.6.2)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.6.2")]
pub struct PdfType1Font {
    #[pdf_key("BaseFont")]
    pub base_font: PdfName,
    #[pdf_key("FirstChar")]
    pub first_char: Option<i64>,
    #[pdf_key("LastChar")]
    pub last_char: Option<i64>,
    #[pdf_key("Widths")]
    pub widths: Option<Handle<Vec<Object>>>,
    #[pdf_key("FontDescriptor")]
    pub font_descriptor: Option<Handle<Object>>,
}

/// TrueType Font Dictionary (Clause 9.6.3)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.6.3")]
pub struct PdfTrueTypeFont {
    #[pdf_key("BaseFont")]
    pub base_font: PdfName,
    #[pdf_key("FirstChar")]
    pub first_char: Option<i64>,
    #[pdf_key("LastChar")]
    pub last_char: Option<i64>,
    #[pdf_key("Widths")]
    pub widths: Option<Handle<Vec<Object>>>,
    #[pdf_key("FontDescriptor")]
    pub font_descriptor: Option<Handle<Object>>,
}

/// Type 0 Font Dictionary (Clause 9.7)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.7")]
pub struct PdfType0Font {
    #[pdf_key("BaseFont")]
    pub base_font: PdfName,
    #[pdf_key("Encoding")]
    pub encoding: Object, // Name or Stream
    #[pdf_key("DescendantFonts")]
    pub descendant_fonts: Handle<Vec<Object>>, // CIDFont
}

/// OpenType Font Dictionary (Clause 9.6.4)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.6.4")]
pub struct PdfOpenTypeFont {
    #[pdf_key("BaseFont")]
    pub base_font: PdfName,
    #[pdf_key("FontDescriptor")]
    pub font_descriptor: Handle<Object>,
}

/// CIDFont Dictionary (Clause 9.7.4)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.7.4")]
pub struct PdfCIDFont {
    #[pdf_key("Subtype")]
    pub subtype: PdfName,
    #[pdf_key("BaseFont")]
    pub base_font: PdfName,
    #[pdf_key("CIDSystemInfo")]
    pub cid_system_info: Handle<Object>,
    #[pdf_key("FontDescriptor")]
    pub font_descriptor: Handle<Object>,
    #[pdf_key("DW")]
    pub dw: Option<i64>,
    #[pdf_key("W")]
    pub w: Option<Handle<Vec<Object>>>,
    #[pdf_key("DW2")]
    pub dw2: Option<Handle<Vec<Object>>>,
    #[pdf_key("W2")]
    pub w2: Option<Handle<Vec<Object>>>,
    #[pdf_key("CIDToGIDMap")]
    pub cid_to_gid_map: Option<Object>,
}
