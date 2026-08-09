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
        message: format!("Unsupported predictor: {}", predictor).into(),
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
    let columns = get_int_param(dict, arena, "Columns", 1) as usize;
    let colors = get_int_param(dict, arena, "Colors", 1) as usize;
    let bpc = get_int_param(dict, arena, "BitsPerComponent", 8) as usize;

    let bytes_per_pixel = (colors * bpc).div_ceil(8);
    let row_size = (columns * colors * bpc).div_ceil(8);
    let stride = row_size + 1;

    if !data.len().is_multiple_of(stride) {
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
            0 => input[j],                                                     // None
            1 => input[j].wrapping_add(left),                                  // Sub
            2 => input[j].wrapping_add(up),                                    // Up
            3 => input[j].wrapping_add(((left as u16 + up as u16) / 2) as u8), // Average
            4 => input[j].wrapping_add(paeth(left, up, up_left)),              // Paeth
            _ => {
                return Err(PdfError::Filter {
                    filter: "PNGPredictor".into(),
                    message: format!("Invalid PNG predictor tag: {}", tag).into(),
                });
            }
        };
    }
    Ok(())
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let pa = (p - a as i16).abs();
    let pb = (p - b as i16).abs();
    let pc = (p - c as i16).abs();

    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}
