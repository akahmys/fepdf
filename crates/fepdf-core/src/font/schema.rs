use crate::handle::Handle;
use crate::{FromPdfObject, Object, PdfName};

/// PDF Font Descriptor (ISO 32000-2:2020 Clause 9.8)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.8")]
pub struct PdfFontDescriptor {
    #[pdf_key("FontName")]
    /// `/FontName`: the PostScript name of the font.
    pub font_name: PdfName,
    #[pdf_key("Flags")]
    /// `/Flags`: descriptor flag bits (serif, symbolic, italic, fixed pitch...).
    pub flags: i64,
    #[pdf_key("FontBBox")]
    /// `/FontBBox`: the glyph bounding box in glyph space.
    pub font_bbox: crate::graphics::Rect,
    #[pdf_key("ItalicAngle")]
    /// `/ItalicAngle`: degrees counter-clockwise from vertical.
    pub italic_angle: f64,
    #[pdf_key("Ascent")]
    /// `/Ascent`: maximum height above the baseline.
    pub ascent: f64,
    #[pdf_key("Descent")]
    /// `/Descent`: maximum depth below the baseline, negative.
    pub descent: f64,
    #[pdf_key("CapHeight")]
    /// `/CapHeight`: height of a capital letter above the baseline.
    pub cap_height: Option<f64>,
    #[pdf_key("StemV")]
    /// `/StemV`: dominant vertical stem width.
    pub stem_v: Option<f64>,
    #[pdf_key("FontFile")]
    /// `/FontFile`: an embedded Type 1 program.
    pub font_file: Option<Handle<Object>>,
    #[pdf_key("FontFile2")]
    /// `/FontFile2`: an embedded TrueType program.
    pub font_file2: Option<Handle<Object>>,
    #[pdf_key("FontFile3")]
    /// `/FontFile3`: an embedded CFF or OpenType program.
    pub font_file3: Option<Handle<Object>>,
}

/// Base PDF Font Dictionary (Clause 9.2)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.2")]
pub struct PdfFont {
    #[pdf_key("Subtype")]
    /// `/Subtype`: the font's specific kind.
    pub subtype: PdfName,
    #[pdf_key("BaseFont")]
    /// `/BaseFont`: the PostScript name of the font.
    pub base_font: PdfName,
    #[pdf_key("FirstChar")]
    /// `/FirstChar`: first code covered by `widths`.
    pub first_char: Option<i64>,
    #[pdf_key("LastChar")]
    /// `/LastChar`: last code covered by `widths`.
    pub last_char: Option<i64>,
    #[pdf_key("Widths")]
    /// `/Widths`: advance widths for `first_char..=last_char`.
    pub widths: Option<Handle<Vec<Object>>>,
    #[pdf_key("FontDescriptor")]
    /// `/FontDescriptor`: metrics and the embedded program.
    pub font_descriptor: Option<Handle<Object>>,
    #[pdf_key("Encoding")]
    /// `/Encoding`: a base encoding name or a differences dictionary.
    pub encoding: Option<Object>,
}

/// Type 1 Font Dictionary (Clause 9.6.2)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.6.2")]
pub struct PdfType1Font {
    #[pdf_key("BaseFont")]
    /// `/BaseFont`: the PostScript name of the font.
    pub base_font: PdfName,
    #[pdf_key("FirstChar")]
    /// `/FirstChar`: first code covered by `widths`.
    pub first_char: Option<i64>,
    #[pdf_key("LastChar")]
    /// `/LastChar`: last code covered by `widths`.
    pub last_char: Option<i64>,
    #[pdf_key("Widths")]
    /// `/Widths`: advance widths for `first_char..=last_char`.
    pub widths: Option<Handle<Vec<Object>>>,
    #[pdf_key("FontDescriptor")]
    /// `/FontDescriptor`: metrics and the embedded program.
    pub font_descriptor: Option<Handle<Object>>,
}

/// TrueType Font Dictionary (Clause 9.6.3)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.6.3")]
pub struct PdfTrueTypeFont {
    #[pdf_key("BaseFont")]
    /// `/BaseFont`: the PostScript name of the font.
    pub base_font: PdfName,
    #[pdf_key("FirstChar")]
    /// `/FirstChar`: first code covered by `widths`.
    pub first_char: Option<i64>,
    #[pdf_key("LastChar")]
    /// `/LastChar`: last code covered by `widths`.
    pub last_char: Option<i64>,
    #[pdf_key("Widths")]
    /// `/Widths`: advance widths for `first_char..=last_char`.
    pub widths: Option<Handle<Vec<Object>>>,
    #[pdf_key("FontDescriptor")]
    /// `/FontDescriptor`: metrics and the embedded program.
    pub font_descriptor: Option<Handle<Object>>,
}

/// Type 0 Font Dictionary (Clause 9.7)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.7")]
pub struct PdfType0Font {
    #[pdf_key("BaseFont")]
    /// `/BaseFont`: the PostScript name of the composite font.
    pub base_font: PdfName,
    #[pdf_key("Encoding")]
    /// `/Encoding`: a predefined CMap name, or an embedded CMap stream.
    pub encoding: Object, // Name or Stream
    #[pdf_key("DescendantFonts")]
    /// `/DescendantFonts`: the single CIDFont this Type 0 font wraps.
    pub descendant_fonts: Handle<Vec<Object>>, // CIDFont
}

/// OpenType Font Dictionary (Clause 9.6.4)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.6.4")]
pub struct PdfOpenTypeFont {
    #[pdf_key("BaseFont")]
    /// `/BaseFont`: the PostScript name of the font.
    pub base_font: PdfName,
    #[pdf_key("FontDescriptor")]
    /// `/FontDescriptor`: metrics and the embedded program.
    pub font_descriptor: Handle<Object>,
}

/// CIDFont Dictionary (Clause 9.7.4)
#[derive(Debug, Clone, FromPdfObject)]
#[pdf_dict(clause = "9.7.4")]
pub struct PdfCIDFont {
    #[pdf_key("Subtype")]
    /// `/Subtype`: CIDFontType0 (CFF) or CIDFontType2 (TrueType).
    pub subtype: PdfName,
    #[pdf_key("BaseFont")]
    /// `/BaseFont`: the PostScript name of the font.
    pub base_font: PdfName,
    #[pdf_key("CIDSystemInfo")]
    /// `/CIDSystemInfo`: registry, ordering and supplement of the CID collection.
    pub cid_system_info: Handle<Object>,
    #[pdf_key("FontDescriptor")]
    /// `/FontDescriptor`: metrics and the embedded program.
    pub font_descriptor: Handle<Object>,
    #[pdf_key("DW")]
    /// `/DW`: default horizontal advance for CIDs absent from `w`.
    pub dw: Option<i64>,
    #[pdf_key("W")]
    /// `/W`: horizontal advances, in the spec's run-length form.
    pub w: Option<Handle<Vec<Object>>>,
    #[pdf_key("DW2")]
    /// `/DW2`: default vertical position and advance.
    pub dw2: Option<Handle<Vec<Object>>>,
    #[pdf_key("W2")]
    /// `/W2`: vertical metrics, in the spec's run-length form.
    pub w2: Option<Handle<Vec<Object>>>,
    #[pdf_key("CIDToGIDMap")]
    /// `/CIDToGIDMap`: `Identity`, or a stream mapping CIDs to glyph indices.
    pub cid_to_gid_map: Option<Object>,
}
