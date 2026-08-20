//! `/DCTDecode` (7.4.8) — a JPEG, in the colour space the image dictionary declares.

use crate::PdfResult;
use crate::error::PdfError;
use crate::filters::{DecodingFilter, FilterContext};
use bytes::Bytes;

/// The unit [`crate::filters::filter_for`] hands `/DCTDecode` to.
pub struct JpegFilter;

impl DecodingFilter for JpegFilter {
    fn decode(&self, input: &[u8], _cx: &FilterContext<'_>) -> PdfResult<Bytes> {
        decode_jpeg(input)
    }
}

/// Decodes a JPEG into the components the image dictionary describes (7.4.8).
///
/// **A filter returns samples, not a colour.** This went through
/// `image::ImageReader::decode()` and then `to_rgb8()`, which is three components
/// whatever the file holds — and `image`'s `DynamicImage` has no CMYK variant, so the
/// conversion was not laziness but the only thing that type can express.
///
/// The consequence was a rendering defect on two of the three colour spaces a JPEG can
/// be in. `detect_pixel_format` reads `/ColorSpace` from the image dictionary and picks
/// `Gray8` for `/DeviceGray` and `Cmyk8` for `/DeviceCMYK`; the backend then walked a
/// three-byte-per-pixel buffer one byte at a time, or four. Only `/DeviceRGB` came out
/// right, and it came out right by coincidence.
///
/// So the decoder underneath `image` is used directly, and the output colour space
/// follows the input: `Luma` stays one component, `CMYK` and `YCCK` stay four, and
/// everything else becomes RGB — which is what the image dictionary will have said.
///
/// **Not handled here**: Adobe writes CMYK JPEGs with inverted samples, and files
/// carrying one usually say so with `/Decode [1 0 1 0 1 0 1 0]`. Reading `/Decode` is
/// the image dictionary's job, not the filter's, and it is not done yet.
pub(crate) fn decode_jpeg(input: &[u8]) -> PdfResult<Bytes> {
    use zune_core::bytestream::ZCursor;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;
    use zune_jpeg::JpegDecoder;

    let fail = |what: &str, e: zune_jpeg::errors::DecodeErrors| PdfError::Filter {
        filter: "DCTDecode".into(),
        message: format!("{what}: {e:?}").into(),
    };

    let mut decoder = JpegDecoder::new(ZCursor::new(input));
    decoder.decode_headers().map_err(|e| fail("Failed to read JPEG", e))?;
    let out = match decoder.input_colorspace() {
        Some(ColorSpace::Luma) => ColorSpace::Luma,
        Some(ColorSpace::CMYK | ColorSpace::YCCK) => ColorSpace::CMYK,
        _ => ColorSpace::RGB,
    };
    decoder.set_options(DecoderOptions::default().jpeg_set_out_colorspace(out));
    decoder.decode().map(Bytes::from).map_err(|e| fail("Failed to decode JPEG", e))
}
