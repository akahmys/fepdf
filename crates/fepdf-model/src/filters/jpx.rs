//! `/JPXDecode` (7.4.9) — JPEG 2000, the codec of archived and high-compression scans.
//!
//! **This is the one filter whose output layout is in the data rather than in the image
//! dictionary**, and the standard says so: 7.4.9 makes `/ColorSpace` *optional* for a
//! JPX image, and where it is present it overrides whatever the codestream declares.
//! Every other image in a PDF describes itself in its dictionary; this one may not.
//!
//! So this module answers two questions rather than one. [`JpxFilter`] decodes, like any
//! other filter. [`layout`] reads the codestream's header and reports how many components
//! a sample has, which is what the interpreter needs when the dictionary is silent — and
//! without it a greyscale JPX would be read three bytes at a time, which is the defect
//! `DCTDecode` was found committing on 160 images.
//!
//! Both the raw codestream and the JP2 container occur in PDF, and `hayro-jpeg2000`
//! reads both.
//!
//! **`/SMaskInData` is not implemented.** Its default is 0 — *ignore any alpha the
//! codestream carries* — and that is what happens here: the colour channels are
//! interleaved and an alpha channel is dropped. A file asking for 1 or 2 gets the
//! default treatment, silently, and that is a gap rather than a decision. It is recorded
//! in `ROADMAP.md` rather than left to be discovered.

use crate::PdfResult;
use crate::error::PdfError;
use crate::filters::{DecodingFilter, FilterContext};
use crate::graphics::PixelFormat;
use bytes::Bytes;
use hayro_jpeg2000::{ColorSpace, DecodeSettings, DecoderContext, Image};

/// The unit [`crate::filters::filter_for`] hands `/JPXDecode` to.
pub struct JpxFilter;

impl DecodingFilter for JpxFilter {
    fn decode(&self, input: &[u8], _cx: &FilterContext<'_>) -> PdfResult<Bytes> {
        decode(input).map(|(samples, _)| samples)
    }
}

/// Decodes a JPEG 2000 image, returning its samples and the layout they are in.
///
/// # Errors
/// Fails when the codestream does not parse or decode.
pub fn decode(input: &[u8]) -> PdfResult<(Bytes, PixelFormat)> {
    let image =
        Image::new(input, &DecodeSettings::default()).map_err(|e| refuse(&format!("{e:?}")))?;
    let colours = usize::from(image.color_space().num_channels());
    let format = format_of(image.color_space())
        .ok_or_else(|| refuse(&format!("{} components is not a layout PDF describes", colours)))?;

    let mut context = DecoderContext::default();
    let decoded = image.decode(&mut context).map_err(|e| refuse(&format!("{e:?}")))?;

    // `/SMaskInData` defaults to 0: any alpha the codestream carries is not part of the
    // image. `data_u8` interleaves every component, so an alpha channel is dropped here
    // rather than asked for and thrown away.
    let components = decoded.components();
    let samples = if components.len() > colours {
        let interleaved = decoded.data_u8();
        interleaved
            .chunks_exact(components.len())
            .flat_map(|px| px[..colours].iter().copied())
            .collect()
    } else {
        decoded.data_u8()
    };
    Ok((Bytes::from(samples), format))
}

/// How many components a sample of this codestream has, without decoding it.
///
/// What the interpreter needs when the image dictionary omits `/ColorSpace`, which
/// 7.4.9 permits for this filter and no other. Reading the header is cheap; decoding
/// the image to find out how to read the image is not.
#[must_use]
pub fn layout(input: &[u8]) -> Option<PixelFormat> {
    let image = Image::new(input, &DecodeSettings::default()).ok()?;
    format_of(image.color_space())
}

/// The layout PDF describes for a JPEG 2000 colour space.
///
/// An ICC-based or unknown space is taken by its *channel count*, because that is what
/// decides how the samples are read — the same reasoning `[/ICCBased …]` gets in an
/// image dictionary, where `/N` answers and the family name does not.
fn format_of(space: &ColorSpace) -> Option<PixelFormat> {
    match space.num_channels() {
        1 => Some(PixelFormat::Gray8),
        3 => Some(PixelFormat::Rgb8),
        4 => Some(PixelFormat::Cmyk8),
        _ => None,
    }
}

fn refuse(why: &str) -> PdfError {
    PdfError::Filter { filter: "JPXDecode".into(), message: why.to_string().into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Data that is not a codestream is refused by name, not decoded into noise.
    #[test]
    fn a_stream_that_is_not_jpeg_2000_is_refused() {
        let err = decode(&[0xDE, 0xAD, 0xBE, 0xEF]).expect_err("not jpx");
        let PdfError::Filter { filter, .. } = err else { panic!("a filter error") };
        assert_eq!(filter, "JPXDecode");
        assert!(layout(&[0xDE, 0xAD, 0xBE, 0xEF]).is_none());
    }

    /// The layout is a question about the header, so it costs no decode and answers for
    /// a codestream the image dictionary says nothing about.
    #[test]
    fn the_layout_comes_from_the_codestream() {
        // A minimal SIZ: three components makes an RGB layout, one makes grey.
        assert_eq!(format_of(&ColorSpace::Gray), Some(PixelFormat::Gray8));
        assert_eq!(format_of(&ColorSpace::RGB), Some(PixelFormat::Rgb8));
        assert_eq!(format_of(&ColorSpace::CMYK), Some(PixelFormat::Cmyk8));
        assert_eq!(
            format_of(&ColorSpace::Icc { profile: Vec::new(), num_channels: 1 }),
            Some(PixelFormat::Gray8),
            "an ICC space is read by its channel count, as `[/ICCBased …]` is by its /N"
        );
        assert_eq!(format_of(&ColorSpace::Unknown { num_channels: 2 }), None);
    }
}
