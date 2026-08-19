//! PDF Stream Decoding Filters (ISO 32000-2:2020 Clause 7.4)

use crate::PdfResult;
use crate::arena::PdfArena;
use crate::error::PdfError;
use crate::object::Object;
use bytes::Bytes;

pub mod ascii;
pub mod flate;
pub mod lzw;
pub mod predictor;
pub mod runlength;

/// The filters of clause 7.4 that are plain byte transformations rather than image
/// codecs, including the Table 6 abbreviations that only inline images may use (7.8.6).
///
/// Split out of `decode_stream` so that neither function exceeds the length RR-15 allows;
/// they are one decision expressed in two places only because of that limit.
fn decode_byte_filter(
    filter_name: &str,
    input: &[u8],
    params: Option<&Object>,
    arena: &PdfArena,
) -> PdfResult<Bytes> {
    match filter_name {
        "LZWDecode" | "LZW" => lzw::LzwFilter.decode(input, params, arena),
        "ASCIIHexDecode" | "AHx" => ascii::AsciiHexFilter.decode(input, params, arena),
        "ASCII85Decode" | "A85" => ascii::Ascii85Filter.decode(input, params, arena),
        _ => runlength::RunLengthFilter.decode(input, params, arena),
    }
}

/// A trait for decoding PDF stream filters.
pub trait DecodingFilter {
    /// Decodes the input bytes according to the filter logic.
    fn decode(&self, input: &[u8], params: Option<&Object>, arena: &PdfArena) -> PdfResult<Bytes>;
}

/// Dispatches decoding requests to the appropriate filter implementation.
pub fn decode_stream(
    filter_name: &str,
    input: &[u8],
    params: Option<&Object>,
    arena: &PdfArena,
) -> PdfResult<Bytes> {
    // Heuristic: Check for Zstd magic number (28 B5 2F FD)
    // This handles "Lie Filters" where the dict says Flate but the data is Zstd.
    if input.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        let decoded = zstd::decode_all(input).map_err(|e| PdfError::Filter {
            filter: "Zstd(Heuristic)".into(),
            message: e.to_string().into(),
        })?;
        return Ok(Bytes::from(decoded));
    }

    // Table 6 gives every filter an abbreviation for use in inline images (7.8.6), and
    // producers use them: `/AHx` appears seven times in one external file and `/A85` and
    // `/LZW` once each. Only `Fl` and `DCT` were matched before, so a stream naming any
    // of the others by its short name was refused for the wrong reason — not "this engine
    // cannot decode that" but "this engine has not heard of that".
    match filter_name {
        "FlateDecode" | "Fl" => {
            let decoder = flate::FlateFilter;
            decoder.decode(input, params, arena)
        }
        "LZWDecode" | "LZW" | "ASCIIHexDecode" | "AHx" | "ASCII85Decode" | "A85"
        | "RunLengthDecode" | "RL" => decode_byte_filter(filter_name, input, params, arena),
        "ZstandardDecode" | "Zstd" => {
            let decoded = zstd::decode_all(input).map_err(|e| PdfError::Filter {
                filter: filter_name.to_string().into(),
                message: e.to_string().into(),
            })?;
            Ok(Bytes::from(decoded))
        }
        "DCTDecode" | "DCT" => {
            use image::ImageReader;
            use std::io::Cursor;
            let img = ImageReader::new(Cursor::new(input))
                .with_guessed_format()
                .map_err(|e| PdfError::Filter {
                    filter: "DCTDecode".into(),
                    message: format!("Failed to read JPEG: {e}").into(),
                })?
                .decode()
                .map_err(|e| PdfError::Filter {
                    filter: "DCTDecode".into(),
                    message: format!("Failed to decode JPEG: {e}").into(),
                })?;

            let bytes = img.to_rgb8().into_raw();
            Ok(Bytes::from(bytes))
        }
        _ => Err(PdfError::Filter {
            filter: filter_name.to_string().into(),
            message: format!("Unsupported filter: {filter_name}").into(),
        }),
    }
}

/// Orchestrates multi-filter decoding for a stream dictionary.
pub fn process_arena_filters(
    data: &[u8],
    dict: &std::collections::BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>,
    arena: &PdfArena,
) -> PdfResult<Bytes> {
    let filter_key = arena.intern_name(crate::object::PdfName::new("Filter"));
    let params_key = arena.intern_name(crate::object::PdfName::new("DecodeParms"));

    let mut current_data = Bytes::copy_from_slice(data);

    if let Some(filter_obj) = dict.get(&filter_key) {
        let filter_obj = filter_obj.resolve(arena);
        match filter_obj {
            Object::Name(h) => {
                let name = arena
                    .get_name(h)
                    .ok_or_else(|| PdfError::Other("Filter name not found".into()))?;
                let params = dict.get(&params_key).map(|o| o.resolve(arena));
                current_data = decode_stream(name.as_str(), &current_data, params.as_ref(), arena)?;
            }
            Object::Array(h) => {
                let filters = arena
                    .get_array(h)
                    .ok_or_else(|| PdfError::Other("Filter array not found".into()))?;
                let params_arr = dict.get(&params_key).and_then(|o| {
                    if let Object::Array(ah) = o.resolve(arena) {
                        arena.get_array(ah)
                    } else {
                        None
                    }
                });

                for (i, f_obj) in filters.iter().enumerate() {
                    if let Object::Name(fh) = f_obj.resolve(arena) {
                        let name = arena
                            .get_name(fh)
                            .ok_or_else(|| PdfError::Other("Filter name not found".into()))?;
                        let p = params_arr.as_ref().and_then(|a| a.get(i));
                        current_data = decode_stream(name.as_str(), &current_data, p, arena)?;
                    }
                }
            }
            _ => {}
        }
    }

    Ok(current_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flate_decode_stream() {
        use flate2::Compression;
        use flate2::write::ZlibEncoder;
        use std::io::Write;

        let arena = PdfArena::new();
        let raw_data = b"Hello fepdf PDF 2.0 Engine!";
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(raw_data).unwrap();
        let compressed = encoder.finish().unwrap();

        let decoded = decode_stream("FlateDecode", &compressed, None, &arena).unwrap();
        assert_eq!(&decoded[..], raw_data);
    }

    #[test]
    fn test_zstd_decode_stream() {
        let arena = PdfArena::new();
        let raw_data = b"fepdf PDF Zstd Compression Stream";
        let compressed = zstd::encode_all(&raw_data[..], 3).unwrap();
        let decoded = decode_stream("ZstandardDecode", &compressed, None, &arena).unwrap();
        assert_eq!(&decoded[..], raw_data);
    }

    #[test]
    fn test_unknown_filter_error() {
        let arena = PdfArena::new();
        let result = decode_stream("NonExistentFilter", b"data", None, &arena);
        assert!(result.is_err());
    }
}
