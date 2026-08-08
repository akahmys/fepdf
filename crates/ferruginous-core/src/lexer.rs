//! ISO 32000-2:2020 Clause 7.2 - Lexical Conventions

use crate::PdfResult;
use bytes::Bytes;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Boolean(bool),
    Integer(i64),
    Real(f64),
    String(Bytes),
    Hex(Bytes),
    Name(Bytes),
    Keyword(String),
    LeftArray,
    RightArray,
    LeftDict,
    RightDict,
    Comment(String),
    Null,
    EOF,
}

impl Token {
    pub fn write_to(&self, output: &mut Vec<u8>) {
        match self {
            Token::Boolean(b) => output.extend_from_slice(if *b { b"true " } else { b"false " }),
            Token::Integer(i) => output.extend_from_slice(format!("{} ", i).as_bytes()),
            Token::Real(f) => output.extend_from_slice(format!("{:.4} ", f).as_bytes()),
            Token::String(s) => self.write_literal_string(s, output),
            Token::Hex(s) => self.write_hex_string(s, output),
            Token::Name(n) => self.write_name(n, output),
            Token::Keyword(kw) => {
                output.extend_from_slice(kw.as_bytes());
                output.push(b' ');
            }
            Token::LeftArray => output.extend_from_slice(b"[ "),
            Token::RightArray => output.extend_from_slice(b"] "),
            Token::LeftDict => output.extend_from_slice(b"<< "),
            Token::RightDict => output.extend_from_slice(b">> "),
            Token::Comment(c) => {
                output.push(b'%');
                output.extend_from_slice(c.as_bytes());
                output.push(b'\n');
            }
            Token::Null => output.extend_from_slice(b"null "),
            Token::EOF => {}
        }
    }

    fn write_literal_string(&self, s: &[u8], output: &mut Vec<u8>) {
        output.push(b'(');
        for &b in s {
            if b == b'(' || b == b')' || b == b'\\' {
                output.push(b'\\');
            }
            output.push(b);
        }
        output.push(b')');
        output.push(b' ');
    }

    fn write_hex_string(&self, s: &[u8], output: &mut Vec<u8>) {
        output.push(b'<');
        for &b in s {
            output.extend_from_slice(format!("{:02X}", b).as_bytes());
        }
        output.push(b'>');
        output.push(b' ');
    }

    fn write_name(&self, n: &[u8], output: &mut Vec<u8>) {
        output.push(b'/');
        for &b in n {
            if b == b'#' || b <= 32 || b >= 127 || is_delimiter(b) {
                output.extend_from_slice(format!("#{b:02X}").as_bytes());
            } else {
                output.push(b);
            }
        }
        output.push(b' ');
    }
}

/// Convenience function to tokenize a buffer.
pub fn tokenize(data: &[u8]) -> Vec<Token> {
    let mut lexer = Lexer::new(Bytes::copy_from_slice(data));
    let mut tokens = Vec::new();
    while let Ok(token) = lexer.next_token() {
        if token == Token::EOF {
            break;
        }
        tokens.push(token);
    }
    tokens
}

pub struct Lexer {
    data: Bytes,
    pos: usize,
}

impl Lexer {
    pub fn new(data: Bytes) -> Self {
        Self { data, pos: 0 }
    }

    pub fn get_data(&self) -> &Bytes {
        &self.data
    }

