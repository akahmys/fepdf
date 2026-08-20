//! `ASCIIHexDecode` (7.4.2) and `ASCII85Decode` (7.4.3).
//!
//! Both turn printable ASCII back into bytes and neither compresses anything. They exist
//! so a stream can survive a transport that is not eight-bit clean, which is why they are
//! usually the *first* filter in a `/Filter` array with a real compressor behind them.
//!
//! Absent until a corpus this project did not choose was measured. All nine files in
//! `samples/` are `FlateDecode` and `DCTDecode` and nothing else, so nothing had ever
//! asked; among 242 external files, `/ASCIIHexDecode` appears in two and the abbreviation
//! `/AHx` seven times in one more.

use crate::PdfResult;
use crate::error::PdfError;
use crate::filters::{DecodingFilter, FilterContext};
use bytes::Bytes;

/// `ASCIIHexDecode` (7.4.2): pairs of hex digits, `>` ends the data.
pub struct AsciiHexFilter;

impl DecodingFilter for AsciiHexFilter {
    fn decode(&self, input: &[u8], _cx: &FilterContext<'_>) -> PdfResult<Bytes> {
        let mut out = Vec::with_capacity(input.len() / 2);
        let mut high: Option<u8> = None;
        for &byte in input {
            if byte == b'>' {
                break;
            }
            if byte.is_ascii_whitespace() || byte == 0 {
                continue;
            }
            let Some(nibble) = hex_value(byte) else {
                return Err(PdfError::Filter {
                    filter: "ASCIIHexDecode".into(),
                    message: format!("'{}' is not a hexadecimal digit", byte as char).into(),
                });
            };
            match high.take() {
                Some(first) => out.push((first << 4) | nibble),
                None => high = Some(nibble),
            }
        }
        // 7.4.2: an odd number of digits means the last one is followed by an implicit
        // zero. Discarding it instead would silently drop half a byte of real data.
        if let Some(first) = high {
            out.push(first << 4);
        }
        Ok(Bytes::from(out))
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// `ASCII85Decode` (7.4.3): base-85 in groups of five, `~>` ends the data.
pub struct Ascii85Filter;

impl DecodingFilter for Ascii85Filter {
    fn decode(&self, input: &[u8], _cx: &FilterContext<'_>) -> PdfResult<Bytes> {
        let mut out = Vec::with_capacity(input.len() * 4 / 5);
        let mut group = [0u8; 5];
        let mut n = 0;
        // A leading `<~` is not in 7.4.3 — it belongs to Adobe's standalone encoding —
        // but producers emit it, and refusing the whole stream over two bytes that carry
        // no data would be reading the clause more strictly than the files behave.
        let mut data = input;
        if data.starts_with(b"<~") {
            data = &data[2..];
        }

        for &byte in data {
            if byte == b'~' {
                break;
            }
            if byte.is_ascii_whitespace() || byte == 0 {
                continue;
            }
            // `z` stands for a whole group of four zero bytes, and only between groups.
            if byte == b'z' && n == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            group[n] = digit(byte)?;
            n += 1;
            if n == 5 {
                push_group(&mut out, &group, 5);
                n = 0;
            }
        }
        finish_group(&mut out, &mut group, n)?;
        Ok(Bytes::from(out))
    }
}

/// One base-85 digit, or an error naming the character that is not one.
fn digit(byte: u8) -> PdfResult<u8> {
    if (b'!'..=b'u').contains(&byte) {
        Ok(byte - b'!')
    } else {
        Err(PdfError::Filter {
            filter: "ASCII85Decode".into(),
            message: format!("'{}' is outside the base-85 alphabet", byte as char).into(),
        })
    }
}

/// The last, possibly short, group.
fn finish_group(out: &mut Vec<u8>, group: &mut [u8; 5], n: usize) -> PdfResult<()> {
    if n == 0 {
        return Ok(());
    }
    if n == 1 {
        return Err(PdfError::Filter {
            filter: "ASCII85Decode".into(),
            message: "a final group of one character encodes no byte (7.4.3)".into(),
        });
    }
    // A short final group is padded with the *highest* digit, and yields one fewer byte
    // than it has characters.
    for slot in group.iter_mut().skip(n) {
        *slot = 84;
    }
    push_group(out, group, n);
    Ok(())
}

/// Expands one base-85 group, keeping `count - 1` of the four bytes it produces.
fn push_group(out: &mut Vec<u8>, group: &[u8; 5], count: usize) {
    let mut value = 0u32;
    for &digit in group {
        value = value.wrapping_mul(85).wrapping_add(u32::from(digit));
    }
    let bytes = value.to_be_bytes();
    out.extend_from_slice(&bytes[..count - 1]);
}
