//! Building objects out of the byte offsets `fepdf-syntax` located.
//!
//! `fepdf_syntax::xref` says *where* things are; this says *what* they are. The split
//! is the arena: constructing an [`Object`] needs one, finding its offset does not.
//!
//! Every tolerance here is recorded as a [`Decision`], because the choices this module
//! makes are the substance of reading a pre-2.0 file — see `ARCHITECTURE.md` §5.3.

use crate::arena::PdfArena;
use crate::error::{PdfError, PdfResult};
use crate::interpretation::{Decision, DecisionLog};
use crate::object::{Object, SublimatedData};
use crate::parser::Parser;
use bytes::Bytes;
use fepdf_syntax::lexer::{Lexer, Token};

/// One indirect object as written in the file.
#[derive(Debug, Clone)]
pub struct IndirectObject {
    /// Object number.
    pub number: u32,
    /// Generation number.
    pub generation: u16,
    /// The object itself.
    pub object: Object,
}

/// Reads the `N G obj … endobj` at `offset`.
///
/// Streams are recognised here rather than in the object parser because delimiting
/// one needs `/Length`, which is a document-level question: the value may be an
/// indirect reference, and it is frequently wrong.
pub fn parse_indirect_at(
    bytes: &[u8],
    offset: usize,
    arena: &PdfArena,
    decisions: &mut DecisionLog,
) -> PdfResult<IndirectObject> {
    if offset >= bytes.len() {
        return Err(PdfError::Parse {
            pos: offset,
            message: "object offset lies past the end of the file".into(),
        });
    }

    let mut lexer = Lexer::new(Bytes::copy_from_slice(&bytes[offset..]));
    let number = expect_unsigned(&mut lexer, offset)?;
    let generation = expect_unsigned(&mut lexer, offset)?;
    match lexer.next_token()? {
        Token::Keyword(k) if k == "obj" => {}
        other => {
            return Err(PdfError::Parse {
                pos: offset,
                message: format!("expected `obj`, found {other:?}").into(),
            });
        }
    }

    let body_at = offset + lexer.pos();
    let mut parser = Parser::new(Bytes::copy_from_slice(&bytes[body_at..]), arena);
    let object = parser.parse_object()?;
    let after_body = body_at + parser.position();

    let object = match stream_start(bytes, after_body) {
        Some(data_at) => attach_stream(bytes, data_at, object, arena, decisions)?,
        None => object,
    };

    Ok(IndirectObject {
        number: u32::try_from(number).unwrap_or(0),
        generation: u16::try_from(generation).unwrap_or(0),
        object,
    })
}

/// Where a stream's data begins, if the object is followed by `stream`.
///
/// 7.3.8.1 requires CRLF or LF after the keyword, never CR alone; a lone CR is
/// accepted because producers emit it and the intent is unambiguous.
fn stream_start(bytes: &[u8], after_body: usize) -> Option<usize> {
    let mut i = after_body;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if !bytes[i..].starts_with(b"stream") {
        return None;
    }
    i += 6;
    if bytes[i..].starts_with(b"\r\n") {
        i += 2;
    } else if bytes.get(i) == Some(&b'\n') || bytes.get(i) == Some(&b'\r') {
        i += 1;
    }
    Some(i)
}

/// Turns a stream's dictionary into a stream object, delimiting its data.
fn attach_stream(
    bytes: &[u8],
    data_at: usize,
    dict_object: Object,
    arena: &PdfArena,
    decisions: &mut DecisionLog,
) -> PdfResult<Object> {
    let Object::Dictionary(dict_h) = dict_object else {
        return Err(PdfError::Parse {
            pos: data_at,
            message: "`stream` follows an object that is not a dictionary".into(),
        });
    };

    let declared = arena
        .get_dict(dict_h)
        .and_then(|d| d.get(&arena.name("Length")).cloned())
        .and_then(|o| match o {
            Object::Integer(n) => usize::try_from(n).ok(),
            // An indirect /Length cannot be resolved until every object is loaded,
            // so the data is delimited by scanning instead.
            _ => None,
        });

    let end = resolve_stream_extent(bytes, data_at, declared, decisions);

    let data = Bytes::copy_from_slice(&bytes[data_at..end.min(bytes.len())]);
    Ok(Object::Stream(dict_h, std::sync::Arc::new(SublimatedData::Raw(data))))
}

/// Decides where a stream's data ends, recording why when the file is not clear.
///
/// `/Length` is authoritative when it agrees with the `endstream` keyword. When they
/// disagree the scanned extent wins: a wrong length truncates or overruns real data,
/// whereas the keyword is where the producer actually stopped writing.
fn resolve_stream_extent(
    bytes: &[u8],
    data_at: usize,
    declared: Option<usize>,
    decisions: &mut DecisionLog,
) -> usize {
    let scanned = find_endstream(bytes, data_at);
    match (declared, scanned) {
        (Some(len), Some(found)) if data_at + len == found => found,
        (Some(len), Some(found)) => {
            decisions.push(Decision::repaired(
                "7.3.8.2",
                format!("/Length {len} but endstream is {} bytes in", found - data_at),
                "used the scanned extent, since /Length would truncate or overrun the data",
            ));
            found
        }
        (Some(len), None) if data_at + len <= bytes.len() => {
            decisions.push(Decision::repaired(
                "7.3.8.2",
                "no endstream keyword after the stream data",
                "trusted /Length",
            ));
            data_at + len
        }
        (None, Some(found)) => {
            decisions.push(Decision::ambiguity(
                "7.3.8.2",
                "/Length absent or an indirect reference",
                "delimited the data by scanning to endstream",
            ));
            found
        }
        _ => {
            decisions.push(Decision::violation(
                "7.3.8.2",
                "stream has neither a usable /Length nor an endstream keyword",
                "treated the remainder of the file as the stream data",
            ));
            bytes.len()
        }
    }
}

