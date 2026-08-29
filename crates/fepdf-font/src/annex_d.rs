//! The simple-font base encodings of Annex D, which are not CMaps and were reaching the
//! CMap loader.
//!
//! A simple font's `/Encoding` may name one of these tables (9.6.6.1), and a font
//! dictionary's `/BaseEncoding` may name one under a `/Differences` array. Both arrived
//! at [`crate::cmap::CMap::load_named`], which searches Adobe's *CMap Resources* — a
//! collection of CJK character collections that has never contained an Annex D table and
//! never will. The lookup failed, the font was left with no encoding at all, and every
//! code the ASCII guess could not reach came back unnamed.
//!
//! **Measured, that was 36,914 glyphs of `intel_sdm.pdf`**, whose 1,600 font references
//! all declare `/WinAnsiEncoding`. Every code that failed is one of the 128 above
//! `U+007E`, and the top of the list is ordinary punctuation:
//!
//! | code | character | lost |
//! |---|---|---|
//! | `0x97` | `—` | 8,563 |
//! | `0x95` | `•` | 6,558 |
//! | `0x93` `0x94` | `“` `”` | 12,295 |
//! | `0xAE` | `®` | 4,940 |
//!
//! The table is code to text rather than code to glyph name, though Annex D is written in
//! glyph names. The name is an intermediate here — `finish_unicode` accepts either, and
//! resolving a name goes through an Adobe Glyph List that carries 61 entries and answers
//! the empty string for everything else. Composing the two at table-generation time is
//! the same answer without that step.

use crate::cmap::CMap;
use std::collections::BTreeMap;
use std::sync::Arc;

