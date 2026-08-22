//! PDF Stream Decoding Filters (ISO 32000-2:2020 Clause 7.4)

use crate::PdfResult;
use crate::arena::PdfArena;
use crate::error::PdfError;
use crate::object::Object;
use bytes::Bytes;

/// An object dictionary, spelt out because the type is otherwise unwieldy.
type Dict = std::collections::BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>;

pub mod ascii;
pub(crate) mod bilevel;
pub mod ccitt;
pub mod flate;
pub mod jbig2;
pub mod jpeg;
pub mod jpx;
pub mod lzw;
pub mod predictor;
pub mod runlength;

/// Whether this engine decodes a filter of this name.
///
/// Derived from [`filter_for`], so the census in `file_structure.rs` and the decoder can
/// no longer disagree — they were two hand-kept lists with a test between them.
#[must_use]
pub fn is_decoded(filter_name: &str) -> bool {
    filter_for(filter_name).is_some()
}

/// A trait for decoding PDF stream filters.
/// What a filter is given besides its bytes.
///
/// The parameters and the arena were enough while every filter was a byte
/// transformation. `CCITTFaxDecode` is not: `/Rows` defaults to the *image
/// dictionary's* `/Height` (Table 12), which is a fact about the object the stream
/// belongs to rather than about the filter. That one exception had already split the
/// entry point into two functions and left two filters outside the trait; a context
/// keeps the contract single, and gives `/JBIG2Globals` somewhere to arrive.
#[derive(Clone, Copy)]
pub struct FilterContext<'a> {
    /// The stream's `/DecodeParms`, already resolved.
    pub params: Option<&'a Object>,
    /// The arena the stream lives in, for resolving what the parameters point at.
    pub arena: &'a PdfArena,
    /// The image dictionary's `/Height`, when the stream is an image's.
    pub image_rows: Option<u32>,
}

impl<'a> FilterContext<'a> {
    /// A context for a stream that is not an image's.
    #[must_use]
    pub fn new(params: Option<&'a Object>, arena: &'a PdfArena) -> Self {
        Self { params, arena, image_rows: None }
    }

    /// The same context, told how tall the image is.
    #[must_use]
    pub fn in_image(self, rows: Option<u32>) -> Self {
        Self { image_rows: rows, ..self }
    }
}

/// One filter of clause 7.4.
///
/// Every filter this engine decodes implements this, image codecs included, and
/// [`filter_for`] is the only place a name is turned into one. That single mapping is
/// what makes a codec swappable: replacing `CCITTFaxDecode` means writing another unit
/// and changing one arm, with nothing outside `filters/` aware that it happened.
pub trait DecodingFilter {
    /// Decodes the input bytes according to the filter logic.
    ///
    /// # Errors
    /// Fails when the input is not what this filter decodes.
    fn decode(&self, input: &[u8], cx: &FilterContext<'_>) -> PdfResult<Bytes>;
}

/// The unit that decodes `filter_name`, or `None` when this engine has no decoder for it.
///
/// **The one list.** `is_decoded` used to be a second one, written out by hand beside
/// this dispatch, and a test existed to catch them disagreeing — which is a test that
/// exists because of a shape, not because of a risk. There is one shape now.
#[must_use]
pub fn filter_for(filter_name: &str) -> Option<&'static dyn DecodingFilter> {
    Some(match filter_name {
        "FlateDecode" | "Fl" => &flate::FlateFilter,
        "LZWDecode" | "LZW" => &lzw::LzwFilter,
        "ASCIIHexDecode" | "AHx" => &ascii::AsciiHexFilter,
        "ASCII85Decode" | "A85" => &ascii::Ascii85Filter,
        "RunLengthDecode" | "RL" => &runlength::RunLengthFilter,
        "DCTDecode" | "DCT" => &jpeg::JpegFilter,
        "CCITTFaxDecode" | "CCF" => &ccitt::CcittFilter,
        "JBIG2Decode" => &jbig2::Jbig2Filter,
        "JPXDecode" => &jpx::JpxFilter,
        _ => return None,
    })
}

/// Decodes one filter, for a stream that is not an image's.
///
/// # Errors
/// Propagates whatever the filter says about data it cannot decode.
pub fn decode_stream(
    filter_name: &str,
    input: &[u8],
    params: Option<&Object>,
    arena: &PdfArena,
) -> PdfResult<Bytes> {
    decode_with(filter_name, input, &FilterContext::new(params, arena))
}