/// Offset of the `endstream` keyword after `from`, excluding the whitespace before it.
fn find_endstream(bytes: &[u8], from: usize) -> Option<usize> {
    let at = from + bytes[from..].windows(9).position(|w| w == b"endstream")?;
    // The EOL introduced before `endstream` is not part of the data (7.3.8.1).
    let mut end = at;
    if end > from && bytes[end - 1] == b'\n' {
        end -= 1;
    }
    if end > from && bytes[end - 1] == b'\r' {
        end -= 1;
    }
    Some(end)
}

/// Reads the next token as an unsigned integer.
fn expect_unsigned(lexer: &mut Lexer, at: usize) -> PdfResult<i64> {
    match lexer.next_token()? {
        Token::Integer(n) if n >= 0 => Ok(n),
        other => Err(PdfError::Parse {
            pos: at,
            message: format!("expected an object number, found {other:?}").into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(src: &[u8]) -> (IndirectObject, DecisionLog) {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        let obj = parse_indirect_at(src, 0, &arena, &mut log).expect("should parse");
        (obj, log)
    }

    #[test]
    fn a_plain_object_carries_its_number_and_generation() {
        let (obj, log) = read(b"12 3 obj\n42\nendobj\n");
        assert_eq!(obj.number, 12);
        assert_eq!(obj.generation, 3);
        assert_eq!(obj.object, Object::Integer(42));
        assert!(log.is_conforming(), "a well-formed object needs no decisions");
    }

    #[test]
    fn a_stream_with_a_correct_length_is_read_without_comment() {
        let (obj, log) = read(b"1 0 obj\n<< /Length 5 >>\nstream\nHELLO\nendstream\nendobj\n");
        let Object::Stream(_, data) = obj.object else { panic!("expected a stream") };
        let SublimatedData::Raw(bytes) = data.as_ref() else { panic!("expected raw data") };
        assert_eq!(&bytes[..], b"HELLO");
        assert!(log.is_conforming());
    }

    #[test]
    fn a_wrong_length_is_repaired_and_recorded() {
        // The most common real-world non-conformance: /Length disagrees with the data.
        let (obj, log) = read(b"1 0 obj\n<< /Length 2 >>\nstream\nHELLO\nendstream\nendobj\n");
        let Object::Stream(_, data) = obj.object else { panic!("expected a stream") };
        let SublimatedData::Raw(bytes) = data.as_ref() else { panic!("expected raw data") };
        assert_eq!(&bytes[..], b"HELLO", "the scanned extent wins over a wrong /Length");
        assert_eq!(log.entries().len(), 1);
        assert!(log.entries()[0].found.contains("/Length 2"));
    }

    #[test]
    fn an_indirect_length_is_resolved_by_scanning() {
        // /Length may reference an object that is not loaded yet, so the data is
        // delimited by scanning; that is a reading chosen, not an error.
        let (obj, log) = read(b"1 0 obj\n<< /Length 9 0 R >>\nstream\nHELLO\nendstream\nendobj\n");
        let Object::Stream(_, data) = obj.object else { panic!("expected a stream") };
        let SublimatedData::Raw(bytes) = data.as_ref() else { panic!("expected raw data") };
        assert_eq!(&bytes[..], b"HELLO");
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].severity, crate::interpretation::Severity::Ambiguity);
    }

    #[test]
    fn a_lone_carriage_return_after_stream_is_accepted() {
        // 7.3.8.1 requires CRLF or LF; producers emit a bare CR and the intent is
        // unambiguous, so it is accepted.
        let (obj, _) = read(b"1 0 obj\n<< /Length 5 >>\nstream\rHELLO\nendstream\nendobj\n");
        let Object::Stream(_, data) = obj.object else { panic!("expected a stream") };
        let SublimatedData::Raw(bytes) = data.as_ref() else { panic!("expected raw data") };
        assert_eq!(&bytes[..], b"HELLO");
    }

    #[test]
    fn a_stream_with_no_endstream_and_no_length_is_a_violation() {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        let src = b"1 0 obj\n<< /Foo 1 >>\nstream\nHELLO";
        let obj = parse_indirect_at(src, 0, &arena, &mut log).expect("should still parse");
        let Object::Stream(..) = obj.object else { panic!("expected a stream") };
        assert_eq!(log.entries()[0].severity, crate::interpretation::Severity::Violation);
    }

    #[test]
    fn an_offset_past_the_end_is_refused() {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        assert!(parse_indirect_at(b"1 0 obj\n1\nendobj", 900, &arena, &mut log).is_err());
    }

    #[test]
    fn a_non_object_at_the_offset_is_refused() {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        assert!(parse_indirect_at(b"trailer\n<<>>", 0, &arena, &mut log).is_err());
    }
}