/// `WinAnsiEncoding` (D.2), which is Windows code page 1252 with two substitutions the
/// specification names: `0xA0` is a space rather than a no-break space, and `0xAD` is a
/// hyphen rather than a soft one. Codes CP1252 leaves undefined — `0x81`, `0x8D`, `0x8F`,
/// `0x90`, `0x9D` — are absent here too, so a document using one is reported rather than
/// answered.
static WIN_ANSI: &[(u8, &str)] = &[
    (0x20, "\u{0020}"),
    (0x21, "\u{0021}"),
    (0x22, "\u{0022}"),
    (0x23, "\u{0023}"),
    (0x24, "\u{0024}"),
    (0x25, "\u{0025}"),
    (0x26, "\u{0026}"),
    (0x27, "\u{0027}"),
    (0x28, "\u{0028}"),
    (0x29, "\u{0029}"),
    (0x2a, "\u{002A}"),
    (0x2b, "\u{002B}"),
    (0x2c, "\u{002C}"),
    (0x2d, "\u{002D}"),
    (0x2e, "\u{002E}"),
    (0x2f, "\u{002F}"),
    (0x30, "\u{0030}"),
    (0x31, "\u{0031}"),
    (0x32, "\u{0032}"),
    (0x33, "\u{0033}"),
    (0x34, "\u{0034}"),
    (0x35, "\u{0035}"),
    (0x36, "\u{0036}"),
    (0x37, "\u{0037}"),
    (0x38, "\u{0038}"),
    (0x39, "\u{0039}"),
    (0x3a, "\u{003A}"),
    (0x3b, "\u{003B}"),
    (0x3c, "\u{003C}"),
    (0x3d, "\u{003D}"),
    (0x3e, "\u{003E}"),
    (0x3f, "\u{003F}"),
    (0x40, "\u{0040}"),
    (0x41, "\u{0041}"),
    (0x42, "\u{0042}"),
    (0x43, "\u{0043}"),
    (0x44, "\u{0044}"),
    (0x45, "\u{0045}"),
    (0x46, "\u{0046}"),
    (0x47, "\u{0047}"),
    (0x48, "\u{0048}"),
    (0x49, "\u{0049}"),
    (0x4a, "\u{004A}"),
    (0x4b, "\u{004B}"),
    (0x4c, "\u{004C}"),
    (0x4d, "\u{004D}"),
    (0x4e, "\u{004E}"),
    (0x4f, "\u{004F}"),
    (0x50, "\u{0050}"),
    (0x51, "\u{0051}"),
    (0x52, "\u{0052}"),
    (0x53, "\u{0053}"),
    (0x54, "\u{0054}"),
    (0x55, "\u{0055}"),
    (0x56, "\u{0056}"),
    (0x57, "\u{0057}"),
    (0x58, "\u{0058}"),
    (0x59, "\u{0059}"),
    (0x5a, "\u{005A}"),
    (0x5b, "\u{005B}"),
    (0x5c, "\u{005C}"),
    (0x5d, "\u{005D}"),
    (0x5e, "\u{005E}"),
    (0x5f, "\u{005F}"),
    (0x60, "\u{0060}"),
    (0x61, "\u{0061}"),
    (0x62, "\u{0062}"),
    (0x63, "\u{0063}"),
    (0x64, "\u{0064}"),
    (0x65, "\u{0065}"),
    (0x66, "\u{0066}"),
    (0x67, "\u{0067}"),
    (0x68, "\u{0068}"),
    (0x69, "\u{0069}"),
    (0x6a, "\u{006A}"),
    (0x6b, "\u{006B}"),
    (0x6c, "\u{006C}"),
    (0x6d, "\u{006D}"),
    (0x6e, "\u{006E}"),
    (0x6f, "\u{006F}"),
    (0x70, "\u{0070}"),
    (0x71, "\u{0071}"),
    (0x72, "\u{0072}"),
    (0x73, "\u{0073}"),
    (0x74, "\u{0074}"),
    (0x75, "\u{0075}"),
    (0x76, "\u{0076}"),
    (0x77, "\u{0077}"),
    (0x78, "\u{0078}"),
    (0x79, "\u{0079}"),
    (0x7a, "\u{007A}"),
    (0x7b, "\u{007B}"),
    (0x7c, "\u{007C}"),
    (0x7d, "\u{007D}"),
    (0x7e, "\u{007E}"),
    (0x80, "\u{20AC}"),
    (0x82, "\u{201A}"),
    (0x83, "\u{0192}"),
    (0x84, "\u{201E}"),
    (0x85, "\u{2026}"),
    (0x86, "\u{2020}"),
    (0x87, "\u{2021}"),
    (0x88, "\u{02C6}"),
    (0x89, "\u{2030}"),
    (0x8a, "\u{0160}"),
    (0x8b, "\u{2039}"),
    (0x8c, "\u{0152}"),
    (0x8e, "\u{017D}"),
    (0x91, "\u{2018}"),
    (0x92, "\u{2019}"),
    (0x93, "\u{201C}"),
    (0x94, "\u{201D}"),
    (0x95, "\u{2022}"),
    (0x96, "\u{2013}"),
    (0x97, "\u{2014}"),
    (0x98, "\u{02DC}"),
    (0x99, "\u{2122}"),
    (0x9a, "\u{0161}"),
    (0x9b, "\u{203A}"),
    (0x9c, "\u{0153}"),
    (0x9e, "\u{017E}"),
    (0x9f, "\u{0178}"),
    (0xa0, "\u{0020}"),
    (0xa1, "\u{00A1}"),
    (0xa2, "\u{00A2}"),
    (0xa3, "\u{00A3}"),
    (0xa4, "\u{00A4}"),
    (0xa5, "\u{00A5}"),
    (0xa6, "\u{00A6}"),
    (0xa7, "\u{00A7}"),
    (0xa8, "\u{00A8}"),
    (0xa9, "\u{00A9}"),
    (0xaa, "\u{00AA}"),
    (0xab, "\u{00AB}"),
    (0xac, "\u{00AC}"),
    (0xad, "\u{002D}"),
    (0xae, "\u{00AE}"),
    (0xaf, "\u{00AF}"),
    (0xb0, "\u{00B0}"),
    (0xb1, "\u{00B1}"),
    (0xb2, "\u{00B2}"),
    (0xb3, "\u{00B3}"),
    (0xb4, "\u{00B4}"),
    (0xb5, "\u{00B5}"),
    (0xb6, "\u{00B6}"),
    (0xb7, "\u{00B7}"),
    (0xb8, "\u{00B8}"),
    (0xb9, "\u{00B9}"),
    (0xba, "\u{00BA}"),
    (0xbb, "\u{00BB}"),
    (0xbc, "\u{00BC}"),
    (0xbd, "\u{00BD}"),
    (0xbe, "\u{00BE}"),
    (0xbf, "\u{00BF}"),
    (0xc0, "\u{00C0}"),
    (0xc1, "\u{00C1}"),
    (0xc2, "\u{00C2}"),
    (0xc3, "\u{00C3}"),
    (0xc4, "\u{00C4}"),
    (0xc5, "\u{00C5}"),
    (0xc6, "\u{00C6}"),
    (0xc7, "\u{00C7}"),
    (0xc8, "\u{00C8}"),
    (0xc9, "\u{00C9}"),
    (0xca, "\u{00CA}"),
    (0xcb, "\u{00CB}"),
    (0xcc, "\u{00CC}"),
    (0xcd, "\u{00CD}"),
    (0xce, "\u{00CE}"),
    (0xcf, "\u{00CF}"),
    (0xd0, "\u{00D0}"),
    (0xd1, "\u{00D1}"),
    (0xd2, "\u{00D2}"),
    (0xd3, "\u{00D3}"),
    (0xd4, "\u{00D4}"),
    (0xd5, "\u{00D5}"),
    (0xd6, "\u{00D6}"),
    (0xd7, "\u{00D7}"),
    (0xd8, "\u{00D8}"),
    (0xd9, "\u{00D9}"),
    (0xda, "\u{00DA}"),
    (0xdb, "\u{00DB}"),
    (0xdc, "\u{00DC}"),
    (0xdd, "\u{00DD}"),
    (0xde, "\u{00DE}"),
    (0xdf, "\u{00DF}"),
    (0xe0, "\u{00E0}"),
    (0xe1, "\u{00E1}"),
    (0xe2, "\u{00E2}"),
    (0xe3, "\u{00E3}"),
    (0xe4, "\u{00E4}"),
    (0xe5, "\u{00E5}"),
    (0xe6, "\u{00E6}"),
    (0xe7, "\u{00E7}"),
    (0xe8, "\u{00E8}"),
    (0xe9, "\u{00E9}"),
    (0xea, "\u{00EA}"),
    (0xeb, "\u{00EB}"),
    (0xec, "\u{00EC}"),
    (0xed, "\u{00ED}"),
    (0xee, "\u{00EE}"),
    (0xef, "\u{00EF}"),
    (0xf0, "\u{00F0}"),
    (0xf1, "\u{00F1}"),
    (0xf2, "\u{00F2}"),
    (0xf3, "\u{00F3}"),
    (0xf4, "\u{00F4}"),
    (0xf5, "\u{00F5}"),
    (0xf6, "\u{00F6}"),
    (0xf7, "\u{00F7}"),
    (0xf8, "\u{00F8}"),
    (0xf9, "\u{00F9}"),
    (0xfa, "\u{00FA}"),
    (0xfb, "\u{00FB}"),
    (0xfc, "\u{00FC}"),
    (0xfd, "\u{00FD}"),
    (0xfe, "\u{00FE}"),
    (0xff, "\u{00FF}"),
];

