//! The base encodings of Annex D, and the one character that cannot be spelled by name.
//!
//! `/WinAnsiEncoding` reached `CMap::load_named`, which searches Adobe's CMap Resources —
//! CJK character collections, which have never held an Annex D table. The lookup failed
//! and the font was left with no encoding at all: measured, 36,914 glyphs of
//! `intel_sdm.pdf`, whose 1,600 font references all declare it.

use fepdf_font::annex_d::{base_encoding, is_base_encoding_name};

/// What the encoding says a code is, as text.
fn says(code: u8) -> Option<String> {
    base_encoding("WinAnsiEncoding").expect("carried").map(&[code])
}

#[test]
fn the_codes_that_were_lost_are_the_ones_above_ascii() {
    // The seven that account for 36,700 of the 36,914, in order of how many were lost.
    for (code, expected) in [
        (0x97, "\u{2014}"), // — em dash, 8,563
        (0x95, "\u{2022}"), // • bullet, 6,558
        (0x93, "\u{201C}"), // " left double quote, 6,151
        (0x94, "\u{201D}"), // " right double quote, 6,144
        (0xAE, "\u{00AE}"), // ® registered, 4,940
        (0x92, "\u{2019}"), // ' right single quote, 2,226
        (0x96, "\u{2013}"), // – en dash, 1,061
    ] {
        assert_eq!(says(code).as_deref(), Some(expected), "code {code:#04x}");
    }
}

#[test]
fn the_two_substitutions_the_specification_names_are_made() {
    // D.2 note: `0xA0` is a space, not a no-break space, and `0xAD` is a hyphen, not a
    // soft one. CP1252 says otherwise for both, and the table is not CP1252.
    assert_eq!(says(0xA0).as_deref(), Some(" "));
    assert_eq!(says(0xAD).as_deref(), Some("-"));
}

#[test]
fn a_code_the_encoding_leaves_undefined_is_absent_rather_than_guessed() {
    for code in [0x81_u8, 0x8D, 0x8F, 0x90, 0x9D] {
        assert_eq!(says(code), None, "code {code:#04x} is undefined in CP1252 and here");
    }
}

#[test]
fn ascii_is_carried_too_because_a_font_may_have_no_other_route() {
    assert_eq!(says(0x41).as_deref(), Some("A"));
    assert_eq!(says(0x20).as_deref(), Some(" "));
    // The one that cost 41,058 glyphs when the table first went in: a mapping value
    // beginning with a slash used to mean "this is a glyph name", and `/` is a character.
    assert_eq!(says(0x2F).as_deref(), Some("/"));
}

#[test]
fn a_name_this_engine_does_not_carry_is_still_known_to_be_an_encoding() {
    // The difference between a gap and a silence: these have no table here, and saying so
    // is what a decision is for.
    assert!(base_encoding("MacRomanEncoding").is_none());
    assert!(is_base_encoding_name("MacRomanEncoding"));
    assert!(is_base_encoding_name("StandardEncoding"));
    assert!(!is_base_encoding_name("Identity-H"));
}