/// Decodes one filter in the context the stream sits in.
///
/// The whole dispatch: a name becomes a unit, and the unit decodes. Every arm that used
/// to be here — a match with a `FlateFilter` in one branch, a helper for the byte
/// transformations, an inline Zstd decoder, and two image codecs that had escaped
/// the trait entirely — is now one unit each, listed in [`filter_for`].
///
/// # Errors
/// Fails when no filter of that name is decoded, and propagates whatever the filter says
/// about data it cannot decode.
pub fn decode_with(filter_name: &str, input: &[u8], cx: &FilterContext<'_>) -> PdfResult<Bytes> {
    // Table 6 gives every filter an abbreviation for use in inline images (7.8.6), and
    // producers use them: `/AHx` appears seven times in one external file and `/A85` and
    // `/LZW` once each. Only `Fl` and `DCT` were matched before, so a stream naming any
    // of the others by its short name was refused for the wrong reason — not "this engine
    // cannot decode that" but "this engine has not heard of that".
    filter_for(filter_name)
        .ok_or_else(|| PdfError::Filter {
            filter: filter_name.to_string().into(),
            message: format!("Unsupported filter: {filter_name}").into(),
        })?
        .decode(input, cx)
}

/// Orchestrates multi-filter decoding for a stream dictionary.
pub fn process_arena_filters(data: &[u8], dict: &Dict, arena: &PdfArena) -> PdfResult<Bytes> {
    let filter_key = arena.intern_name(crate::object::PdfName::new("Filter"));
    let params_key = arena.intern_name(crate::object::PdfName::new("DecodeParms"));
    let rows = image_rows(dict, arena);

    let mut current_data = Bytes::copy_from_slice(data);

    if let Some(filter_obj) = dict.get(&filter_key) {
        let filter_obj = filter_obj.resolve(arena);
        match filter_obj {
            Object::Name(h) => {
                let name = arena
                    .get_name(h)
                    .ok_or_else(|| PdfError::Other("Filter name not found".into()))?;
                let params = dict.get(&params_key).map(|o| o.resolve(arena));
                let cx = FilterContext::new(params.as_ref(), arena).in_image(rows);
                current_data = decode_with(name.as_str(), &current_data, &cx)?;
            }
            Object::Array(h) => {
                current_data = decode_filter_chain(&current_data, h, dict, arena, rows)?;
            }
            _ => {}
        }
    }

    Ok(current_data)
}

/// A stream filtered more than once (7.4.1): each filter in turn, with the parameters at
/// the same index of `/DecodeParms`.
fn decode_filter_chain(
    data: &[u8],
    filters: crate::handle::Handle<Vec<Object>>,
    dict: &Dict,
    arena: &PdfArena,
    rows: Option<u32>,
) -> PdfResult<Bytes> {
    let params_key = arena.intern_name(crate::object::PdfName::new("DecodeParms"));
    let filters =
        arena.get_array(filters).ok_or_else(|| PdfError::Other("Filter array not found".into()))?;
    let params_arr = dict.get(&params_key).and_then(|o| {
        if let Object::Array(ah) = o.resolve(arena) { arena.get_array(ah) } else { None }
    });

    let mut current_data = Bytes::copy_from_slice(data);
    for (i, f_obj) in filters.iter().enumerate() {
        if let Object::Name(fh) = f_obj.resolve(arena) {
            let name = arena
                .get_name(fh)
                .ok_or_else(|| PdfError::Other("Filter name not found".into()))?;
            let p = params_arr.as_ref().and_then(|a| a.get(i));
            let cx = FilterContext::new(p, arena).in_image(rows);
            current_data = decode_with(name.as_str(), &current_data, &cx)?;
        }
    }
    Ok(current_data)
}

/// What `/SMaskInData` (8.9.5.2, Table 89) says about a `/JPXDecode` image's fourth
/// channel.
///
/// Only this filter can carry one: every other image in a PDF describes its transparency
/// in a separate `/SMask` stream, and this one may have it inside the codestream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SoftMaskInData {
    /// 0, the default: whatever alpha the data carries is not part of the image.
    #[default]
    Ignored,
    /// 1: the data carries a soft mask, and the colour is not premultiplied by it.
    Present,
    /// 2: the data carries a soft mask, and the colour **is** premultiplied by it.
    Premultiplied,
}

