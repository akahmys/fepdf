//! `LZWDecode` (7.4.4.2).
//!
//! The other half of clause 7.4.4 — `FlateDecode` is the half this engine had. LZW is
//! what PDF used before 1.2 and what a producer targeting older readers still emits, so a
//! file that needs it is an old file rather than an exotic one. Two of the 242 external
//! files use it and none of the nine in `samples/` does, which is exactly why it was
//! missing.
//!
//! `/EarlyChange` is the detail that makes two implementations disagree: with its default
//! of 1 the code width grows one code sooner than the table filling would suggest. A
//! decoder that ignores it produces plausible-looking rubbish rather than an error, which
//! is the worst failure mode available here.

use crate::PdfResult;
use crate::arena::PdfArena;
use crate::error::PdfError;
use crate::filters::{DecodingFilter, FilterContext, predictor};
use crate::object::Object;
use bytes::Bytes;

/// Matches `FlateDecode`'s ceiling: LZW can expand far more than it compresses, and a
/// crafted stream is a denial-of-service dressed as a document.
const MAX_DECOMPRESSED_SIZE: usize = 128 * 1024 * 1024;

const CLEAR_TABLE: u16 = 256;
const END_OF_DATA: u16 = 257;

/// The `LZWDecode` stream filter.
pub struct LzwFilter;

impl DecodingFilter for LzwFilter {
    fn decode(&self, input: &[u8], cx: &FilterContext<'_>) -> PdfResult<Bytes> {
        let (params, arena) = (cx.params, cx.arena);
        let early_change =
            params.and_then(|p| int_param(p, arena, "EarlyChange")).unwrap_or(1).clamp(0, 1) as u16;
        let mut decoded = decode_lzw(input, early_change)?;
        // Table 8's predictors apply to LZW exactly as they do to Flate.
        if let Some(p) = params {
            decoded = predictor::apply_predictor(&decoded, p, arena)?;
        }
        Ok(Bytes::from(decoded))
    }
}

/// Reads `/DecodeParms` for one integer, when it is a dictionary that carries it.
fn int_param(params: &Object, arena: &PdfArena, key: &str) -> Option<i64> {
    let dict = arena.get_dict(params.resolve(arena).as_dict_handle()?)?;
    dict.get(&arena.get_name_by_str(key)?)?.resolve(arena).as_integer()
}

/// The table of expansions, indexed by code.
///
/// Entries 0–255 are the single bytes, 256 and 257 are the two control codes, and
/// everything from 258 is built as the stream is read.
struct Table {
    entries: Vec<Vec<u8>>,
}

impl Table {
    fn new() -> Self {
        let mut entries: Vec<Vec<u8>> = (0..=255u16).map(|b| vec![b as u8]).collect();
        entries.push(Vec::new()); // 256, clear
        entries.push(Vec::new()); // 257, end of data
        Self { entries }
    }

    fn get(&self, code: u16) -> Option<&Vec<u8>> {
        self.entries.get(code as usize).filter(|e| !e.is_empty() || code < CLEAR_TABLE)
    }

    fn len(&self) -> u16 {
        self.entries.len() as u16
    }

    /// The width the *next* code will be read at.
    ///
    /// `early_change` is why this is not simply "the width that holds `len()`": with the
    /// default of 1 the width grows one entry sooner.
    fn next_width(&self, early_change: u16) -> u32 {
        match self.len() + early_change {
            0..=511 => 9,
            512..=1023 => 10,
            1024..=2047 => 11,
            _ => 12,
        }
    }
}

fn decode_lzw(input: &[u8], early_change: u16) -> PdfResult<Vec<u8>> {
    let mut table = Table::new();
    let mut out: Vec<u8> = Vec::new();
    let mut previous: Option<Vec<u8>> = None;
    let mut bits = BitReader::new(input);

    while let Some(code) = bits.read(table.next_width(early_change)) {
        match code {
            CLEAR_TABLE => {
                table = Table::new();
                previous = None;
                continue;
            }
            END_OF_DATA => break,
            _ => {}
        }

        let entry = match table.get(code) {
            Some(found) => found.clone(),
            // The one legal forward reference: a code for the entry this step is about
            // to add, which expands to the previous string plus its own first byte.
            None if code == table.len() => match &previous {
                Some(prev) => {
                    let mut e = prev.clone();
                    e.push(prev[0]);
                    e
                }
                None => return Err(bad_code(code)),
            },
            None => return Err(bad_code(code)),
        };

        out.extend_from_slice(&entry);
        if out.len() > MAX_DECOMPRESSED_SIZE {
            return Err(PdfError::Filter {
                filter: "LZWDecode".into(),
                message: format!("expanded past the limit of {MAX_DECOMPRESSED_SIZE} bytes").into(),
            });
        }

        if let Some(prev) = previous.replace(entry.clone())
            && table.len() < 4096
        {
            let mut new_entry = prev;
            new_entry.push(entry[0]);
            table.entries.push(new_entry);
        }
    }
    Ok(out)
}

fn bad_code(code: u16) -> PdfError {
    PdfError::Filter {
        filter: "LZWDecode".into(),
        message: format!("code {code} names no table entry").into(),
    }
}

/// Codes are packed most significant bit first and are not byte-aligned.
struct BitReader<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    /// The next `width` bits, or `None` once the data runs out.
    ///
    /// Running out is not an error: 7.4.4.2 requires an `END_OF_DATA` code, and files
    /// exist that simply stop instead. Returning what was decoded so far is what every
    /// other reader does, and erroring would lose a page over a missing two bytes.
    fn read(&mut self, width: u32) -> Option<u16> {
        let mut value = 0u32;
        for _ in 0..width {
            let byte = self.data.get(self.bit / 8)?;
            let shift = 7 - (self.bit % 8);
            value = (value << 1) | u32::from((byte >> shift) & 1);
            self.bit += 1;
        }
        Some(value as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::Table;

    /// `/EarlyChange` moves the code width up one entry sooner, and that is the whole of
    /// what it does. Tested here rather than end to end because the worked example in
    /// 7.4.4.2 is nine bytes long and never reaches a boundary: injecting "ignore
    /// EarlyChange" left every end-to-end test passing, which made them vacuous about
    /// the one parameter most likely to be got wrong.
    #[test]
    fn early_change_moves_the_width_boundary_by_one_entry() {
        let mut table = Table::new();
        // 258 entries to start with. Fill to 510, one below the early boundary.
        while table.len() < 510 {
            table.entries.push(vec![0]);
        }
        assert_eq!(table.next_width(1), 9);
        assert_eq!(table.next_width(0), 9);

        // At 511 the early-change decoder has already widened and the other has not.
        table.entries.push(vec![0]);
        assert_eq!(table.len(), 511);
        assert_eq!(table.next_width(1), 10, "one entry early");
        assert_eq!(table.next_width(0), 9, "still nine without it");

        // And the same one-entry offset at the next two boundaries.
        while table.len() < 1023 {
            table.entries.push(vec![0]);
        }
        assert_eq!(table.next_width(1), 11);
        assert_eq!(table.next_width(0), 10);
        while table.len() < 2047 {
            table.entries.push(vec![0]);
        }
        assert_eq!(table.next_width(1), 12);
        assert_eq!(table.next_width(0), 11);
    }
}
