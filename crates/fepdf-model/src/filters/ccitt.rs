//! `/CCITTFaxDecode` (7.4.6, Table 12) — the compression of a scanned page.
//!
//! Group 3 and Group 4 are what a fax machine and a document scanner produce: one bit
//! per pixel, run-length coded along and between scan lines. ROADMAP Phase L declined
//! this codec on a measurement — two files of 251, costing 3.9% of a page each — and
//! Phase M reopens it on the trigger that phase named, which was never the corpus count
//! but the use case. A corpus of born-digital PDFs cannot tell you how common a scan is.
//!
//! The decoding is [`hayro_ccitt`]: pure Rust, `unsafe` forbidden by a crate attribute,
//! and shared with the JBIG2 decoder, whose generic regions may be MMR-coded — and MMR
//! *is* T.6. What this module owns is the *PDF* half: Table 12's parameters, and packing
//! the result the way an image dictionary says it will be laid out.
//!
//! **The output is one bit per pixel with each row starting on a byte boundary**, which
//! is what 8.9.5.1 says image data is and what `/ImageMask` images in the corpus expect.
//! The filter does not expand it to bytes and does not convert it to a colour: a filter
//! returns samples, which is the rule `DCTDecode` was found breaking.

use crate::PdfResult;
use crate::arena::PdfArena;
use crate::error::PdfError;
use crate::filters::bilevel::Bitmap;
use crate::object::Object;
use bytes::Bytes;
use hayro_ccitt::{DecodeSettings, Decoder, DecoderContext, EncodingMode};

/// Table 12's defaults, for the entries a file may leave out.
const DEFAULT_COLUMNS: u32 = 1728;

/// The unit [`crate::filters::filter_for`] hands `/CCITTFaxDecode` to.
///
/// Swapping the decoder underneath means writing another unit and changing one arm of
/// that table. Nothing outside this module knows which crate does the decoding.
pub struct CcittFilter;

impl crate::filters::DecodingFilter for CcittFilter {
    fn decode(&self, input: &[u8], cx: &crate::filters::FilterContext<'_>) -> PdfResult<Bytes> {
        decode(input, cx.params, cx.arena, cx.image_rows)
    }
}

/// Decodes a CCITT stream into one-bit samples.
///
/// `image_rows` is the image dictionary's `/Height`, which `/Rows` defaults to when the
/// parameters omit it. It is the one thing this filter needs that its own parameters do
/// not carry — a filter that decoded to `/Rows` of zero would return an empty image for
/// most of the files that use it.
///
/// # Errors
/// Fails when the coded data does not decode: a truncated stream, an invalid code, or a
/// scan line of the wrong length.
pub fn decode(
    input: &[u8],
    params: Option<&Object>,
    arena: &PdfArena,
    image_rows: Option<u32>,
) -> PdfResult<Bytes> {
    let get = |key: &str| -> Option<Object> {
        let dict = arena.get_dict(params?.resolve(arena).as_dict_handle()?)?;
        Some(dict.get(&arena.name(key))?.resolve(arena))
    };
    let integer = |key: &str| get(key).and_then(|o| o.as_integer());
    let flag = |key: &str, default: bool| get(key).and_then(|o| o.as_bool()).unwrap_or(default);

    let columns = u32::try_from(integer("Columns").unwrap_or(i64::from(DEFAULT_COLUMNS)))
        .unwrap_or(DEFAULT_COLUMNS);
    let rows = integer("Rows")
        .and_then(|r| u32::try_from(r).ok())
        .filter(|r| *r > 0)
        .or(image_rows)
        .unwrap_or(0);
    if columns == 0 {
        return Err(refuse("/Columns is zero, so the image has no samples"));
    }

    // `/K` chooses the coding, and its *sign* is what matters (Table 12): negative is
    // pure two-dimensional Group 4, zero is one-dimensional Group 3, and positive is
    // Group 3 with up to K two-dimensional lines after each one-dimensional one.
    let k = integer("K").unwrap_or(0);
    let encoding = match k {
        k if k < 0 => EncodingMode::Group4,
        0 => EncodingMode::Group3_1D,
        k => EncodingMode::Group3_2D { k: u32::try_from(k).unwrap_or(1) },
    };

    let settings = DecodeSettings {
        columns,
        rows,
        // Table 12 defaults `/EndOfBlock` to true; when it is, the decoder may stop at
        // the marker before `rows` is reached, which is how a file that overstates its
        // height still decodes.
        end_of_block: flag("EndOfBlock", true),
        end_of_line: flag("EndOfLine", false),
        rows_are_byte_aligned: flag("EncodedByteAlign", false),
        encoding,
        // `/BlackIs1` decides which bit value is black, and the default is that **0 is
        // black** — which is also what `PixelFormat::MonoMask` means by "0 paints the
        // fill colour", so a scanned mask paints its ink and not its paper.
        invert_black: flag("BlackIs1", false),
    };

    let mut page = Page(Bitmap::new(columns, rows));
    hayro_ccitt::decode(input, &mut page, &mut DecoderContext::new(settings))
        .map_err(|e| refuse(&format!("{e:?} after {} of {rows} rows", page.0.rows_done)))?;
    Ok(Bytes::from(page.0.finish()))
}

fn refuse(why: &str) -> PdfError {
    PdfError::Filter { filter: "CCITTFaxDecode".into(), message: why.to_string().into() }
}

/// Adapts `hayro-ccitt`'s whiteness to the shared bilevel packer, which stores the
/// same thing — so this is the codec whose convention needs no inversion.
struct Page(Bitmap);

