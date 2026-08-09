use crate::lexer::{Lexer, Token};
use crate::{PdfArena, PdfResult};
use std::collections::BTreeMap;

/// A utility for rewriting PDF content streams.
pub struct ContentRewriter<'a> {
    _arena: &'a PdfArena,
    data: bytes::Bytes,
}

impl<'a> ContentRewriter<'a> {
    pub fn new(arena: &'a PdfArena, data: bytes::Bytes) -> Self {
        Self { _arena: arena, data }
    }

    pub fn rewrite(&self) -> PdfResult<Vec<u8>> {
        let mut lexer = Lexer::new(self.data.clone());
        let mut output = Vec::new();

        while let Ok(token) = lexer.next_token() {
            if token == Token::EOF {
                break;
            }
            token.write_to(&mut output);
        }

        Ok(output)
    }

    /// Inserts Marked Content (BDC/EMC) based on operator index.
    pub fn insert_mcids(&self, mapping: BTreeMap<usize, (&str, i32)>) -> PdfResult<Vec<u8>> {
        let mut lexer = Lexer::new(self.data.clone());
        let mut output = Vec::new();
        let mut op_index = 0;

        while let Ok(token) = lexer.next_token() {
            if token == Token::EOF {
                break;
            }

            if let Token::Keyword(_) = &token {
                if let Some((tag, mcid)) = mapping.get(&op_index) {
                    // Start BDC
                    output.extend_from_slice(
                        format!("/{} << /MCID {} >> BDC ", tag, mcid).as_bytes(),
                    );
                    token.write_to(&mut output);
                    output.extend_from_slice(b"EMC ");
                } else {
                    token.write_to(&mut output);
                }
                op_index += 1;
            } else {
                token.write_to(&mut output);
            }
        }

        Ok(output)
    }
}