impl SoftMaskInData {
    /// Whether the image claims to carry its own mask.
    #[must_use]
    pub const fn expects_a_mask(self) -> bool {
        matches!(self, Self::Present | Self::Premultiplied)
    }
}

/// Reads `/SMaskInData` from an image dictionary.
///
/// # Errors
/// Returns the value the file wrote when it is not one of Table 89's three. The caller
/// records it: 8.9.5.2 defines 0, 1 and 2 and nothing else, and reading a `3` as the
/// default is a choice about non-conforming input rather than a fact about the file.
pub fn soft_mask_in_data(dict: &Dict, arena: &PdfArena) -> Result<SoftMaskInData, i64> {
    let key = arena.intern_name(crate::object::PdfName::new("SMaskInData"));
    match dict.get(&key).and_then(|entry| entry.resolve(arena).as_integer()) {
        None | Some(0) => Ok(SoftMaskInData::Ignored),
        Some(1) => Ok(SoftMaskInData::Present),
        Some(2) => Ok(SoftMaskInData::Premultiplied),
        Some(other) => Err(other),
    }
}

/// An image's samples, with the soft mask its own data carried.
pub struct DecodedImage {
    /// The colour samples, in the layout the dictionary or the codestream gives.
    pub samples: Bytes,
    /// One byte of opacity per pixel, when `/SMaskInData` asked for it **and** the
    /// codestream had a channel to give. `None` for both "not asked for" and "asked for
    /// and not there" — the caller knows which it asked and reports the second.
    pub soft_mask: Option<Bytes>,
}

/// Decodes an image stream, keeping the soft mask `/SMaskInData` asks for.
///
/// The same chain [`process_arena_filters`] runs, with one difference: where the last
/// filter is `/JPXDecode` and `in_data` asks for a mask, that step keeps the alpha
/// instead of dropping it. Last, because 7.4.9 puts `/JPXDecode` there — it produces
/// image samples, so nothing can follow it — and because a filter in the middle of a
/// chain has no alpha to speak of.
///
/// # Errors
/// Fails for the reasons the chain does: a filter this engine cannot decode, or data
/// that will not decode with it.
pub fn decode_image(
    data: &[u8],
    dict: &Dict,
    arena: &PdfArena,
    in_data: SoftMaskInData,
) -> PdfResult<DecodedImage> {
    let chain = named_filters(dict, arena);
    if !in_data.expects_a_mask() || chain.last().map(|(name, _)| name.as_str()) != Some("JPXDecode")
    {
        return Ok(DecodedImage {
            samples: process_arena_filters(data, dict, arena)?,
            soft_mask: None,
        });
    }
    let rows = image_rows(dict, arena);
    let mut current = Bytes::copy_from_slice(data);
    let last = chain.len().saturating_sub(1);
    for (i, (name, params)) in chain.iter().enumerate() {
        if i == last {
            let premultiplied = in_data == SoftMaskInData::Premultiplied;
            let (samples, _, mask) = jpx::decode_keeping_alpha(&current, premultiplied)?;
            return Ok(DecodedImage { samples, soft_mask: mask });
        }
        let cx = FilterContext::new(params.as_ref(), arena).in_image(rows);
        current = decode_with(name, &current, &cx)?;
    }
    Ok(DecodedImage { samples: current, soft_mask: None })
}

