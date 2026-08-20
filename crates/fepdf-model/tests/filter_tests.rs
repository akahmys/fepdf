//! Clause 7.4 filters, against vectors from the standard itself where it gives them.
//!
//! These filters were absent until a corpus this project did not choose was measured:
//! all nine files in `samples/` are `FlateDecode` and `DCTDecode`, so nothing had ever
//! asked for the rest.

use fepdf_model::filters::decode_stream;
use fepdf_model::{Object, PdfArena};

fn decode(filter: &str, input: &[u8]) -> Vec<u8> {
    let arena = PdfArena::new();
    decode_stream(filter, input, None, &arena)
        .unwrap_or_else(|e| panic!("{filter} refused its own test vector: {e:?}"))
        .to_vec()
}

fn refuses(filter: &str, input: &[u8]) -> bool {
    let arena = PdfArena::new();
    decode_stream(filter, input, None, &arena).is_err()
}

/// The worked example in 7.4.4.2: the encoded form of `45 45 45 45 45 65 45 45 45 66`.
///
/// A decoder that ignores `/EarlyChange` does not fail on this — it produces plausible
/// rubbish, which is why the vector matters more here than anywhere else in the clause.
#[test]
fn lzw_decodes_the_example_from_clause_7_4_4_2() {
    let encoded = [0x80, 0x0B, 0x60, 0x50, 0x22, 0x0C, 0x0C, 0x85, 0x01];
    assert_eq!(decode("LZWDecode", &encoded), vec![45, 45, 45, 45, 45, 65, 45, 45, 45, 66]);
    // Table 6's abbreviation reaches the same decoder. `/LZW` occurs in the external
    // corpus, and only `Fl` and `DCT` were matched before this.
    assert_eq!(decode("LZW", &encoded), decode("LZWDecode", &encoded));
}

/// 7.4.2: pairs of hex digits, whitespace ignored, `>` ends the data, and an odd final
/// digit is completed with a zero rather than dropped.
#[test]
fn ascii_hex_reads_pairs_and_completes_an_odd_final_digit() {
    assert_eq!(decode("ASCIIHexDecode", b"48656C6C6F>"), b"Hello".to_vec());
    assert_eq!(decode("ASCIIHexDecode", b"48 65\n6C\t6C 6F >"), b"Hello".to_vec());
    assert_eq!(decode("ASCIIHexDecode", b"48656C6C6F"), b"Hello".to_vec(), "EOD may be absent");

    // `4` alone is `0x40`, not nothing: half a byte of real data.
    assert_eq!(decode("ASCIIHexDecode", b"414>"), vec![0x41, 0x40]);
    // Anything after `>` belongs to no stream.
    assert_eq!(decode("ASCIIHexDecode", b"41>4242"), vec![0x41]);
    assert_eq!(decode("AHx", b"41>"), vec![0x41], "the abbreviation, 7 uses in one file");

    assert!(refuses("ASCIIHexDecode", b"41ZZ>"), "not a hexadecimal digit");
}

/// 7.4.3, checked against an implementation with no relationship to this one.
///
/// Both vectors in the first version of this test were wrong and the decoder was right
/// twice: `87cURD]i,"Ebo80` is `Hello World!`, not `Hello, World`. Hand-written
/// expectations are the checker's own opinion, so the table below is generated from
/// Python's `base64.a85encode` and pasted, and the special cases are kept separately.
#[test]
fn ascii85_agrees_with_an_unrelated_implementation() {
    let vectors: &[(&[u8], &[u8])] = &[
        (b"~>".as_slice(), b"".as_slice()),
        (b"@/~>".as_slice(), b"a".as_slice()),
        (b"@:B~>".as_slice(), b"ab".as_slice()),
        (b"@:E^~>".as_slice(), b"abc".as_slice()),
        (b"@:E_W~>".as_slice(), b"abcd".as_slice()),
        (b"@:E_WAH~>".as_slice(), b"abcde".as_slice()),
        (b"z~>".as_slice(), b"\x00\x00\x00\x00".as_slice()),
        (b"z@:E^~>".as_slice(), b"\x00\x00\x00\x00abc".as_slice()),
        (
            b"!!*-'\x229eu7#RLhG$k3[W~>".as_slice(),
            b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f".as_slice(),
        ),
        (b"FD,5.EHPu*CER),Dg-(AAoDn~>".as_slice(), b"the quick brown fox".as_slice()),
        (b"87cURD]i,\x22Ebo80~>".as_slice(), b"Hello World!".as_slice()),
    ];
    for (encoded, expected) in vectors {
        assert_eq!(decode("ASCII85Decode", encoded), expected.to_vec(), "{encoded:?}");
    }
}

