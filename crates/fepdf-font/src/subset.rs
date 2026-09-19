/// The six-letter tag on a subsetted font's `/BaseFont`, if it carries one.
///
/// A font program that holds only the glyphs a document uses is named `ABCDEF+Original`:
/// exactly six uppercase letters, a `+`, and the original name. **This is the one place
/// that decides it.** Four sites used to decide it separately, in two disagreeing forms:
/// `base_font.contains('+')`, which calls `Arial+Bold` a subset, and a positional test at
/// byte 6 that accepted any six characters, digits and lowercase included.
///
/// The two forms separate no file in the 524-file corpus — all 316 subsetted names there
/// match both — so this unification fixes a divergence that had not yet been reached, not
/// a reading that was wrong today.
///
/// [ADR-0071](../../../docs/adr/0071-three-declarations-that-read-nothing-and-one-that-wrote-nothing.md).
pub fn subset_tag(base_font: &str) -> Option<&str> {
    let bytes = base_font.as_bytes();
    if bytes.len() < 8 || bytes[6] != b'+' {
        return None;
    }
    let tag = &base_font[..6];
    tag.bytes().all(|b| b.is_ascii_uppercase()).then_some(tag)
}

// ---------------------------------------------------------------------------
// Subsetting a TrueType program: glyph ids stay where they are.
// ---------------------------------------------------------------------------

use crate::reconstruction::FontReconstructor;
use crate::{FontError, FontResult};
use std::collections::BTreeSet;

/// The tables a subsetted TrueType program keeps.
///
/// **Kept rather than dropped**, because a table this engine does not understand is a
/// table whose absence it cannot predict the effect of. `DSIG` goes because a signature
/// over a font that has just been rewritten is false by construction; the layout tables
/// go because nothing in a PDF's simple or CID font model consults them — a viewer places
/// glyphs from the PDF's own positioning, not from `GPOS`.
///
/// **`vhea`, `vmtx` and `VORG` are here because a page can be set vertically.** They are
/// where a glyph's vertical advance and origin live, and a subset that dropped them would
/// leave vertical Japanese to fall back on synthesised metrics — the one writing mode this
/// engine goes out of its way to keep, down to the TJ offsets that hold ruby in place.
///
/// **The exact list 9.9 requires of an embedded program is not checked here against the
/// standard's text**, which outranks this comment; reading it is the first task of the
/// item that writes the `/FontFile2` (W-E1c). A variable face's `fvar`, `gvar` and `avar`
/// are **not** kept and not yet thought about: this list is for the static faces the
/// corpus presents, and an instance of a variable one is a different question.
const KEPT_TABLES: [&[u8; 4]; 14] = [
    b"head", b"hhea", b"maxp", b"loca", b"glyf", b"hmtx", b"cvt ", b"fpgm", b"prep", b"cmap",
    b"OS/2", b"vhea", b"vmtx", b"VORG",
];

/// Where each glyph's outline starts and ends, and whether `loca` is in the long form.
///
/// `head.indexToLocFormat` decides the form, and getting it wrong moves every glyph:
/// the short form stores an offset **halved**, so reading it as bytes finds the middle of
/// some other glyph rather than failing.
fn loca_offsets(program: &[u8]) -> FontResult<(Vec<u32>, bool)> {
    let err = |m: &str| FontError::Internal(m.to_string());
    let (head, _) = find_table(program, b"head").ok_or_else(|| err("no head table"))?;
    let (maxp, _) = find_table(program, b"maxp").ok_or_else(|| err("no maxp table"))?;
    let (loca, loca_end) = find_table(program, b"loca").ok_or_else(|| err("no loca table"))?;

    let long = read_u16(program, head + 50).ok_or_else(|| err("head is truncated"))? == 1;
    let num_glyphs = read_u16(program, maxp + 4).ok_or_else(|| err("maxp is truncated"))?;

    let mut offsets = Vec::with_capacity(usize::from(num_glyphs) + 1);
    for i in 0..=usize::from(num_glyphs) {
        let value = if long {
            let at = loca.checked_add(i * 4).ok_or_else(|| err("loca overflows"))?;
            if at + 4 > loca_end {
                break;
            }
            read_u32(program, at).ok_or_else(|| err("loca is truncated"))?
        } else {
            let at = loca.checked_add(i * 2).ok_or_else(|| err("loca overflows"))?;
            if at + 2 > loca_end {
                break;
            }
            u32::from(read_u16(program, at).ok_or_else(|| err("loca is truncated"))?) * 2
        };
        offsets.push(value);
    }
    if offsets.len() < 2 {
        return Err(err("loca holds no glyph"));
    }
    Ok((offsets, long))
}