/// The base encoding a name refers to, or `None` when it names none of them.
///
/// **Only `WinAnsiEncoding` is here.** `MacRomanEncoding` and `StandardEncoding` are
/// equally published and equally absent from this engine, and writing them from memory
/// against no document that uses one is how a table gets a wrong entry that nothing
/// catches. Nothing in the corpus names them; a document that does now records a decision
/// saying so, which is the difference between a gap and a silence.
#[must_use]
pub fn base_encoding(name: &str) -> Option<CMap> {
    let table: &[(u8, &str)] = match name {
        "WinAnsiEncoding" => WIN_ANSI,
        _ => return None,
    };
    let mappings: BTreeMap<Vec<u8>, String> =
        table.iter().map(|(code, text)| (vec![*code], (*text).to_string())).collect();
    Some(CMap { name: name.to_string(), mappings: Arc::new(mappings), ..CMap::default() })
}

/// Whether a name is one Annex D defines, whether or not this engine carries its table.
///
/// Separate from [`base_encoding`] so that a caller can tell "this is an encoding I do not
/// have" from "this is not an encoding at all", and say which in a decision.
#[must_use]
pub const fn is_base_encoding_name(name: &str) -> bool {
    matches!(
        name.as_bytes(),
        b"WinAnsiEncoding" | b"MacRomanEncoding" | b"StandardEncoding" | b"MacExpertEncoding"
    )
}
