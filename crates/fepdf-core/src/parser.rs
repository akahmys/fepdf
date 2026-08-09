//! ISO 32000-2:2020 Clause 7.3 - Objects

use crate::PdfResult;
use crate::arena::PdfArena;
use crate::error::PdfError;
use crate::handle::Handle;
use crate::lexer::{Lexer, Token};
use crate::object::{Object, PdfName};
use bytes::Bytes;
use std::collections::BTreeMap;

pub struct Parser<'a> {
    lexer: Lexer,
    arena: &'a PdfArena,
}

impl<'a> Parser<'a> {
    pub fn new(data: Bytes, arena: &'a PdfArena) -> Self {
        Self { lexer: Lexer::new(data), arena }
    }

    pub fn peek(&mut self) -> PdfResult<Token> {
        self.lexer.peek()
    }

    pub fn next_token(&mut self) -> PdfResult<Token> {
        self.lexer.next_token()
    }

    /// Parses a single PDF object from the token stream.
    pub fn parse_object(&mut self) -> PdfResult<Object> {
        let token = self.lexer.next_token()?;
        match token {
            Token::Boolean(b) => Ok(Object::Boolean(b)),
            Token::Integer(i) => {
                // Peek to see if it's the start of an indirect reference (R)
                let saved_pos = self.lexer.pos();
                if let Ok(Token::Integer(_gen_num)) = self.lexer.next_token()
                    && let Ok(Token::Keyword(ref k)) = self.lexer.peek()
                    && k == "R"
                {
                    let _ = self.lexer.next_token(); // consume "R"
                    return Ok(Object::Reference(Handle::new(i as u32)));
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
                message: format!("Unexpected token: {:?}", token).into(),
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
                        message: format!("Expected name as dictionary key, found {:?}", key_token)
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