/// The glyphs a composite glyph draws, as it names them.
///
/// **A composite is why a subset cannot be the set that was asked for.** Dropping a
/// component leaves the glyph that references it drawing part of itself — an accent
/// without its letter — which renders as a defect rather than as an error.
fn components_of(glyph: &[u8]) -> Vec<u16> {
    // int16 numberOfContours, then four int16 of bounding box.
    let Some(contours) = read_u16(glyph, 0) else { return Vec::new() };
    if (contours as i16) >= 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut at = 10;
    loop {
        let (Some(flags), Some(index)) = (read_u16(glyph, at), read_u16(glyph, at + 2)) else {
            return out;
        };
        out.push(index);
        at += 4;
        at += if flags & 0x0001 != 0 { 4 } else { 2 }; // ARG_1_AND_2_ARE_WORDS
        at += if flags & 0x0008 != 0 {
            2 // WE_HAVE_A_SCALE
        } else if flags & 0x0040 != 0 {
            4 // WE_HAVE_AN_X_AND_Y_SCALE
        } else if flags & 0x0080 != 0 {
            8 // WE_HAVE_A_TWO_BY_TWO
        } else {
            0
        };
        if flags & 0x0020 == 0 {
            // MORE_COMPONENTS
            return out;
        }
    }
}

/// `wanted`, glyph 0, and every glyph a kept composite draws, transitively.
///
/// # Errors
/// Fails when the program carries no `loca`, `head` or `maxp` to read.
pub fn glyph_closure(program: &[u8], wanted: &BTreeSet<u16>) -> FontResult<BTreeSet<u16>> {
    let (offsets, _) = loca_offsets(program)?;
    let (glyf, glyf_end) =
        find_table(program, b"glyf").ok_or_else(|| FontError::Internal("no glyf table".into()))?;

    let mut kept: BTreeSet<u16> = BTreeSet::new();
    // Glyph 0 is `.notdef` and a program without it is not a font.
    let mut pending: Vec<u16> = wanted.iter().copied().chain(std::iter::once(0)).collect();
    while let Some(gid) = pending.pop() {
        if !kept.insert(gid) {
            continue;
        }
        let Some(outline) = glyph_bytes(program, &offsets, glyf, glyf_end, gid) else { continue };
        for component in components_of(outline) {
            if !kept.contains(&component) {
                pending.push(component);
            }
        }
    }
    Ok(kept)
}

/// The bytes of glyph `gid`, or `None` when it has no outline or does not exist.
fn glyph_bytes<'a>(
    program: &'a [u8],
    offsets: &[u32],
    glyf: usize,
    glyf_end: usize,
    gid: u16,
) -> Option<&'a [u8]> {
    let i = usize::from(gid);
    let (from, to) = (*offsets.get(i)? as usize, *offsets.get(i + 1)? as usize);
    if to <= from {
        return None; // An empty glyph, such as a space: legal, and nothing to keep.
    }
    let (start, end) = (glyf.checked_add(from)?, glyf.checked_add(to)?);
    if end > glyf_end || end > program.len() {
        return None;
    }
    program.get(start..end)
}

/// The `CFF ` table of `program`, where it has one.
///
/// **A program has outlines by one route or the other**, and which one decides what can
/// be subsetted: `glyf` by [`subset_truetype`], and a charstring index by nothing yet.
#[must_use]
pub fn cff_table(program: &[u8]) -> Option<&[u8]> {
    let (from, to) = find_table(program, b"CFF ")?;
    program.get(from..to)
}

/// The outline bytes of glyph `gid`, as `glyf` holds them.
///
/// `None` where the glyph has no outline — a space is a legal empty entry — or where the
/// program does not carry the tables to find one.
#[must_use]
pub fn glyph_outline(program: &[u8], gid: u16) -> Option<&[u8]> {
    let (offsets, _) = loca_offsets(program).ok()?;
    let (glyf, glyf_end) = find_table(program, b"glyf")?;
    glyph_bytes(program, &offsets, glyf, glyf_end, gid)
}