impl Decoder for Page {
    fn push_pixel(&mut self, white: bool) {
        self.0.push(white);
    }

    fn push_pixel_chunk(&mut self, white: bool, chunk_count: u32) {
        self.0.push_bytes(white, chunk_count);
    }

    fn next_line(&mut self) {
        self.0.next_line();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parameters, as a file writes them.
    fn parms(arena: &PdfArena, source: &str) -> Object {
        let mut parser =
            crate::parser::Parser::new(bytes::Bytes::from(format!("{source}\n")), arena);
        parser.parse_object().expect("parses")
    }

    /// A one-bit bitmap, packed the way PDF lays image samples out, encoded as Group 4.
    ///
    /// Encoded with `hayro-ccitt`'s own decoder in reverse is not possible — it has no
    /// encoder — so the fixture is a byte sequence taken from a known-good Group 4
    /// stream: eight columns by two rows, all white. In Group 4 an all-white line is a
    /// single vertical-mode code against an imaginary all-white reference line.
    #[test]
    fn a_group_four_stream_decodes_to_one_bit_per_pixel() {
        let arena = PdfArena::new();
        // V0 (`1`) codes "the change is where the reference says", and with a white
        // reference line that is the end of the row. Two rows, then EOFB.
        let coded = [0b1100_0000_u8, 0x00, 0x10, 0x01];
        let params = parms(&arena, "<< /K -1 /Columns 8 /Rows 2 /BlackIs1 false >>");

        let out = decode(&coded, Some(&params), &arena, None).expect("decodes");
        assert_eq!(out.len(), 2, "eight columns is one byte a row, two rows");
        assert_eq!(&out[..], &[0xFF, 0xFF], "all white, and white is a 1 bit");
    }

    /// `/BlackIs1` flips which bit value is ink.
    #[test]
    fn black_is_one_inverts_the_samples() {
        let arena = PdfArena::new();
        let coded = [0b1100_0000_u8, 0x00, 0x10, 0x01];
        let params = parms(&arena, "<< /K -1 /Columns 8 /Rows 2 /BlackIs1 true >>");

        let out = decode(&coded, Some(&params), &arena, None).expect("decodes");
        assert_eq!(&out[..], &[0x00, 0x00], "the same page, with white as 0");
    }

    /// `/Rows` falls back to the image's `/Height`, which is where most files keep it.
    #[test]
    fn rows_falls_back_to_the_image_height() {
        let arena = PdfArena::new();
        let coded = [0b1100_0000_u8, 0x00, 0x10, 0x01];
        let params = parms(&arena, "<< /K -1 /Columns 8 >>");

        let out = decode(&coded, Some(&params), &arena, Some(2)).expect("decodes");
        assert_eq!(out.len(), 2, "two rows, from /Height");
    }

    /// A picture, encoded by one implementation and decoded by another.
    ///
    /// The fixtures above are hand-written byte sequences that decode to a flat colour,
    /// which checks the plumbing and nothing about the coding. This encodes a **pattern**
    /// with `fax` — a Group 4 encoder that is not the decoder under test — and asks for
    /// it back. Neither crate could give this evidence alone.
    ///
    /// No sample of a real scan exists in either corpus, so a second implementation on
    /// the same bytes is the strongest check available without one.
    #[test]
    fn a_pattern_survives_another_implementation_of_group_four() {
        use fax::Color;

        // Sixteen columns by four rows: a black square in the top-left quarter.
        let (columns, rows) = (16_u16, 4_u16);
        let black_here = |x: u16, y: u16| x < 8 && y < 2;
        let mut encoder = fax::encoder::Encoder::new(fax::VecWriter::new());
        for y in 0..rows {
            encoder
                .encode_line(
                    (0..columns)
                        .map(|x| if black_here(x, y) { Color::Black } else { Color::White }),
                    columns,
                )
                .expect("encodes");
        }
        let coded = encoder.finish().expect("finishes").finish();

        let arena = PdfArena::new();
        let params = parms(&arena, &format!("<< /K -1 /Columns {columns} /Rows {rows} >>"));
        let out = decode(&coded, Some(&params), &arena, None).expect("decodes");

        assert_eq!(out.len(), 2 * rows as usize, "two bytes a row");
        for y in 0..rows as usize {
            let (left, right) = (out[y * 2], out[y * 2 + 1]);
            if y < 2 {
                assert_eq!(left, 0x00, "row {y}: the left half is black, and black is 0");
                assert_eq!(right, 0xFF, "row {y}: the right half is white");
            } else {
                assert_eq!((left, right), (0xFF, 0xFF), "row {y}: all white");
            }
        }
    }

    /// A stream that runs out mid-page is refused, and the message says how far it got.
    ///
    /// The test written first fed it `DEADBEEF` on the assumption that would not be a
    /// fax. It decoded — to four white rows. CCITT is a bit stream of run codes and a
    /// great deal of arbitrary data is *valid*, which is worth knowing before trusting
    /// "it decoded" to mean "it was one".
    #[test]
    fn a_stream_that_runs_out_is_refused_with_its_progress() {
        let arena = PdfArena::new();
        let params = parms(&arena, "<< /K -1 /Columns 1728 /Rows 400 /EndOfBlock false >>");
        let err = decode(&[0xDE, 0xAD], Some(&params), &arena, None).expect_err("truncated");
        let PdfError::Filter { filter, message } = err else { panic!("a filter error") };
        assert_eq!(filter, "CCITTFaxDecode");
        assert!(message.contains("of 400 rows"), "it says how far it got: {message}");
    }
}
