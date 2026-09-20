//! What the lexer writes, the lexer reads.
//!
//! **A literal string is not text.** It carries bytes, and in a CID font those bytes are
//! glyph codes — so a writer that is nearly right produces a page drawing nearly the right
//! letters. `write_literal_string` escaped `(`, `)` and `\` and let a carriage return
//! through raw; 7.3.4.2 says an end-of-line inside a literal string is one line feed, so
//! the code `0x010D` came back as `0x010A`. `Unicode` was written back as `Ukicode`, twice
//! on a page, and only because a test rendered the page and compared every glyph.
//!
//! This asks the question at the level it belongs to: every byte, through the writer and
//! back.

use fepdf_syntax::lexer::{Lexer, Token};

/// The token a stream of `bytes` lexes back to, after being written out.
fn round_trip(token: &Token) -> Token {
    let mut written = Vec::new();
    token.write_to(&mut written);
    Lexer::new(bytes::Bytes::from(written)).next_token().expect("what was written lexes")
}

#[test]
fn a_literal_string_of_every_byte_survives_being_written() {
    let every: Vec<u8> = (0..=255u8).collect();
    let token = Token::String(bytes::Bytes::from(every.clone()));
    match round_trip(&token) {
        Token::String(read) => assert_eq!(
            read.as_ref(),
            every.as_slice(),
            "a byte did not come back as the byte it was written as"
        ),
        other => panic!("a literal string came back as {other:?}"),
    }
}

/// The case that was found in the wild, named so that it stays found: the two-byte code
/// for `n` in one of `unicode_16.pdf`'s fonts.
#[test]
fn a_cid_code_carrying_a_carriage_return_is_the_code_it_was() {
    let token = Token::String(bytes::Bytes::from_static(&[0x01, 0x0D, 0x01, 0x02]));
    match round_trip(&token) {
        Token::String(read) => assert_eq!(
            read.as_ref(),
            [0x01, 0x0D, 0x01, 0x02],
            "the code for `n` came back as a different glyph"
        ),
        other => panic!("a literal string came back as {other:?}"),
    }
}

/// A hex string carries bytes too, and has never had the problem — which is what says the
/// fault was the literal writer's and not the reader's.
#[test]
fn a_hex_string_of_every_byte_survives_too() {
    let every: Vec<u8> = (0..=255u8).collect();
    let token = Token::Hex(bytes::Bytes::from(every.clone()));
    match round_trip(&token) {
        Token::Hex(read) => assert_eq!(read.as_ref(), every.as_slice(), "a byte changed"),
        other => panic!("a hex string came back as {other:?}"),
    }
}