/// A TrueType program drawing `glyphs` and nothing else.
///
/// **Glyph ids do not move.** A subset that renumbers has to renumber `cmap`, `hmtx`,
/// every composite's components and the PDF's own `/CIDToGIDMap` with it, and each of
/// those is a place to be wrong in a way that renders as the wrong letter rather than as
/// an error. Here `loca` keeps its length and a dropped glyph becomes a zero-length
/// entry, so every other table stays true as it is and the weight still goes: `glyf` is
/// the bulk of a CJK program by a wide margin.
///
/// # Errors
/// Fails when the program is not SFNT, or carries no `glyf`, `loca`, `head` or `maxp`.
pub fn subset_truetype(program: &[u8], glyphs: &BTreeSet<u16>) -> FontResult<Vec<u8>> {
    let kept = glyph_closure(program, glyphs)?;
    let (offsets, long) = loca_offsets(program)?;
    let (glyf, glyf_end) =
        find_table(program, b"glyf").ok_or_else(|| FontError::Internal("no glyf table".into()))?;

    let mut new_glyf: Vec<u8> = Vec::new();
    let mut new_loca: Vec<u32> = Vec::with_capacity(offsets.len());
    for gid in 0..offsets.len().saturating_sub(1) {
        new_loca.push(u32::try_from(new_glyf.len()).unwrap_or(u32::MAX));
        let Ok(gid) = u16::try_from(gid) else { continue };
        if !kept.contains(&gid) {
            continue;
        }
        if let Some(outline) = glyph_bytes(program, &offsets, glyf, glyf_end, gid) {
            new_glyf.extend_from_slice(outline);
            // The short form of `loca` stores a halved offset, so every glyph has to
            // start on an even byte or the next one cannot be addressed at all.
            while !new_glyf.len().is_multiple_of(2) {
                new_glyf.push(0);
            }
        }
    }
    new_loca.push(u32::try_from(new_glyf.len()).unwrap_or(u32::MAX));

    let disassembled = FontReconstructor::disassemble_sfnt(program)?;
    let mut tables: Vec<([u8; 4], Vec<u8>)> =
        disassembled.tables.into_iter().filter(|(tag, _)| KEPT_TABLES.contains(&tag)).collect();
    replace_table(&mut tables, b"glyf", new_glyf);
    replace_table(&mut tables, b"loca", encode_loca(&new_loca, long));
    FontReconstructor::assemble_sfnt(&disassembled.magic, &tables)
}

/// `loca` in the form `head.indexToLocFormat` declares.
fn encode_loca(offsets: &[u32], long: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(offsets.len() * if long { 4 } else { 2 });
    for offset in offsets {
        if long {
            out.extend_from_slice(&offset.to_be_bytes());
        } else {
            out.extend_from_slice(&u16::try_from(offset / 2).unwrap_or(u16::MAX).to_be_bytes());
        }
    }
    out
}

fn replace_table(tables: &mut Vec<([u8; 4], Vec<u8>)>, tag: &[u8; 4], data: Vec<u8>) {
    match tables.iter_mut().find(|(t, _)| t == tag) {
        Some(entry) => entry.1 = data,
        None => tables.push((*tag, data)),
    }
}

fn find_table(program: &[u8], tag: &[u8; 4]) -> Option<(usize, usize)> {
    crate::reconstruction::find_table_range(program, tag)
}

fn read_u16(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?]))
}

fn read_u32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *data.get(at)?,
        *data.get(at + 1)?,
        *data.get(at + 2)?,
        *data.get(at + 3)?,
    ]))
}

#[cfg(test)]
mod subset_tag_tests {
    use super::subset_tag;

    #[test]
    fn a_six_letter_tag_is_read() {
        assert_eq!(subset_tag("ARKUPQ+MSMincho"), Some("ARKUPQ"));
    }

    #[test]
    fn a_plus_elsewhere_in_the_name_is_not_a_tag() {
        assert_eq!(subset_tag("Arial+Bold"), None);
        assert_eq!(subset_tag("Helvetica"), None);
    }

    #[test]
    fn a_tag_that_is_not_six_uppercase_letters_is_not_a_tag() {
        assert_eq!(subset_tag("abcdef+Arial"), None);
        assert_eq!(subset_tag("ABC123+Arial"), None);
        assert_eq!(subset_tag("ABCDE+Arial"), None);
        assert_eq!(subset_tag("ABCDEFG+Arial"), None);
    }

    #[test]
    fn a_tag_with_nothing_after_it_is_not_a_tag() {
        assert_eq!(subset_tag("ABCDEF+"), None);
    }
}

