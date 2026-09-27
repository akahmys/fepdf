//! Which glyphs an embedded font program holds, by name or by CID.
//!
//! **What the program says, read from the program.** A font descriptor's `/CharSet` and
//! `/CIDSet` are claims about the program beside them (ISO 32000-1 9.8, Tables 122 and
//! 124), and ISO 14289-1 7.21.4.2 asks that the claim and the program agree. These read
//! the second half.

use std::collections::BTreeSet;

/// Each glyph's advance, by glyph index, in units of 1/1000 em: an SFNT program's `hmtx`
/// advances scaled by its `head` table's `unitsPerEm`, the last advance repeated for the
/// glyphs past `hhea`'s `numberOfHMetrics`, as the format says.
#[must_use]
pub fn sfnt_advances(program: &[u8]) -> Option<Vec<f64>> {
    let raw = ttf_parser::RawFace::parse(program, 0).ok()?;
    let table = |tag: &[u8; 4]| raw.table(ttf_parser::Tag::from_bytes(tag));
    let word =
        |bytes: &[u8], at: usize| bytes.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let units = f64::from(word(table(b"head")?, 18)?.max(1));
    let metrics = usize::from(word(table(b"hhea")?, 34)?);
    let hmtx = table(b"hmtx")?;
    let count = usize::from(sfnt_glyph_count(program)?);
    let advances: Vec<f64> = (0..metrics)
        .map_while(|g| word(hmtx, g * 4))
        .map(|a| f64::from(a) * 1000.0 / units)
        .collect();
    let last = *advances.last()?;
    Some((0..count).map(|g| advances.get(g).copied().unwrap_or(last)).collect())
}

/// Each glyph's advance, by name, in units of 1/1000 em, from a name-keyed CFF program:
/// its charstring's width, or the Private DICT's, through the program's `FontMatrix`.
/// Nothing for a CID-keyed program, whose widths are per font dictionary.
#[must_use]
pub fn cff_name_advances(program: &[u8]) -> Option<std::collections::BTreeMap<String, f64>> {
    let table = cff_table(program)?;
    let scale = f64::from(table.matrix().sx) * 1000.0;
    let advances: std::collections::BTreeMap<String, f64> = (0..table.number_of_glyphs())
        .filter_map(|gid| {
            let glyph = ttf_parser::GlyphId(gid);
            Some((
                table.glyph_name(glyph)?.to_owned(),
                f64::from(table.glyph_width(glyph)?) * scale,
            ))
        })
        .collect();
    (!advances.is_empty()).then_some(advances)
}

/// Each glyph's advance, by name, in units of 1/1000 em, from a Type 1 program: its
/// charstring's `hsbw` or `sbw`, through the `/FontMatrix` its cleartext states, or
/// `[0.001 0 0 0.001 0 0]` where it states none.
#[must_use]
pub fn type1_advances(
    program: &[u8],
    cleartext: usize,
) -> Option<std::collections::BTreeMap<String, f64>> {
    let (ascii, rest) = program.split_at_checked(cleartext)?;
    let text = String::from_utf8_lossy(ascii);
    let scale = text
        .find("/FontMatrix")
        .and_then(|at| text[at..].split(['[', ']']).nth(1))
        .and_then(|inner| inner.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.001)
        * 1000.0;
    let advances =
        crate::reconstruction::FontReconstructor::type1_advances(ascii, &eexec_portion(rest))?;
    let advances: std::collections::BTreeMap<String, f64> =
        advances.into_iter().map(|(name, width)| (name, width * scale)).collect();
    (!advances.is_empty()).then_some(advances)
}

/// A Type 1 program's eexec-encrypted portion as bytes: as it is, or, written in
/// hexadecimal, decoded (ISO 32000-1 9.9).
fn eexec_portion(rest: &[u8]) -> Vec<u8> {
    if !rest.iter().take(4).all(u8::is_ascii_hexdigit) {
        return rest.to_vec();
    }
    let digits: Vec<u8> = rest.iter().copied().filter(u8::is_ascii_hexdigit).collect();
    digits
        .chunks_exact(2)
        .filter_map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect()
}