/// The filters a stream names, in order, each with the `/DecodeParms` at its index.
///
/// Both of 7.4.1's forms — one name with one parameter dictionary, or an array of each —
/// flattened to the same shape. An element that is not a name is skipped rather than
/// failing the stream, which is what [`decode_filter_chain`] does with it too.
fn named_filters(dict: &Dict, arena: &PdfArena) -> Vec<(String, Option<Object>)> {
    let filter_key = arena.intern_name(crate::object::PdfName::new("Filter"));
    let params_key = arena.intern_name(crate::object::PdfName::new("DecodeParms"));
    let named = |handle| arena.get_name(handle).map(|n| n.as_str().to_string());
    let Some(filters) = dict.get(&filter_key).map(|o| o.resolve(arena)) else { return Vec::new() };
    let params = dict.get(&params_key).map(|o| o.resolve(arena));
    match filters {
        Object::Name(h) => named(h).map(|name| vec![(name, params)]).unwrap_or_default(),
        Object::Array(ah) => {
            let at = |i: usize| match params.as_ref() {
                Some(Object::Array(ph)) => arena.get_array(*ph).and_then(|a| a.get(i).cloned()),
                _ => None,
            };
            arena
                .get_array(ah)
                .unwrap_or_default()
                .iter()
                .enumerate()
                .filter_map(|(i, item)| Some((named(item.resolve(arena).as_name()?)?, at(i))))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// The image's `/Height`, which only `/CCITTFaxDecode` reads and only without its own.
fn image_rows(dict: &Dict, arena: &PdfArena) -> Option<u32> {
    dict.get(&arena.intern_name(crate::object::PdfName::new("Height")))
        .and_then(|o| o.resolve(arena).as_integer())
        .and_then(|h| u32::try_from(h).ok())
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
    fn test_unknown_filter_error() {
        let arena = PdfArena::new();
        let result = decode_stream("NonExistentFilter", b"data", None, &arena);
        assert!(result.is_err());
    }

    /// A grey JPEG decodes to **one** component, not three.
    ///
    /// The image dictionary says `/DeviceGray`, `detect_pixel_format` picks `Gray8`, and
    /// the backend then reads one byte per pixel. While this returned RGB it read the
    /// red channel of every third pixel and ran out of buffer two thirds of the way
    /// down — a visible defect on any grey scan, which is most of them.
    #[test]
    fn a_grey_jpeg_decodes_to_one_component_per_pixel() {
        let arena = PdfArena::new();
        let (w, h) = (8_u32, 4_u32);
        let samples: Vec<u8> = (0..w * h).map(|i| (i * 8) as u8).collect();
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
            .encode(&samples, w, h, image::ExtendedColorType::L8)
            .expect("encodes");

        let decoded = decode_stream("DCTDecode", &jpeg, None, &arena).expect("decodes");
        assert_eq!(
            decoded.len(),
            (w * h) as usize,
            "one byte per pixel, not three: {} bytes for {w}×{h}",
            decoded.len()
        );
        // Lossy at any quality, so the check is that the ramp survived, not that the
        // bytes are identical.
        assert!(decoded[0] < decoded[decoded.len() - 1], "the gradient is still a gradient");
    }

    /// A colour JPEG still decodes to three, which is what `/DeviceRGB` describes.
    #[test]
    fn a_colour_jpeg_still_decodes_to_three() {
        let arena = PdfArena::new();
        let (w, h) = (8_u32, 4_u32);
        let samples: Vec<u8> = (0..w * h * 3).map(|i| (i % 251) as u8).collect();
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
            .encode(&samples, w, h, image::ExtendedColorType::Rgb8)
            .expect("encodes");

        let decoded = decode_stream("DCTDecode", &jpeg, None, &arena).expect("decodes");
        assert_eq!(decoded.len(), (w * h * 3) as usize);
    }

    /// Every name the dispatch table answers to is decoded, and the two questions are
    /// now the same question.
    ///
    /// `is_decoded` used to be a hand-written second list beside the dispatch, and this
    /// test compared them by asking the decoder itself about nineteen names. It is
    /// derived from `filter_for` now, so what is left to check is that a name the table
    /// does not know is refused *by name* — the defect that made `/AHx` unreadable was a
    /// dispatch that had not heard of it, not one that could not decode it.
    #[test]
    fn a_filter_the_table_does_not_know_is_refused_by_name() {
        let arena = PdfArena::new();
        for name in ["XXXDecode", "Crypt", "NoSuchDecode"] {
            assert!(!is_decoded(name), "{name} has no unit");
            let Err(crate::error::PdfError::Filter { message, filter }) =
                decode_stream(name, b"\x01\x02\x03\x04", None, &arena)
            else {
                panic!("{name} must be refused");
            };
            assert_eq!(filter, name, "refused by its own name");
            assert!(message.starts_with("Unsupported filter"), "{message}");
        }
        for name in ["FlateDecode", "Fl", "AHx", "A85", "RL", "CCF", "DCT", "JBIG2Decode"] {
            assert!(is_decoded(name), "{name} has a unit");
        }
    }
}
