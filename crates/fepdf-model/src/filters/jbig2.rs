//! `/JBIG2Decode` (7.4.7) — the compression of the text on a scanned page.
//!
//! JBIG2 finds the repeated shapes in a bilevel image, codes each one once into a
//! *symbol dictionary*, and then codes the page as placements of those symbols. That is
//! why it beats CCITT on scanned text by a wide margin, and why it needs the entry this
//! module exists to honour.
//!
//! **`/JBIG2Globals` is not optional in practice.** A producer that compresses a hundred
//! pages puts the symbol dictionary in one stream shared by all of them, and each page's
//! own stream then codes nothing but placements. Without that entry such a page decodes
//! to blank paper — which is why Phase M named it as the one thing to settle before
//! writing this module. `hayro_jbig2::Image::new_embedded(data, globals)` takes it, and
//! it is the *embedded* organisation of Annex D.3 that PDF uses rather than a standalone
//! JBIG2 file.
//!
//! **The two conventions are opposite, and inverting is the filter's job.** A JBIG2
//! codestream says 1 for black; a PDF image of one bit per component says 0 for black.
//! `hayro-jbig2` reports blackness and [`super::bilevel::Bitmap`] stores whiteness, so
//! the inversion happens where the two meet and nothing downstream has to know.

use crate::PdfResult;
use crate::error::PdfError;
use crate::filters::bilevel::Bitmap;
use crate::filters::{DecodingFilter, FilterContext};
use crate::object::Object;
use bytes::Bytes;

/// The unit [`crate::filters::filter_for`] hands `/JBIG2Decode` to.
pub struct Jbig2Filter;

impl DecodingFilter for Jbig2Filter {
    fn decode(&self, input: &[u8], cx: &FilterContext<'_>) -> PdfResult<Bytes> {
        // The globals are borrowed by the parsed image, so they outlive it here.
        let globals = globals_of(cx);
        decode(input, globals.as_deref())
    }
}

/// Decodes an embedded JBIG2 image into one-bit samples.
///
/// # Errors
/// Fails when the segments do not parse or the page cannot be assembled — a truncated
/// stream, or a page whose symbol dictionary is in globals that were not supplied.
pub fn decode(input: &[u8], globals: Option<&[u8]>) -> PdfResult<Bytes> {
    let image = hayro_jbig2::Image::new_embedded(input, globals)
        .map_err(|e| refuse(&format!("{e:?}"), globals.is_some()))?;

    let mut page = Page(Bitmap::new(image.width(), image.height()));
    image.decode(&mut page).map_err(|e| {
        refuse(
            &format!("{e:?} after {} of {} rows", page.0.rows_done, image.height()),
            globals.is_some(),
        )
    })?;
    Ok(Bytes::from(page.0.finish()))
}

/// The `/JBIG2Globals` stream from the decode parameters, decoded.
///
/// The globals are a stream like any other and are usually `/FlateDecode`d themselves,
/// so they go through the filter pipeline before they are JBIG2 segments.
fn globals_of(cx: &FilterContext<'_>) -> Option<Bytes> {
    let arena = cx.arena;
    let params = arena.get_dict(cx.params?.resolve(arena).as_dict_handle()?)?;
    let entry = params.get(&arena.name("JBIG2Globals"))?;
    let Object::Stream(dh, ref data) = entry.resolve(arena) else {
        return None;
    };
    let raw = arena.get_stream_bytes(data).ok()?;
    let dict = arena.get_dict(dh)?;
    super::process_arena_filters(&raw, &dict, arena).ok()
}

/// Says whether globals were supplied, because "this page decodes to nothing" and "this
/// page's symbol dictionary was somewhere else" look identical without it.
fn refuse(why: &str, had_globals: bool) -> PdfError {
    let globals = if had_globals { "with /JBIG2Globals" } else { "and no /JBIG2Globals was given" };
    PdfError::Filter { filter: "JBIG2Decode".into(), message: format!("{why} ({globals})").into() }
}

/// Adapts `hayro-jbig2`'s blackness to the bitmap's whiteness.
struct Page(Bitmap);

impl hayro_jbig2::Decoder for Page {
    fn push_pixel(&mut self, black: bool) {
        self.0.push(!black);
    }

    fn push_pixel_chunk(&mut self, black: bool, chunk_count: u32) {
        self.0.push_bytes(!black, chunk_count);
    }