/// The parts 7.4.3 defines that a generic base-85 encoder never emits.
///
/// Three assertions in this file were wrong before the decoder was, each time because a
/// hand-written expectation is the checker's own opinion of the format. Where an
/// independent implementation could supply the answer it now does; where it cannot —
/// `<~`, the abbreviations, what counts as malformed — the reasoning is written down.
#[test]
fn ascii85_reads_the_things_only_pdf_writes() {
    // Whitespace anywhere, and a `<~` prefix that 7.4.3 does not define but files carry.
    assert_eq!(decode("ASCII85Decode", b"<~87cU\nRD]i,\x22Ebo80~>"), b"Hello World!".to_vec());
    assert_eq!(decode("A85", b"z~>"), vec![0, 0, 0, 0], "the abbreviation");
    // Anything past `~` belongs to no stream.
    assert_eq!(decode("ASCII85Decode", b"@/~>@/"), b"a".to_vec());

    // A final group of *one* character encodes no byte at all — a malformed stream
    // rather than an empty one. `@:E_WA` is a full group of five plus a single `A`.
    assert!(refuses("ASCII85Decode", b"@:E_WA~>"));
    assert!(refuses("ASCII85Decode", b"abc\x7f~>"), "outside the base-85 alphabet");
    // `z` stands for a whole group and is only legal between groups; part-way through
    // one it is just a character outside the alphabet, which is what it must be read as.
    assert!(refuses("ASCII85Decode", b"@z~>"));
    // And a two-character final group is *legal* — it encodes one byte. An earlier
    // version of this test asserted the opposite, which the decoder was right to fail.
    assert_eq!(decode("ASCII85Decode", b"ab~>"), vec![0xC9]);
}

/// 7.4.5: a length byte, then either that many literal bytes or one byte repeated.
#[test]
fn run_length_reads_literal_runs_and_repeats() {
    // 2 -> three literal bytes; 254 -> the next byte three times; 128 ends the data.
    assert_eq!(
        decode("RunLengthDecode", &[2, b'a', b'b', b'c', 254, b'z', 128]),
        b"abczzz".to_vec()
    );
    assert_eq!(decode("RL", &[0, b'x', 128]), b"x".to_vec(), "length 0 is one byte, not none");
    assert_eq!(decode("RunLengthDecode", &[255, b'q', 128]), b"qq".to_vec(), "255 repeats twice");

    // Anything past the end-of-data byte is not part of the stream.
    assert_eq!(decode("RunLengthDecode", &[0, b'x', 128, 0, b'y']), b"x".to_vec());
    // Truncated mid-run: return what arrived rather than lose the whole stream.
    assert_eq!(decode("RunLengthDecode", &[5, b'a', b'b']), b"ab".to_vec());
}

/// A filter this engine does not implement must say so by name, not be mistaken for one
/// it does.
///
/// `CCITTFaxDecode` and `CCF` stood in this list until Phase M built them — which is the
/// check doing its job, since a filter that gains a decoder must stop being an example
/// of one that has none.
#[test]
fn an_unimplemented_filter_is_named_rather_than_guessed() {
    let arena = PdfArena::new();
    for name in ["JBIG2Decode", "JPXDecode", "NoSuchDecode"] {
        let err = decode_stream(name, b"anything", None, &arena).expect_err(name);
        assert!(format!("{err:?}").contains(name), "{name} was not named in {err:?}");
    }
}

/// Table 6's abbreviation reaches the same decoder as the long name, and not a
/// neighbouring one: `CCF` must not land in `DCT`.
#[test]
fn the_ccitt_abbreviation_reaches_the_ccitt_decoder() {
    let arena = PdfArena::new();
    for name in ["CCITTFaxDecode", "CCF"] {
        let err = decode_stream(name, b"anything", None, &arena).expect_err(name);
        let message = format!("{err:?}");
        assert!(message.contains("CCITTFaxDecode"), "{name} landed elsewhere: {message}");
        assert!(!message.contains("Unsupported filter"), "{name} is implemented: {message}");
    }
}

/// The predictor table applies to LZW as it does to Flate (Table 8), so the parameter has
/// to reach it. A `/Predictor 1` means none and must leave the bytes alone.
#[test]
fn lzw_passes_its_decode_parms_through() {
    let arena = PdfArena::new();
    let mut parms = std::collections::BTreeMap::new();
    parms.insert(arena.name("Predictor"), Object::Integer(1));
    parms.insert(arena.name("EarlyChange"), Object::Integer(1));
    let parms = Object::Dictionary(arena.alloc_dict(parms));

    let encoded = [0x80, 0x0B, 0x60, 0x50, 0x22, 0x0C, 0x0C, 0x85, 0x01];
    let out = decode_stream("LZWDecode", &encoded, Some(&parms), &arena).expect("decoded");
    assert_eq!(out.to_vec(), vec![45, 45, 45, 45, 45, 65, 45, 45, 45, 66]);
}