/// A CFF program's table, as `ttf_parser` reads it.
pub type CffTable<'a> = ttf_parser::cff::Table<'a>;

/// The CFF table of `program` — a bare CFF, or the `CFF ` table of an SFNT.
#[must_use]
pub fn cff_table(program: &[u8]) -> Option<CffTable<'_>> {
    ttf_parser::cff::Table::parse(crate::cff::body(program))
}

/// The encoding a Type 1 program's cleartext portion states for itself (ISO 32000-1
/// 9.6.6.2, a font's built-in encoding): `StandardEncoding` by name, or the codes its
/// `dup <code> /<name> put` entries assign. **Nothing when it states neither**, rather than
/// an empty table that would call every code undefined.
#[must_use]
pub fn type1_built_in_encoding(cleartext: &[u8]) -> Option<std::collections::BTreeMap<u8, String>> {
    let text = String::from_utf8_lossy(cleartext);
    let at = text.find("/Encoding")?;
    let rest = &text[at + "/Encoding".len()..];
    if rest.trim_start().starts_with("StandardEncoding") {
        return Some(
            crate::latin_names::STANDARD_ENCODING
                .iter()
                .map(|(code, name)| (*code, (*name).to_owned()))
                .collect(),
        );
    }
    let body = rest.split(" def").next().unwrap_or(rest);
    let words: Vec<&str> = body.split_whitespace().collect();
    let table: std::collections::BTreeMap<u8, String> = words
        .windows(4)
        .filter(|w| w[0] == "dup" && w[3] == "put")
        .filter_map(|w| Some((w[1].parse().ok()?, w[2].strip_prefix('/')?.to_owned())))
        .collect();
    (!table.is_empty()).then_some(table)
}

/// The glyph names a CFF program (`/FontFile3`, `/Type1C`) defines, `.notdef` left out as
/// `/CharSet` leaves it out.
#[must_use]
pub fn cff_glyph_names(program: &[u8]) -> Option<BTreeSet<String>> {
    let table = ttf_parser::cff::Table::parse(crate::cff::body(program))?;
    Some(
        (0..table.number_of_glyphs())
            .filter_map(|gid| table.glyph_name(ttf_parser::GlyphId(gid)))
            .filter(|name| *name != ".notdef")
            .map(str::to_owned)
            .collect(),
    )
}

/// The CIDs a CFF program holds: its charset's, when it is CID-keyed, and otherwise its
/// glyph indices, which a CIDFont with no CID-keyed program uses as CIDs.
#[must_use]
pub fn cff_cids(program: &[u8]) -> Option<BTreeSet<u32>> {
    cff_cid_glyphs(program).map(|glyphs| glyphs.into_keys().collect())
}

/// Each CID a CFF program holds, and the glyph index it has there.
#[must_use]
pub fn cff_cid_glyphs(program: &[u8]) -> Option<std::collections::BTreeMap<u32, u16>> {
    let table = ttf_parser::cff::Table::parse(crate::cff::body(program))?;
    Some(
        (0..table.number_of_glyphs())
            .map(|gid| {
                let cid =
                    table.glyph_cid(ttf_parser::GlyphId(gid)).map_or(u32::from(gid), u32::from);
                (cid, gid)
            })
            .collect(),
    )
}

/// How many glyphs an SFNT program has, by its `maxp` table.
#[must_use]
pub fn sfnt_glyph_count(program: &[u8]) -> Option<u16> {
    let raw = ttf_parser::RawFace::parse(program, 0).ok()?;
    let maxp = raw.table(ttf_parser::Tag::from_bytes(b"maxp"))?;
    maxp.get(4..6).map(|b| u16::from_be_bytes([b[0], b[1]]))
}

