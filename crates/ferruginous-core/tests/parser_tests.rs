//! Integration tests for PDF Lexer & Object Parser

use bytes::Bytes;
use ferruginous_core::parser::Parser;
use ferruginous_core::{Object, PdfArena};

fn parse_bytes(input: &[u8], arena: &PdfArena) -> Object {
    let mut parser = Parser::new(Bytes::copy_from_slice(input), arena);
    parser.parse_object().unwrap()
}

#[test]
fn test_parser_scalar_objects() {
    let arena = PdfArena::new();

    // Parse Integer
    let obj = parse_bytes(b"42", &arena);
    assert_eq!(obj, Object::Integer(42));

    // Parse Real
    let obj = parse_bytes(b"3.6251", &arena);
    assert_eq!(obj, Object::Real(3.6251));

    // Parse Boolean
    let obj = parse_bytes(b"true", &arena);
    assert_eq!(obj, Object::Boolean(true));

    // Parse Null
    let obj = parse_bytes(b"null", &arena);
    assert_eq!(obj, Object::Null);
}

#[test]
fn test_parser_names_and_strings() {
    let arena = PdfArena::new();

    // Parse Name
    let obj = parse_bytes(b"/Helvetica", &arena);
    if let Object::Name(h) = obj {
        assert_eq!(arena.get_name(h).unwrap().as_str(), "Helvetica");
    } else {
        panic!("Expected Name object");
    }

    // Parse Literal String
    let obj = parse_bytes(b"(Hello PDF)", &arena);
    if let Object::String(s) = obj {
        assert_eq!(s.as_ref(), b"Hello PDF");
    } else {
        panic!("Expected String object");
    }

    // Parse Hex String
    let obj = parse_bytes(b"<48656C6C6F>", &arena);
    if let Object::Hex(s) = obj {
        assert_eq!(s.as_ref(), b"Hello");
    } else {
        panic!("Expected Hex object");
    }
}

#[test]
fn test_parser_complex_arrays() {
    let arena = PdfArena::new();
    let input = b"[ 100 /Page false ]";
    let obj = parse_bytes(input, &arena);

    if let Object::Array(ah) = obj {
        let arr = arena.get_array(ah).unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0], Object::Integer(100));
    } else {
        panic!("Expected Array object");
    }
}
