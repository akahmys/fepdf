//! Packing one-bit samples the way PDF lays image data out.
//!
//! Both bilevel codecs — `/CCITTFaxDecode` and `/JBIG2Decode` — hand their output back a
//! pixel or a byte at a time through a callback, and both need it packed the same way:
//! **one bit per pixel, most significant bit first, each row starting on a byte
//! boundary** (8.9.5.1). A 1,728-column fax is 216 bytes a row exactly; a 1,729-column
//! one is 217, with seven bits of padding that belong to no pixel.
//!
//! **White is a 1 bit and black is a 0**, which is what an image dictionary declaring
//! `/DeviceGray` and one bit per component means, and what `PixelFormat::MonoMask` means
//! by "0 paints the fill colour" — so a scanned page paints its ink and not its paper.
//! The two codecs arrive at that convention from opposite directions: CCITT reports
//! whiteness and JBIG2 reports blackness, which is why each adapter says which it has
//! rather than sharing a `bool` whose meaning depends on the caller.

/// A bilevel page, packed as PDF lays image samples out.
pub(crate) struct Bitmap {
    data: Vec<u8>,
    /// How many pixels of the current row have been written.
    x: u32,
    /// Completed rows, which is what an error message needs to be diagnosable.
    pub(crate) rows_done: u32,
}

impl Bitmap {
    /// Room for `columns × rows`, which is a hint and not a limit — a codestream that
    /// disagrees with the dictionary still writes what it has.
    pub(crate) fn new(columns: u32, rows: u32) -> Self {
        let stride = (columns as usize).div_ceil(8);
        Self { data: Vec::with_capacity(stride * rows as usize), x: 0, rows_done: 0 }
    }

    /// One pixel.
    pub(crate) fn push(&mut self, white: bool) {
        let bit = self.x % 8;
        if bit == 0 {
            self.data.push(0);
        }
        if white && let Some(byte) = self.data.last_mut() {
            *byte |= 0x80 >> bit;
        }
        self.x += 1;
    }

    /// Whole bytes of one colour, which both codecs offer for runs and neither offers
    /// off a byte boundary.
    pub(crate) fn push_bytes(&mut self, white: bool, count: u32) {
        debug_assert!(self.x.is_multiple_of(8), "a chunk must start on a byte boundary");
        self.data.extend(std::iter::repeat_n(if white { 0xFF } else { 0x00 }, count as usize));
        self.x += count * 8;
    }

    /// Completes the current row, so the next one starts on a byte boundary.
    pub(crate) fn next_line(&mut self) {
        self.pad_row();
        self.rows_done += 1;
        self.x = 0;
    }

    /// The page, with the last row completed.
    pub(crate) fn finish(mut self) -> Vec<u8> {
        self.pad_row();
        self.data
    }

    fn pad_row(&mut self) {
        if !self.x.is_multiple_of(8) {
            // The partial byte is already in `data`; the remaining bits stay 0.
            self.x += 8 - (self.x % 8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row that is not a multiple of eight columns still starts the next one on a byte
    /// boundary — the padding belongs to no pixel (8.9.5.1).
    #[test]
    fn a_row_is_padded_to_a_byte_boundary() {
        let mut bitmap = Bitmap::new(12, 2);
        for _ in 0..12 {
            bitmap.push(true);
        }
        bitmap.next_line();
        for _ in 0..12 {
            bitmap.push(false);
        }
        let out = bitmap.finish();
        assert_eq!(out.len(), 4, "twelve columns is two bytes a row, two rows");
        assert_eq!(out[0], 0xFF);
        assert_eq!(out[1], 0xF0, "four bits of the second byte are padding");
        assert_eq!(&out[2..], &[0x00, 0x00]);
    }

    /// A run of whole bytes lands in the same place a pixel at a time would.
    #[test]
    fn a_run_of_bytes_matches_the_same_pixels_one_at_a_time() {
        let mut by_byte = Bitmap::new(16, 1);
        by_byte.push_bytes(true, 2);

        let mut by_pixel = Bitmap::new(16, 1);
        for _ in 0..16 {
            by_pixel.push(true);
        }
        assert_eq!(by_byte.finish(), by_pixel.finish());
    }
}