    fn next_line(&mut self) {
        self.0.next_line();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One segment, in the form Annex D.3 embeds (T.88 §7.2).
    ///
    /// Hand-assembled because the alternative was a dev-dependency on a JBIG2 encoder
    /// that reads its input through leptonica — a C library, for a test fixture. Forty
    /// lines of byte layout is the cheaper of the two, and it exercises the segment
    /// parser rather than trusting a second implementation to agree.
    fn segment(number: u32, kind: u8, data: &[u8]) -> Vec<u8> {
        let mut out = number.to_be_bytes().to_vec();
        out.push(kind); // flags: type in the low six bits, one-byte page association
        out.push(0x00); // no referred-to segments
        out.push(0x01); // page 1
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        out.extend_from_slice(data);
        out
    }

    /// A page of `columns × rows` coded as one MMR generic region of white.
    ///
    /// MMR is T.6 — the same coding `/CCITTFaxDecode` reads with `/K` negative — which
    /// is why `hayro-jbig2` depends on `hayro-ccitt`. An all-white row under MMR is a
    /// single V0 code against the imaginary white reference line, so `rows` rows are
    /// `rows` one-bits.
    fn coded_page(columns: u32, rows: u32, default_black: bool) -> Vec<u8> {
        let mut page_info = Vec::new();
        page_info.extend_from_slice(&columns.to_be_bytes());
        page_info.extend_from_slice(&rows.to_be_bytes());
        page_info.extend_from_slice(&0_u32.to_be_bytes()); // x resolution, unstated
        page_info.extend_from_slice(&0_u32.to_be_bytes()); // y resolution, unstated
        // Bit 0: the page is lossless. Bit 2: the pixel value the page starts as
        // (§7.4.8.5), which the region below is then combined onto by OR — so a black
        // page stays black whatever white is written over it.
        page_info.push(if default_black { 0x05 } else { 0x01 });
        page_info.extend_from_slice(&0_u16.to_be_bytes()); // not striped

        let mut region = Vec::new();
        region.extend_from_slice(&columns.to_be_bytes());
        region.extend_from_slice(&rows.to_be_bytes());
        region.extend_from_slice(&0_u32.to_be_bytes()); // at x = 0
        region.extend_from_slice(&0_u32.to_be_bytes()); // at y = 0
        region.push(0x00); // combine by OR
        region.push(0x01); // generic region flags: MMR
        // `rows` V0 codes, each a single 1 bit, packed from the most significant end.
        let mut coded = 0_u8;
        for i in 0..rows.min(8) {
            coded |= 0x80 >> i;
        }
        region.push(coded);

        let mut out = segment(0, 48, &page_info); // page information
        out.extend(segment(1, 38, &region)); // immediate generic region
        out
    }

    /// A JBIG2 page decodes, and **white comes back as a 1 bit**.
    ///
    /// The inversion is the part worth checking: a JBIG2 codestream says 1 for black and
    /// a PDF image of one bit per component says 0 for black, so a filter that passed
    /// the samples through would render every scan as its own negative. Reasoning says
    /// invert; this says the reasoning was right.
    #[test]
    fn a_page_decodes_and_white_is_a_one_bit() {
        let out = decode(&coded_page(16, 2, false), None).expect("decodes");
        assert_eq!(out.len(), 4, "sixteen columns is two bytes a row, two rows");
        assert_eq!(&out[..], &[0xFF, 0xFF, 0xFF, 0xFF], "all white, and white is a 1 bit");

        // The other direction, so the first assertion cannot be passing on a bitmap that
        // was never written to: the same region over a page whose default pixel is black.
        let out = decode(&coded_page(16, 2, true), None).expect("decodes");
        assert_eq!(&out[..], &[0x00, 0x00, 0x00, 0x00], "all black, and black is a 0 bit");
    }

    /// A stream that is not JBIG2 is refused, and the message says whether globals were
    /// in play — the difference between "this is not JBIG2" and "its dictionary was
    /// somewhere I was not given".
    #[test]
    fn a_stream_that_is_not_jbig2_says_whether_globals_were_supplied() {
        let err = decode(&[0xDE, 0xAD, 0xBE, 0xEF], None).expect_err("not jbig2");
        let PdfError::Filter { filter, message } = err else { panic!("a filter error") };
        assert_eq!(filter, "JBIG2Decode");
        assert!(message.contains("no /JBIG2Globals"), "{message}");

        let err = decode(&[0xDE, 0xAD, 0xBE, 0xEF], Some(&[])).expect_err("not jbig2");
        let PdfError::Filter { message, .. } = err else { panic!("a filter error") };
        assert!(message.contains("with /JBIG2Globals"), "{message}");
    }

    /// An empty stream is refused rather than returning an empty page, which would
    /// render as a blank sheet with nothing said.
    #[test]
    fn an_empty_stream_is_refused() {
        assert!(decode(&[], None).is_err());
    }
}
