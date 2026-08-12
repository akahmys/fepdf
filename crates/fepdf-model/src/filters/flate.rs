//! FlateDecode Filter (ISO 32000-2:2020 Clause 7.4.4)

use crate::PdfResult;
use crate::arena::PdfArena;
use crate::error::PdfError;
use crate::filters::{DecodingFilter, predictor};
use crate::object::Object;
use bytes::Bytes;
use flate2::read::ZlibDecoder;
use std::io::Read;

const MAX_DECOMPRESSED_SIZE: u64 = 128 * 1024 * 1024; // 128 MB

/// The `FlateDecode` stream filter.
pub struct FlateFilter;

impl DecodingFilter for FlateFilter {
    fn decode(&self, input: &[u8], params: Option<&Object>, arena: &PdfArena) -> PdfResult<Bytes> {
        let mut decoder = ZlibDecoder::new(input).take(MAX_DECOMPRESSED_SIZE + 1);
        let mut decoded = Vec::new();

        decoder.read_to_end(&mut decoded).map_err(|e| PdfError::Filter {
            filter: "FlateDecode".into(),
            message: format!("Flate decompression failed: {e}").into(),
        })?;

        if decoded.len() as u64 > MAX_DECOMPRESSED_SIZE {
            return Err(PdfError::Filter {
                filter: "FlateDecode".into(),
                message: format!(
                    "Decompressed stream size exceeded limit of {MAX_DECOMPRESSED_SIZE} bytes"
                )
                .into(),
            });
        }

        // Apply predictors if present in DecodeParms
        if let Some(p) = params {
            decoded = predictor::apply_predictor(&decoded, p, arena)?;
        }

        Ok(Bytes::from(decoded))
    }
}
