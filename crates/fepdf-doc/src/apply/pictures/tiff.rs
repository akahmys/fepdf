//! A TIFF, decoded page by page (TIFF 6.0).
//!
//! **Each directory is a page**, unless `NewSubfileType` marks it a reduced copy of
//! another — a thumbnail, which is not a page. A palette is expanded to RGB through
//! `ColorMap`; grey whose zero is white (`PhotometricInterpretation` 0, what a fax scan
//! is) is drawn the right way round by `/Decode`; one-bit samples stay one-bit, packed
//! as TIFF and PDF both pack them. An extra sample after the colour is taken as alpha.

use super::{Picture, Samples, Space, Unreadable, per_inch, split_alpha};
use ::tiff::ColorType;
use ::tiff::decoder::{Decoder, DecodingResult};
use ::tiff::tags::Tag;

/// The pages of a TIFF, as pictures.
///
/// # Errors
/// When it will not decode, or a page's samples are of a kind this does not draw:
/// YCbCr, Lab, many bands, or floating point.
pub(super) fn read(bytes: &[u8]) -> Result<Vec<Picture>, Unreadable> {
    let fail = |e: ::tiff::TiffError| format!("it will not decode: {e}");
    let mut decoder = Decoder::new(std::io::Cursor::new(bytes)).map_err(fail)?;
    let mut pictures = Vec::new();
    // As many pages as a TIFF may hold before this stops reading it (Rule 6).
    for page in 1..=10_000 {
        let reduced = decoder.get_tag_u32(Tag::NewSubfileType).is_ok_and(|t| t & 1 == 1);
        if !reduced {
            pictures.push(one(&mut decoder).map_err(|why| format!("page {page}: {why}"))?);
        }
        if !decoder.more_images() {
            break;
        }
        decoder.next_image().map_err(fail)?;
    }
    if pictures.is_empty() {
        return Err("it has no page that is not a thumbnail".into());
    }
    Ok(pictures)
}

/// The page the decoder is at.
fn one(decoder: &mut Decoder<std::io::Cursor<&[u8]>>) -> Result<Picture, Unreadable> {
    let fail = |e: ::tiff::TiffError| format!("it will not decode: {e}");
    let (width, height) = decoder.dimensions().map_err(fail)?;
    let colour = decoder.colortype().map_err(fail)?;
    let photometric = decoder.get_tag_unsigned::<u16>(Tag::PhotometricInterpretation).ok();
    let colour_map = decoder.get_tag_u16_vec(Tag::ColorMap).ok();
    let samples = samples(decoder.read_image().map_err(fail)?)?;
    let (space, bits, channels, alpha) = layout(colour)?;
    let (pixels, alpha) = if let ColorType::Palette(depth) = colour {
        let map = colour_map.ok_or("its palette has no colour map")?;
        (expanded(&samples, &map, depth, width).ok_or("its colour map is short")?, None)
    } else if alpha {
        split_alpha(&samples, channels, usize::from(bits / 8))
    } else {
        (samples, None)
    };
    let dpi = resolution(decoder).unwrap_or((72.0, 72.0));
    Ok(Picture {
        width,
        height,
        space,
        bits,
        samples: Samples::Pixels(pixels),
        inverted: photometric == Some(0) && space == Space::Gray,
        alpha,
        icc: decoder.get_tag_u8_vec(Tag::IccProfile).ok(),
        dpi,
        orientation: decoder
            .get_tag_unsigned::<u16>(Tag::Orientation)
            .ok()
            .filter(|o| (1..=8).contains(o))
            .unwrap_or(1),
    })
}

/// The colour space, depth, samples to a pixel and whether the last is alpha, for a
/// colour type this draws. A palette's pixels come out as eight-bit RGB.
fn layout(colour: ColorType) -> Result<(Space, u8, usize, bool), Unreadable> {
    // `ColorType` is non-exhaustive, so this asks for each kind it draws, by `if let`,
    // rather than a `match` whose last arm would take kinds not yet invented (Rule 5).
    let whole = |bits: u8| matches!(bits, 8 | 16);
    let found = if let ColorType::Gray(b @ (1 | 2 | 4 | 8 | 16)) = colour {
        Some((Space::Gray, b, 1, false))
    } else if let ColorType::GrayA(b) = colour {
        whole(b).then_some((Space::Gray, b, 2, true))
    } else if let ColorType::RGB(b) = colour {
        whole(b).then_some((Space::Rgb, b, 3, false))
    } else if let ColorType::RGBA(b) = colour {
        whole(b).then_some((Space::Rgb, b, 4, true))
    } else if let ColorType::CMYK(b) = colour {
        whole(b).then_some((Space::Cmyk, b, 4, false))
    } else if let ColorType::CMYKA(b) = colour {
        whole(b).then_some((Space::Cmyk, b, 5, true))
    } else if let ColorType::Palette(1 | 2 | 4 | 8) = colour {
        Some((Space::Rgb, 8, 3, false))
    } else {
        None
    };
    found.ok_or_else(|| format!("its samples are {colour:?}, which this does not draw").into())
}

/// The decoded samples as PDF holds them: bytes, sixteen-bit ones big-endian.
fn samples(result: DecodingResult) -> Result<Vec<u8>, Unreadable> {
    if let DecodingResult::U8(bytes) = result {
        return Ok(bytes);
    }
    if let DecodingResult::U16(words) = result {
        return Ok(words.iter().flat_map(|w| w.to_be_bytes()).collect());
    }
    Err("its samples are not whole numbers of eight or sixteen bits".into())
}

/// Palette indices of `depth` bits, rows packed to whole bytes, as eight-bit RGB from
/// `map`: its reds, then its greens, then its blues, each sixteen-bit (TIFF 6.0, 5).
fn expanded(indices: &[u8], map: &[u16], depth: u8, width: u32) -> Option<Vec<u8>> {
    let entries = 1usize << depth;
    let (reds, rest) = map.split_at_checked(entries)?;
    let (greens, blues) = rest.split_at_checked(entries)?;
    let row = usize::try_from(width).ok()?.checked_mul(usize::from(depth))?.div_ceil(8);
    let per_byte = 8 / usize::from(depth);
    let mask = u8::try_from(entries - 1).ok()?;
    let mut out = Vec::new();
    for line in indices.chunks(row.max(1)) {
        let all = line.iter().flat_map(|byte| {
            (0..per_byte).rev().map(move |slot| (byte >> (slot * usize::from(depth))) & mask)
        });
        for index in all.take(usize::try_from(width).ok()?) {
            let at = usize::from(index);
            for channel in [reds, greens, blues] {
                let [high, _] = channel.get(at)?.to_be_bytes();
                out.push(high);
            }
        }
    }
    Some(out)
}

/// Dots to the inch across and up, from `XResolution`, `YResolution` and
/// `ResolutionUnit` (TIFF 6.0, 8): 2 is inches, which is the default, and 3 centimetres.
fn resolution(decoder: &mut Decoder<std::io::Cursor<&[u8]>>) -> Option<(f64, f64)> {
    let unit = decoder.get_tag_unsigned::<u16>(Tag::ResolutionUnit).unwrap_or(2);
    let x = decoder.get_tag_f64(Tag::XResolution).ok()?;
    let y = decoder.get_tag_f64(Tag::YResolution).unwrap_or(x);
    Some((per_inch(x, unit, (2, 3))?, per_inch(y, unit, (2, 3))?))
}