/// The glyphs of an SFNT program that have an outline: those whose `loca` entries differ.
///
/// **Having an outline, not merely a slot.** A subsetted program commonly keeps every slot
/// and empties the glyphs it drops, and a glyph with no outline — a space — can equally be
/// one the font means to hold; this is the set that is certainly there.
#[must_use]
pub fn sfnt_outlined_glyphs(program: &[u8]) -> Option<BTreeSet<u16>> {
    let raw = ttf_parser::RawFace::parse(program, 0).ok()?;
    let head = raw.table(ttf_parser::Tag::from_bytes(b"head"))?;
    let loca = raw.table(ttf_parser::Tag::from_bytes(b"loca"))?;
    let long = head.get(50..52)? == [0, 1];
    let count = sfnt_glyph_count(program)?;
    let offset = |gid: usize| -> Option<u32> {
        if long {
            loca.get(gid * 4..gid * 4 + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        } else {
            loca.get(gid * 2..gid * 2 + 2).map(|b| u32::from(u16::from_be_bytes([b[0], b[1]])) * 2)
        }
    };
    Some(
        (0..count)
            .filter(|gid| {
                let at = usize::from(*gid);
                matches!((offset(at), offset(at + 1)), (Some(a), Some(b)) if b > a)
            })
            .collect(),
    )
}

/// The glyph names a Type 1 program (`/FontFile`) defines in its `/CharStrings`, `.notdef`
/// left out: its first `cleartext` bytes as they are, and the rest eexec-encrypted,
/// in binary or in hexadecimal (ISO 32000-1 9.9, `/Length1` and `/Length2`).
#[must_use]
pub fn type1_glyph_names(program: &[u8], cleartext: usize) -> Option<BTreeSet<String>> {
    let (ascii, rest) = program.split_at_checked(cleartext)?;
    let encrypted = eexec_portion(rest);
    let mut names =
        crate::reconstruction::FontReconstructor::type1_charstring_names(ascii, &encrypted)?;
    names.remove(".notdef");
    Some(names)
}

/// Programs built by hand, and what each is read as holding.
#[cfg(test)]
mod programs {
    use super::{cff_glyph_names, sfnt_glyph_count, sfnt_outlined_glyphs, type1_glyph_names};

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
            .collect()
    }

    /// A Type 1 program of 60 cleartext bytes, then `/CharStrings` for `.notdef`, `A`
    /// and `B` under eexec.
    pub(crate) const TYPE_1: &str = "2521466f6e7454797065312d312e303a20546573740a2f466f6e744e616d65202f54657374206465660a63757272656e7466696c652065657865630ad9d66f633b846a989b9974b0179fc6cc4452954d3a4fc272596999ba876cc6961876a36a3e0691600d27978f3466dac5e6c0328e74b316219e55bef8b40bfa7097977a0ae48a5588a9eb85fa1944834b491163b221bbd8dcd4f7c18ab788697941010806744caceebc5ad9be2c2b7fab5e09aca140d90d8d8444595afff597d945510082af7873f1a9d72848220a";

    /// A name-keyed CFF program: `.notdef`, then `A` and `B` by their standard SIDs.
    pub(crate) const CFF: &str = "01000401000101010554657374000101010d1d000000220f1d0000002711000000000000220023000301010203040e0e0e";

    /// An SFNT program of three glyph slots, of which only glyph 1 has an outline.
    pub(crate) const THREE_SLOTS: &str = "000100000004004000020000676c7966000000000000004c0000000a686561640000000000000058000000366c6f63610000000000000090000000086d6178700000000000000098000000060000000000000000000000000001000000000000000000005f0f3cf5000003e800000000000000000000000000000000000000000000000000000000000000000000000000000000000500050000500000030000";

    #[test]
    fn a_type_1_programs_names_are_read_through_eexec() {
        let names = type1_glyph_names(&bytes(TYPE_1), 60).expect("it reads");
        assert_eq!(names.into_iter().collect::<Vec<_>>(), ["A", "B"]);
    }

    #[test]
    fn a_cff_programs_names_are_its_charsets() {
        let names = cff_glyph_names(&bytes(CFF)).expect("it reads");
        assert_eq!(names.into_iter().collect::<Vec<_>>(), ["A", "B"]);
    }

    #[test]
    fn an_sfnt_programs_outlines_are_its_differing_loca_entries() {
        let program = bytes(THREE_SLOTS);
        assert_eq!(sfnt_glyph_count(&program), Some(3));
        assert_eq!(
            sfnt_outlined_glyphs(&program).expect("it reads").into_iter().collect::<Vec<_>>(),
            [1]
        );
    }
}
