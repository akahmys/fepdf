//! `RunLengthDecode` (7.4.5).
//!
//! **Measured at zero occurrences** — not in the nine files in `samples/`, and not in the
//! 242 external ones either. It is here anyway, which is a departure from this project's
//! habit of refusing to build what nothing reaches, and the reason is that the rule is
//! about *containers*: a type hierarchy nobody fills is dead weight that has to be
//! maintained in step with everything around it. This is a leaf function with a fixed,
//! forty-year-old definition and no dependants, and it sits beside three filters from the
//! same clause that the corpus does exercise. Leaving the one gap would make a file that
//! is legal, simple and old fail for no reason a user could act on.

use crate::PdfResult;
use crate::arena::PdfArena;
use crate::filters::DecodingFilter;
use crate::object::Object;
use bytes::Bytes;

const END_OF_DATA: u8 = 128;

/// The `RunLengthDecode` stream filter.
pub struct RunLengthFilter;

impl DecodingFilter for RunLengthFilter {
    fn decode(
        &self,
        input: &[u8],
        _params: Option<&Object>,
        _arena: &PdfArena,
    ) -> PdfResult<Bytes> {
        let mut out = Vec::with_capacity(input.len() * 2);
        let mut i = 0;
        while let Some(&length) = input.get(i) {
            i += 1;
            if length == END_OF_DATA {
                break;
            }
            if length < END_OF_DATA {
                // 0..=127: the next length + 1 bytes are literal.
                let count = length as usize + 1;
                let end = (i + count).min(input.len());
                out.extend_from_slice(&input[i..end]);
                i = end;
            } else {
                // 129..=255: the next byte, repeated 257 - length times.
                let Some(&byte) = input.get(i) else { break };
                i += 1;
                out.extend(std::iter::repeat_n(byte, 257 - length as usize));
            }
        }
        Ok(Bytes::from(out))
    }
}
