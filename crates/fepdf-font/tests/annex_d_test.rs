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
    // is what a decision is for. StandardEncoding is not to be predefined (Table D.1).
    assert!(base_encoding("StandardEncoding").is_none());
    assert!(base_encoding("MacExpertEncoding").is_none());
    assert!(is_base_encoding_name("MacExpertEncoding"));
    assert!(is_base_encoding_name("StandardEncoding"));
    assert!(!is_base_encoding_name("Identity-H"));
}

/// **MacRomanEncoding is carried, from D.2's MAC column.** Its codes above ASCII are not
/// WinAnsi's: 0x80 is `Adieresis`, 0xDB `currency` (footnote 1 keeps it there), and 0xCA
/// the second `space` footnote 6 adds.
#[test]
fn mac_roman_is_read_from_its_names() {
    let mac = base_encoding("MacRomanEncoding").expect("carried");
    for (code, expected) in [(0x41_u8, "A"), (0x80, "\u{00C4}"), (0xDB, "\u{00A4}"), (0xCA, " ")] {
        assert_eq!(mac.map(&[code]).as_deref(), Some(expected), "code {code:#04x}");
    }
}

/// **Every name in the MAC column is one Adobe's list reads**, so composing through it
/// drops no code the document assigns.
#[test]
fn every_mac_roman_name_reads_as_text() {
    let mac = base_encoding("MacRomanEncoding").expect("carried");
    for (code, name) in fepdf_font::latin_names::MAC_ROMAN {
        assert!(mac.map(&[code]).is_some(), "{name} at {code:#04x} did not read");
    }
}

/// **The WinAnsi table typed as text agrees with D.2's WIN column read as names.** The
/// text table came first and from CP1252; this holds it to the document, code by code.
#[test]
fn win_ansi_as_typed_is_win_ansi_as_published() {
    for (code, name) in fepdf_font::latin_names::WIN_ANSI {
        assert_eq!(
            says(code),
            fepdf_font::agl::lookup(name),
            "{code:#04x}: the text table and D.2's /{name} disagree"
        );
    }
    let typed = (0..=255_u8).filter(|c| says(*c).is_some()).count();
    assert_eq!(
        typed,
        fepdf_font::latin_names::WIN_ANSI.len(),
        "a code is in one and not the other"
    );
}