    pub fn next_token(&mut self) -> PdfResult<Token> {
        self.skip_whitespace_and_comments();
        if self.pos >= self.data.len() {
            return Ok(Token::EOF);
        }

        let b = self.data[self.pos];
        match b {
            b'/' => self.lex_name(),
            b'(' => self.lex_literal_string(),
            b'<' => {
                if self.pos + 1 < self.data.len() && self.data[self.pos + 1] == b'<' {
                    self.pos += 2;
                    Ok(Token::LeftDict)
                } else {
                    self.lex_hex_string()
                }
            }
            b'>' => {
                if self.pos + 1 < self.data.len() && self.data[self.pos + 1] == b'>' {
                    self.pos += 2;
                    Ok(Token::RightDict)
                } else {
                    self.pos += 1;
                    Ok(Token::Keyword(">".to_string()))
                }
            }
            b'[' => {
                self.pos += 1;
                Ok(Token::LeftArray)
            }
            b']' => {
                self.pos += 1;
                Ok(Token::RightArray)
            }
            b'{' => {
                self.pos += 1;
                Ok(Token::Keyword("{".to_string()))
            }
            b'}' => {
                self.pos += 1;
                Ok(Token::Keyword("}".to_string()))
            }
            b'0'..=b'9' | b'+' | b'-' | b'.' => self.lex_number_or_keyword(),
            _ => self.lex_keyword_or_other(),
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            if is_whitespace(b) {
                self.pos += 1;
            } else if b == b'%' {
                self.pos += 1;
                while self.pos < self.data.len() && !is_newline(self.data[self.pos]) {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn lex_name(&mut self) -> PdfResult<Token> {
        self.pos += 1; // skip '/'
        let mut result = Vec::new();
        while self.pos < self.data.len()
            && !is_delimiter(self.data[self.pos])
            && !is_whitespace(self.data[self.pos])
        {
            let b = self.data[self.pos];
            if b == b'#' && self.pos + 2 < self.data.len() {
                let hex = &self.data[self.pos + 1..self.pos + 3];
                if let Ok(utf8_str) = std::str::from_utf8(hex)
                    && let Ok(val) = u8::from_str_radix(utf8_str, 16)
                {
                    result.push(val);
                    self.pos += 3;
                    continue;
                }
            }
            result.push(b);
            self.pos += 1;
        }
        Ok(Token::Name(Bytes::from(result)))
    }

    fn lex_literal_string(&mut self) -> PdfResult<Token> {
        self.pos += 1; // skip '('
        let mut balance = 1;
        let mut result = Vec::new();
        while self.pos < self.data.len() && balance > 0 {
            let b = self.data[self.pos];
            match b {
                b'(' => {
                    balance += 1;
                    result.push(b);
                }
                b')' => {
                    balance -= 1;
                    if balance > 0 {
                        result.push(b);
                    }
                }
                b'\\' => {
                    self.pos += 1;
                    if self.pos < self.data.len() {
                        let b2 = self.data[self.pos];
                        match b2 {
                            b'n' => result.push(b'\n'),
                            b'r' => result.push(b'\r'),
                            b't' => result.push(b'\t'),
                            b'b' => result.push(8),
                            b'f' => result.push(12),
                            b'(' => result.push(b'('),
                            b')' => result.push(b')'),
                            b'\\' => result.push(b'\\'),
                            b'0'..=b'7' => {
                                let (val, new_pos) = self.lex_octal(b2, self.pos);
                                result.push(val);
                                self.pos = new_pos;
                            }
                            _ => result.push(b2),
                        }
                    }
                }
                _ => result.push(b),
            }
            self.pos += 1;
        }
        Ok(Token::String(Bytes::from(result)))
    }

    fn lex_octal(&self, first_digit: u8, start_pos: usize) -> (u8, usize) {
        let mut octal = (first_digit - b'0') as u32;
        let mut pos = start_pos;
        let mut count = 1;
        while count < 3 && pos + 1 < self.data.len() {
            let next_b = self.data[pos + 1];
            if (b'0'..=b'7').contains(&next_b) {
                octal = (octal << 3) | (next_b - b'0') as u32;
                pos += 1;
                count += 1;
            } else {
                break;
            }
        }
        (octal as u8, pos)
    }

    fn lex_hex_string(&mut self) -> PdfResult<Token> {
        self.pos += 1; // skip '<'
        let mut result = Vec::new();
        let mut high_nibble: Option<u8> = None;
        while self.pos < self.data.len() {
            let b = self.data[self.pos];
            if b == b'>' {
                self.pos += 1;
                break;
            }
            if let Some(val) = (b as char).to_digit(16) {
                if let Some(high) = high_nibble {
                    result.push((high << 4) | val as u8);
                    high_nibble = None;
                } else {
                    high_nibble = Some(val as u8);
                }
            }
            self.pos += 1;
        }
        if let Some(high) = high_nibble {
            result.push(high << 4);
        }
        Ok(Token::Hex(Bytes::from(result)))
    }

    fn lex_number_or_keyword(&mut self) -> PdfResult<Token> {
        let start = self.pos;
        let mut is_real = false;
        while self.pos < self.data.len()
            && !is_delimiter(self.data[self.pos])
            && !is_whitespace(self.data[self.pos])
        {
            if self.data[self.pos] == b'.' {
                is_real = true;
            }
            self.pos += 1;
        }
        let s = String::from_utf8_lossy(&self.data[start..self.pos]);
        if is_real {
            if let Ok(f) = s.parse::<f64>() {
                return Ok(Token::Real(f));
            }
        } else if let Ok(i) = s.parse::<i64>() {
            return Ok(Token::Integer(i));
        }
        Ok(Token::Keyword(s.to_string()))
    }

    fn lex_keyword_or_other(&mut self) -> PdfResult<Token> {
        let start = self.pos;
        if self.pos < self.data.len() {
            self.pos += 1;
        }
        while self.pos < self.data.len()
            && !is_delimiter(self.data[self.pos])
            && !is_whitespace(self.data[self.pos])
        {
            self.pos += 1;
        }
        let s = String::from_utf8_lossy(&self.data[start..self.pos]).to_string();
        match s.as_str() {
            "true" => Ok(Token::Boolean(true)),
            "false" => Ok(Token::Boolean(false)),
            "null" => Ok(Token::Null),
            _ => Ok(Token::Keyword(s)),
        }
    }

    pub fn peek(&mut self) -> PdfResult<Token> {
        let prev_pos = self.pos;
        let token = self.next_token();
        self.pos = prev_pos;
        token
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }
}

fn is_whitespace(b: u8) -> bool {
    matches!(b, 0 | 9 | 10 | 12 | 13 | 32)
}

fn is_newline(b: u8) -> bool {
    matches!(b, 10 | 13)
}

pub fn is_delimiter(b: u8) -> bool {
    matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_basic_primitives() {
        let input = b"true false null 123 -456 3.1415 /Type /Page";
        let tokens = tokenize(input);
        assert_eq!(
            tokens,
            vec![
                Token::Boolean(true),
                Token::Boolean(false),
                Token::Null,
                Token::Integer(123),
                Token::Integer(-456),
                Token::Real(3.1415),
                Token::Name(Bytes::from_static(b"Type")),
                Token::Name(Bytes::from_static(b"Page")),
            ]
        );
    }

    #[test]
    fn test_tokenize_arrays_and_dicts() {
        let input = b"<< /Count 1 /Kids [ 3 0 R ] >>";
        let tokens = tokenize(input);
        assert_eq!(
            tokens,
            vec![
                Token::LeftDict,
                Token::Name(Bytes::from_static(b"Count")),
                Token::Integer(1),
                Token::Name(Bytes::from_static(b"Kids")),
                Token::LeftArray,
                Token::Integer(3),
                Token::Integer(0),
                Token::Keyword("R".to_string()),
                Token::RightArray,
                Token::RightDict,
            ]
        );
    }

    #[test]
    fn test_tokenize_hex_and_name_escapes() {
        let input = b"<48656c6c6f> /Foo#20Bar";
        let tokens = tokenize(input);
        assert_eq!(
            tokens,
            vec![
                Token::Hex(Bytes::from_static(b"Hello")),
                Token::Name(Bytes::from_static(b"Foo Bar")),
            ]
        );
    }
}
