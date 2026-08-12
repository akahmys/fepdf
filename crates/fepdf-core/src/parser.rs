//! ISO 32000-2:2020 Clause 7.3 - Objects

use crate::PdfResult;
use crate::arena::PdfArena;
use crate::error::PdfError;
use crate::handle::Handle;
use crate::lexer::{Lexer, Token};
use crate::object::{Object, PdfName};
use bytes::Bytes;
use std::collections::BTreeMap;

const MAX_RECURSION_DEPTH: usize = 512;

/// Builds arena objects from a token stream.
pub struct Parser<'a> {
    lexer: Lexer,
    arena: &'a PdfArena,
    depth: usize,
}

impl<'a> Parser<'a> {
    /// Parses `data`, allocating into `arena`.
    pub fn new(data: Bytes, arena: &'a PdfArena) -> Self {
        Self { lexer: Lexer::new(data), arena, depth: 0 }
    }

    /// Reads the next token without consuming it.
    pub fn peek(&mut self) -> PdfResult<Token> {
        self.lexer.peek()
    }

    /// Reads and consumes the next token.
    pub fn next_token(&mut self) -> PdfResult<Token> {
        self.lexer.next_token()
    }

    /// Parses a single PDF object from the token stream.
    pub fn parse_object(&mut self) -> PdfResult<Object> {
        if self.depth >= MAX_RECURSION_DEPTH {
            return Err(PdfError::Parse {
                pos: self.lexer.pos(),
                message: "Exceeded maximum recursion depth limit (512)".into(),
            });
        }

        self.depth += 1;
        let res = self.parse_object_internal();
        self.depth -= 1;
        res
    }

    fn parse_object_internal(&mut self) -> PdfResult<Object> {
        let token = self.lexer.next_token()?;
        match token {
            Token::Boolean(b) => Ok(Object::Boolean(b)),
            Token::Integer(i) => {
                // Peek to see if it's the start of an indirect reference (R)
                let saved_pos = self.lexer.pos();
                if let Ok(Token::Integer(gen_num)) = self.lexer.next_token()
                    && gen_num >= 0
                    && let Ok(Token::Keyword(ref k)) = self.lexer.peek()
                    && k == "R"
                {
                    let _ = self.lexer.next_token(); // consume "R"
                    let obj_id = u32::try_from(i).map_err(|_| PdfError::Parse {
                        pos: saved_pos,
                        message: format!("Invalid object number: {i}").into(),
                    })?;
                    return Ok(Object::Reference(Handle::new(obj_id)));
                }
                // Backtrack if it's not an indirect reference
                self.lexer.set_pos(saved_pos);
                Ok(Object::Integer(i))
            }
            Token::Real(f) => Ok(Object::Real(f)),
            Token::String(s) => Ok(Object::String(s)),
            Token::Hex(s) => Ok(Object::Hex(s)),
            Token::Name(n) => {
                let name_h = self.arena.intern_name(PdfName::from_bytes(&n));
                Ok(Object::Name(name_h))
            }
            Token::Null => Ok(Object::Null),
            Token::LeftArray => self.parse_array(),
            Token::LeftDict => self.parse_dict(),
            Token::EOF => {
                Err(PdfError::Parse { pos: self.lexer.pos(), message: "Unexpected EOF".into() })
            }
            _ => Err(PdfError::Parse {
                pos: self.lexer.pos(),
                message: format!("Unexpected token: {token:?}").into(),
            }),
        }
    }

    fn parse_array(&mut self) -> PdfResult<Object> {
        let mut elements = Vec::new();
        while self.lexer.peek()? != Token::RightArray && self.lexer.peek()? != Token::EOF {
            elements.push(self.parse_object()?);
        }
        self.lexer.next_token()?; // consume ']'
        let handle = self.arena.alloc_array(elements);
        Ok(Object::Array(handle))
    }

    fn parse_dict(&mut self) -> PdfResult<Object> {
        let mut dict = BTreeMap::new();
        while self.lexer.peek()? != Token::RightDict && self.lexer.peek()? != Token::EOF {
            let key_token = self.lexer.next_token()?;
            let key_handle = match key_token {
                Token::Name(n) => self.arena.intern_name(PdfName::from_bytes(&n)),
                _ => {
                    return Err(PdfError::Parse {
                        pos: self.lexer.pos(),
                        message: format!("Expected name as dictionary key, found {key_token:?}")
                            .into(),
                    });
                }
            };
            let val = self.parse_object()?;
            dict.insert(key_handle, val);
        }
        self.lexer.next_token()?; // consume '>>'

        let handle = self.arena.alloc_dict(dict);
        Ok(Object::Dictionary(handle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_recursion_limit() {
        let arena = PdfArena::new();
        // Construct deeply nested array: [[[[...]]]] over 600 levels
        let mut deeply_nested = Vec::new();
        for _ in 0..600 {
            deeply_nested.extend_from_slice(b"[ ");
        }
        for _ in 0..600 {
            deeply_nested.extend_from_slice(b"] ");
        }

        let mut parser = Parser::new(Bytes::from(deeply_nested), &arena);
        let result = parser.parse_object();
        assert!(result.is_err());
        if let Err(PdfError::Parse { message, .. }) = result {
            assert!(message.contains("Exceeded maximum recursion depth"));
        } else {
            panic!("Expected Parse error with recursion depth limit");
        }
    }
}