#[cfg(test)]
mod subset_truetype_tests {
    use super::{glyph_closure, subset_truetype};
    use std::collections::BTreeSet;

    /// One glyph outline: `contours` of them, and `filler` bytes standing for the points.
    fn simple_glyph(contours: i16, filler: u8, len: usize) -> Vec<u8> {
        let mut g = Vec::new();
        g.extend_from_slice(&contours.to_be_bytes());
        g.extend_from_slice(&[0; 8]); // the bounding box
        g.extend(std::iter::repeat_n(filler, len));
        g
    }

    /// A composite glyph drawing `component` once, with byte arguments and no scale.
    fn composite_glyph(component: u16) -> Vec<u8> {
        let mut g = Vec::new();
        g.extend_from_slice(&(-1i16).to_be_bytes());
        g.extend_from_slice(&[0; 8]);
        g.extend_from_slice(&0u16.to_be_bytes()); // flags: byte args, no scale, no more
        g.extend_from_slice(&component.to_be_bytes());
        g.extend_from_slice(&[0, 0]); // the two arguments
        g
    }

    /// A four-glyph program: 0 `.notdef`, 1 simple, 2 composite drawing 3, 3 simple.
    ///
    /// `long` chooses `head.indexToLocFormat`, which is the field that decides whether a
    /// `loca` entry is an offset or half of one.
    fn program(long: bool) -> Vec<u8> {
        let glyphs = [
            simple_glyph(1, 0xA1, 6),
            simple_glyph(1, 0xB2, 10),
            composite_glyph(3),
            simple_glyph(2, 0xD4, 14),
        ];
        let mut glyf = Vec::new();
        let mut loca_values = vec![0u32];
        for g in &glyphs {
            glyf.extend_from_slice(g);
            while glyf.len() % 2 != 0 {
                glyf.push(0);
            }
            loca_values.push(u32::try_from(glyf.len()).unwrap_or_default());
        }

        let mut head = vec![0u8; 54];
        head[50..52].copy_from_slice(&(i16::from(long)).to_be_bytes());
        let mut maxp = vec![0u8; 6];
        maxp[4..6].copy_from_slice(&(glyphs.len() as u16).to_be_bytes());

        let mut loca = Vec::new();
        for value in &loca_values {
            if long {
                loca.extend_from_slice(&value.to_be_bytes());
            } else {
                loca.extend_from_slice(&u16::try_from(value / 2).unwrap_or_default().to_be_bytes());
            }
        }

        sfnt(&[
            (*b"head", head),
            (*b"maxp", maxp),
            (*b"loca", loca),
            (*b"glyf", glyf),
            (*b"vmtx", vec![0x5A; 16]),
            (*b"post", vec![0x77; 12]),
        ])
    }

