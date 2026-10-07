//! PDF Predictor Functions (ISO 32000-2:2020 Clause 7.4.4.4)

use crate::PdfResult;
use crate::arena::PdfArena;
use crate::error::PdfError;
use crate::handle::Handle;
use crate::object::{Object, PdfName};
use std::collections::BTreeMap;

/// Applies the specified predictor to the decoded data.
pub fn apply_predictor(data: &[u8], params: &Object, arena: &PdfArena) -> PdfResult<Vec<u8>> {
    let dict = params.as_dict_handle().and_then(|h| arena.get_dict(h)).ok_or_else(|| {
        PdfError::Filter {
            filter: "Predictor".into(),
            message: "Predictor params must be a dictionary".into(),
        }
    })?;

    let predictor = get_int_param(&dict, arena, "Predictor", 1);

    if predictor == 1 {
        return Ok(data.to_vec());
    }

    if (10..=15).contains(&predictor) {
        return decode_png_predictor(data, &dict, arena);
    }

    Err(PdfError::Filter {
        filter: "Predictor".into(),
        message: format!("Unsupported predictor: {predictor}").into(),
    })
}

fn get_int_param(
    dict: &BTreeMap<Handle<PdfName>, Object>,
    arena: &PdfArena,
    key: &str,
    default: i64,
) -> i64 {
    arena
        .get_name_by_str(key)
        .and_then(|handle| dict.get(&handle))
        .map(|obj| obj.resolve(arena).as_integer().unwrap_or(default))
        .unwrap_or(default)
}

fn decode_png_predictor(
    data: &[u8],
    dict: &BTreeMap<Handle<PdfName>, Object>,
    arena: &PdfArena,
) -> PdfResult<Vec<u8>> {
    let invalid = |message: &str| PdfError::Filter {
        filter: "PNGPredictor".into(),
        message: message.to_string().into(),
    };
    // A parameter is a positive integer, and a row's width in bits is computed without
    // wrapping. `/Columns 4294967295` overflowed `columns * colors * bpc` in a debug
    // build, and in a release build wrapped to a width that sized the two row buffers
    // below (ROADMAP Z-2, after a finding of PrintCraft's).
    let positive = |key, default| {
        usize::try_from(get_int_param(dict, arena, key, default)).ok().filter(|&v| v > 0)
    };
    let (Some(columns), Some(colors), Some(bpc)) =
        (positive("Columns", 1), positive("Colors", 1), positive("BitsPerComponent", 8))
    else {
        return Err(invalid("Columns, Colors and BitsPerComponent must be positive"));
    };
    let bits_per_pixel =
        colors.checked_mul(bpc).ok_or_else(|| invalid("Colors × BitsPerComponent overflows"))?;
    let bytes_per_pixel = bits_per_pixel.div_ceil(8);
    let row_size = columns
        .checked_mul(bits_per_pixel)
        .map(|bits| bits.div_ceil(8))
        .ok_or_else(|| invalid("a row's width overflows"))?;
    let stride = row_size + 1;

    // A row longer than the data cannot be in it, so it is refused before a buffer of its
    // width is made. Empty data has no rows, and decodes to nothing.
    if data.is_empty() {
        return Ok(Vec::new());
    }
    if stride > data.len() || !data.len().is_multiple_of(stride) {
        return Err(PdfError::Filter {
            filter: "PNGPredictor".into(),
            message: "Invalid PNG predictor data length".into(),
        });
    }

    let rows = data.len() / stride;
    let mut out = Vec::with_capacity(rows * row_size);
    let mut prev_row: Vec<u8> = vec![0; row_size];

    for i in 0..rows {
        let row_data = &data[i * stride + 1..(i + 1) * stride];
        let tag = data[i * stride];
        let mut row = vec![0; row_size];

        decode_row(tag, row_data, &prev_row, bytes_per_pixel, &mut row)?;
        out.extend_from_slice(&row);
        prev_row = row;
    }

    Ok(out)
}

fn decode_row(tag: u8, input: &[u8], prev: &[u8], bpp: usize, out: &mut [u8]) -> PdfResult<()> {
    for j in 0..input.len() {
        let left = if j >= bpp { out[j - bpp] } else { 0 };
        let up = prev[j];
        let up_left = if j >= bpp { prev[j - bpp] } else { 0 };

        out[j] = match tag {
            0 => input[j],                    // None
            1 => input[j].wrapping_add(left), // Sub
            2 => input[j].wrapping_add(up),   // Up
            3 => input[j].wrapping_add(u16::midpoint(u16::from(left), u16::from(up)) as u8), // Average
            4 => input[j].wrapping_add(paeth(left, up, up_left)), // Paeth
            _ => {
                return Err(PdfError::Filter {
                    filter: "PNGPredictor".into(),
                    message: format!("Invalid PNG predictor tag: {tag}").into(),
                });
            }
        };
    }
    Ok(())
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let pa = (p - i16::from(a)).abs();
    let pb = (p - i16::from(b)).abs();
    let pc = (p - i16::from(c)).abs();

    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}
