//! A PNG, decoded (ISO/IEC 15948).
//!
//! **Expanded, and no further**: a palette becomes RGB, grey of fewer than eight bits
//! becomes eight, and `tRNS` becomes an alpha channel — and sixteen-bit samples stay
//! sixteen-bit, which PDF holds. The alpha channel is the image's soft mask. `pHYs` gives
//! the resolution where its unit is the metre, and `iCCP` the profile.

use super::{Picture, Samples, Space, Unreadable, split_alpha};

/// The picture a PNG holds.
///
/// # Errors
/// When the PNG will not decode.
pub(super) fn read(bytes: &[u8]) -> Result<Picture, Unreadable> {
    let mut decoder = ::png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(::png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|e| format!("it will not decode: {e}"))?;
    let size = reader.output_buffer_size().ok_or("it is too large to decode")?;
    let mut buffer = vec![0; size];
    let frame = reader.next_frame(&mut buffer).map_err(|e| format!("it will not decode: {e}"))?;
    buffer.truncate(frame.buffer_size());
    let info = reader.info();
    let dpi = info
        .pixel_dims
        .filter(|d| d.unit == ::png::Unit::Meter && d.xppu > 0 && d.yppu > 0)
        .map_or((72.0, 72.0), |d| (f64::from(d.xppu) * 0.0254, f64::from(d.yppu) * 0.0254));
    let icc = info.icc_profile.as_ref().map(|p| p.to_vec());
    let (colour_type, depth) = reader.output_color_type();
    let bits: u8 = if depth == ::png::BitDepth::Sixteen { 16 } else { 8 };
    let width = usize::from(bits / 8);
    let (space, channels, alpha) = match colour_type {
        ::png::ColorType::Grayscale => (Space::Gray, 1, false),
        ::png::ColorType::GrayscaleAlpha => (Space::Gray, 2, true),
        ::png::ColorType::Rgb => (Space::Rgb, 3, false),
        ::png::ColorType::Rgba => (Space::Rgb, 4, true),
        ::png::ColorType::Indexed => return Err("its palette did not expand".into()),
    };
    let (colour, alpha) =
        if alpha { split_alpha(&buffer, channels, width) } else { (buffer, None) };
    Ok(Picture {
        width: frame.width,
        height: frame.height,
        space,
        bits,
        samples: Samples::Pixels(colour),
        inverted: false,
        alpha,
        icc,
        dpi,
        orientation: 1,
    })
}