    /// The tables, in an SFNT container.
    fn sfnt(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
        out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
        out.extend_from_slice(&[0; 6]);
        let mut offset = 12 + tables.len() * 16;
        for (tag, data) in tables {
            out.extend_from_slice(tag);
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&u32::try_from(offset).unwrap_or_default().to_be_bytes());
            out.extend_from_slice(&u32::try_from(data.len()).unwrap_or_default().to_be_bytes());
            offset += (data.len() + 3) & !3;
        }
        for (_, data) in tables {
            out.extend_from_slice(data);
            out.extend(std::iter::repeat_n(0, (4 - (data.len() % 4)) % 4));
        }
        out
    }

    /// The bytes of glyph `gid` in `program`, read back the way a rasteriser would.
    fn outline(program: &[u8], gid: u16) -> Vec<u8> {
        let (offsets, _) = super::loca_offsets(program).expect("the program has a loca");
        let (glyf, end) = super::find_table(program, b"glyf").expect("it has a glyf");
        super::glyph_bytes(program, &offsets, glyf, end, gid).unwrap_or_default().to_vec()
    }

    /// **A composite draws glyphs nobody asked for, and they have to come too.**
    #[test]
    fn the_closure_pulls_in_what_a_composite_draws() {
        let kept = glyph_closure(&program(true), &BTreeSet::from([2])).expect("it closes");
        assert!(kept.contains(&3), "glyph 2 draws glyph 3, which is not in the subset: {kept:?}");
    }

    /// `.notdef` is glyph 0 and a program without it is not a font.
    #[test]
    fn glyph_zero_is_always_kept() {
        let kept = glyph_closure(&program(true), &BTreeSet::from([1])).expect("it closes");
        assert!(kept.contains(&0));
    }

    #[test]
    fn a_kept_glyph_survives_byte_for_byte() {
        let original = program(true);
        let subsetted = subset_truetype(&original, &BTreeSet::from([1])).expect("it subsets");
        assert_eq!(outline(&subsetted, 1), outline(&original, 1));
    }

    #[test]
    fn a_dropped_glyph_is_gone_and_its_neighbours_are_not() {
        let original = program(true);
        let subsetted = subset_truetype(&original, &BTreeSet::from([1])).expect("it subsets");
        assert!(outline(&subsetted, 3).is_empty(), "glyph 3 was not asked for");
        assert_eq!(outline(&subsetted, 1), outline(&original, 1));
        assert_eq!(outline(&subsetted, 0), outline(&original, 0));
    }

    /// The whole reason ids are left where they are.
    #[test]
    fn the_glyphs_that_remain_keep_their_ids() {
        let original = program(true);
        let subsetted = subset_truetype(&original, &BTreeSet::from([2])).expect("it subsets");
        assert_eq!(outline(&subsetted, 2), outline(&original, 2));
        assert_eq!(outline(&subsetted, 3), outline(&original, 3), "the component moved");
    }

    /// **The short form halves every offset**, so a subset that writes bytes into it puts
    /// each glyph at twice its address and the font draws something else entirely.
    #[test]
    fn the_short_loca_form_survives_a_subset() {
        let original = program(false);
        let subsetted = subset_truetype(&original, &BTreeSet::from([1, 2])).expect("it subsets");
        assert_eq!(outline(&subsetted, 1), outline(&original, 1));
        assert_eq!(outline(&subsetted, 2), outline(&original, 2));
        assert_eq!(outline(&subsetted, 3), outline(&original, 3));
    }

    /// **A page can be set vertically, and `vmtx` is where that advance lives.**
    #[test]
    fn the_vertical_metrics_survive_a_subset() {
        let subsetted = subset_truetype(&program(true), &BTreeSet::from([1])).expect("it subsets");
        let (from, to) = super::find_table(&subsetted, b"vmtx").expect("vmtx is kept");
        assert_eq!(&subsetted[from..to], &[0x5A; 16], "the vertical metrics changed");
    }

    /// And a table nothing consults does not come along.
    #[test]
    fn a_table_the_pdf_model_does_not_consult_is_dropped() {
        let subsetted = subset_truetype(&program(true), &BTreeSet::from([1])).expect("it subsets");
        assert!(super::find_table(&subsetted, b"post").is_none(), "post is still there");
    }

    /// A subset is smaller, or it is not doing anything.
    #[test]
    fn the_outlines_nobody_asked_for_are_not_in_the_output() {
        let original = program(true);
        let subsetted = subset_truetype(&original, &BTreeSet::from([1])).expect("it subsets");
        let (glyf, end) = super::find_table(&subsetted, b"glyf").expect("it has a glyf");
        assert!(
            !subsetted[glyf..end].windows(4).any(|w| w == [0xD4; 4]),
            "glyph 3's outline is still in the file"
        );
    }
}

#[cfg(test)]
mod collection_tests {
    use super::super::reconstruction::sfnt_base;

    /// A collection holding one font whose directory sits at `offset`.
    fn collection(offset: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ttcf");
        out.extend_from_slice(&0x0001_0000_u32.to_be_bytes()); // version
        out.extend_from_slice(&1u32.to_be_bytes()); // numFonts
        out.extend_from_slice(&offset.to_be_bytes());
        out.resize(offset as usize + 12, 0);
        out
    }

    /// **Every face installed on this machine is a collection**, so this is the ordinary
    /// case rather than the odd one.
    #[test]
    fn a_collection_is_read_from_its_first_font() {
        assert_eq!(sfnt_base(&collection(40)), 40);
    }

    #[test]
    fn a_plain_font_starts_where_it_starts() {
        let mut plain = vec![0u8; 20];
        plain[0..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
        assert_eq!(sfnt_base(&plain), 0);
    }

    /// An offset past the end is a file saying something impossible, and reading from it
    /// would be reading whatever is in memory after the buffer.
    #[test]
    fn an_offset_that_does_not_fit_is_not_followed() {
        let mut truncated = collection(40);
        truncated.truncate(30);
        assert_eq!(sfnt_base(&truncated), 0, "an offset past the end must not be followed");
        assert_eq!(sfnt_base(b"ttcf"), 0, "a header too short to hold an offset states none");
    }
}
