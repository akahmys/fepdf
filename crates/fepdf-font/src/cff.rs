//! Subsetting a CFF program, which is how a Japanese face is drawn.
//!
//! **The same rule as `glyf`: glyph ids do not move.** A charstring nobody asked for is
//! replaced by `endchar`, one byte, rather than removed — so the charset still maps every
//! glyph, `FDSelect` still names a font dictionary for each, and the count in every
//! structure stays what it was. Nothing outside the charstrings has to be understood well
//! enough to rewrite, which is the difference between a subsetter that is finished and one
//! that is nearly finished.
//!
//! What does have to be rewritten is the Top DICT, because a shorter charstrings index
//! moves everything laid out after it. Its offsets go out as five-byte fixed numbers, so
//! the dictionary's own size does not depend on the offsets it holds and one pass settles
//! them.
//!
//! **Three of the offsets here are held by a test that fails when they are left behind**,
//! and each took a mutation to establish: the charstrings, through the charstrings that
//! come back; the charset, through the map `inspect_cff` builds by reading it; and the
//! `FDArray` with the Private DICT each of its font dictionaries names, through
//! [`private_dicts`]. `FDSelect` is not checked on its own — it sits in the same block as
//! the charset and moves with it, so it is right by the same arithmetic and not by its own
//! evidence.

use crate::reconstruction::{FontReconstructor, get_index_item, skip_index};
use crate::{FontError, FontResult};
use std::collections::BTreeSet;

/// An operator of a CFF DICT, and the operands before it.
struct DictEntry {
    op: u16,
    operands: Vec<i32>,
}

/// Where each part of a CFF program sits, as byte ranges into it.
struct Layout {
    /// The end of the header, which is where the Name INDEX starts.
    header_end: usize,
    /// The Name INDEX, which the rewrite copies unchanged.
    name: (usize, usize),
    /// The String INDEX and the Global Subr INDEX, likewise.
    string: (usize, usize),
    gsubr: (usize, usize),
    /// Everything between the global subroutines and the charstrings: the charset, the
    /// encoding and `FDSelect` live here, and the rewrite moves the block as one.
    middle: (usize, usize),
    /// The charstrings, which the rewrite replaces.
    charstrings: (usize, usize),
    top_dict: Vec<DictEntry>,
}

/// The CFF bytes of `program`: the `CFF ` table of an SFNT, or the whole of a bare one.
///
/// A `/FontFile3` in a PDF is the program itself, and a face installed on a machine is a
/// table in a container. Both reach here.
#[must_use]
pub fn body(program: &[u8]) -> &[u8] {
    crate::subset::cff_table(program).unwrap_or(program)
}

/// The charstring of glyph `gid`, as the program holds it.
#[must_use]
pub fn charstring(program: &[u8], gid: u16) -> Option<Vec<u8>> {
    let cff = body(program);
    let layout = read_layout(cff).ok()?;
    get_index_item(cff, layout.charstrings.0, usize::from(gid))
}

/// How many glyphs the program draws.
///
/// # Errors
/// Fails when the program carries no charstrings index to count.
pub fn glyph_count(program: &[u8]) -> FontResult<usize> {
    let cff = body(program);
    let layout = read_layout(cff)?;
    Ok(index_count(cff, layout.charstrings.0))
}

/// A CFF program drawing `glyphs`, with `endchar` in place of every other charstring.
///
/// # Errors
/// Fails when the program's header, indexes or Top DICT cannot be read, or when it states
/// no charstrings.
pub fn subset_cff(program: &[u8], glyphs: &BTreeSet<u16>) -> FontResult<Vec<u8>> {
    let cff = body(program);
    let layout = read_layout(cff)?;
    let count = index_count(cff, layout.charstrings.0);
    if count == 0 {
        return Err(FontError::Internal("the program states no charstrings".into()));
    }

    // `endchar` on its own: a glyph that draws nothing, which is what a dropped one now is.
    let empty = vec![14u8];
    let kept: Vec<Vec<u8>> = (0..count)
        .map(|gid| {
            let wanted = u16::try_from(gid).is_ok_and(|g| glyphs.contains(&g) || g == 0);
            match wanted.then(|| get_index_item(cff, layout.charstrings.0, gid)).flatten() {
                Some(charstring) => charstring,
                None => empty.clone(),
            }
        })
        .collect();

    let mut charstrings = Vec::new();
    push_index(&mut charstrings, &kept.iter().map(Vec::as_slice).collect::<Vec<_>>());
    Ok(assemble(cff, &layout, &charstrings))
}

/// The parts of `cff`, and where its charstrings are.
fn read_layout(cff: &[u8]) -> FontResult<Layout> {
    let err = |m: &str| FontError::Internal(m.to_string());
    let header_size = usize::from(*cff.get(2).ok_or_else(|| err("the CFF has no header"))?);
    if header_size < 4 || header_size > cff.len() {
        return Err(err("the CFF header states an impossible size"));
    }

    let name_at = header_size;
    let top_at = skip_index(cff, name_at);
    let string_at = skip_index(cff, top_at);
    let gsubr_at = skip_index(cff, string_at);
    let gsubr_end = skip_index(cff, gsubr_at);

    let top_dict = read_dict(
        &get_index_item(cff, top_at, 0).ok_or_else(|| err("the CFF states no Top DICT"))?,
    );
    let charstrings_at = top_dict
        .iter()
        .find(|entry| entry.op == 17)
        .and_then(|entry| entry.operands.last())
        .and_then(|offset| usize::try_from(*offset).ok())
        .ok_or_else(|| err("the Top DICT names no charstrings"))?;
    if charstrings_at >= cff.len() {
        return Err(err("the charstrings index is past the end of the program"));
    }

    let charstrings_end = skip_index(cff, charstrings_at);
    if charstrings_at < gsubr_end || charstrings_end < charstrings_at {
        return Err(err("the charstrings do not sit after the global subroutines"));
    }

    Ok(Layout {
        header_end: header_size,
        name: (name_at, top_at),
        string: (string_at, gsubr_at),
        gsubr: (gsubr_at, gsubr_end),
        middle: (gsubr_end, charstrings_at),
        charstrings: (charstrings_at, charstrings_end),
        top_dict,
    })
}

/// The operators and operands of a DICT, in order.
fn read_dict(data: &[u8]) -> Vec<DictEntry> {
    let mut out = Vec::new();
    let mut operands: Vec<i32> = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let b0 = data[at];
        if b0 <= 21 {
            let mut op = u16::from(b0);
            at += 1;
            if op == 12 {
                op = (op << 8) | u16::from(data.get(at).copied().unwrap_or(0));
                at += 1;
            }
            out.push(DictEntry { op, operands: std::mem::take(&mut operands) });
            continue;
        }
        let (value, width) = read_operand(data, at);
        if width == 0 {
            break;
        }
        if let Some(value) = value {
            operands.push(value);
        }
        at += width;
    }
    out
}

/// One operand of a DICT at `at`: what it is worth, and how many bytes it took.
///
/// **A real number is counted and not valued.** No offset is written as one, so the value
/// goes out as zero and the operator that follows still lands in the right place — a
/// dictionary this engine does not rewrite survives being read.
fn read_operand(data: &[u8], at: usize) -> (Option<i32>, usize) {
    let byte = |i: usize| i32::from(data.get(i).copied().unwrap_or(0));
    match data.get(at).copied().unwrap_or(0) {
        28 => (
            Some(i32::from(i16::from_be_bytes([
                data.get(at + 1).copied().unwrap_or(0),
                data.get(at + 2).copied().unwrap_or(0),
            ]))),
            3,
        ),
        29 => (
            Some(i32::from_be_bytes([
                data.get(at + 1).copied().unwrap_or(0),
                data.get(at + 2).copied().unwrap_or(0),
                data.get(at + 3).copied().unwrap_or(0),
                data.get(at + 4).copied().unwrap_or(0),
            ])),
            5,
        ),
        30 => (Some(0), real_number_width(data, at)),
        b0 @ 32..=246 => (Some(i32::from(b0) - 139), 1),
        b0 @ 247..=250 => (Some((i32::from(b0) - 247) * 256 + byte(at + 1) + 108), 2),
        b0 @ 251..=254 => (Some(-(i32::from(b0) - 251) * 256 - byte(at + 1) - 108), 2),
        _ => (None, 1),
    }
}

/// How many bytes a real number occupies: nibbles until one of them is `f`.
fn real_number_width(data: &[u8], at: usize) -> usize {
    let mut width = 1;
    while let Some(byte) = data.get(at + width) {
        width += 1;
        if byte & 0x0F == 0x0F || byte >> 4 == 0x0F {
            break;
        }
    }
    width
}

/// How many items an INDEX at `at` holds.
fn index_count(data: &[u8], at: usize) -> usize {
    match (data.get(at), data.get(at + 1)) {
        (Some(hi), Some(lo)) => usize::from(u16::from_be_bytes([*hi, *lo])),
        _ => 0,
    }
}

/// An INDEX of `entries`, with four-byte offsets.
fn push_index(out: &mut Vec<u8>, entries: &[&[u8]]) {
    FontReconstructor::push_cff_index(out, entries);
}

/// A DICT operand as a five-byte number, so that its size does not depend on its value.
fn push_fixed(out: &mut Vec<u8>, value: i32) {
    out.push(29);
    out.extend_from_slice(&value.to_be_bytes());
}

/// A DICT operator, after its operands.
fn push_op(out: &mut Vec<u8>, op: u16) {
    if op > 0xFF {
        out.push(12);
        out.push(u8::try_from(op & 0xFF).unwrap_or(0));
    } else {
        out.push(u8::try_from(op).unwrap_or(0));
    }
}

/// The program, rebuilt with `charstrings` in place of the ones it had.
///
/// The parts go out in the order they came in, with one appended:
///
/// ```text
/// header | name | TOP DICT | string | gsubr | middle | CHARSTRINGS | tail | FDARRAY
/// ```
///
/// Only the three in capitals are written rather than copied. Two blocks move — the
/// middle, by the difference between the new Top DICT and the old, and the tail, by that
/// difference plus whatever the subset took off the charstrings — and every offset the Top
/// DICT holds points into one of them.
///
/// **The `FDArray` goes to the end instead of moving**, because its font dictionaries hold
/// offsets of their own: each names the Private DICT it uses, absolutely. Rewriting them
/// in place would change the array's length and move everything after it again, so the
/// rewritten array is appended and the copy left in the tail is dead bytes — a few hundred
/// of them, against the megabytes the subset takes off a CJK face.
fn assemble(cff: &[u8], layout: &Layout, charstrings: &[u8]) -> Vec<u8> {
    let slice = |(from, to): (usize, usize)| cff.get(from..to).unwrap_or_default();
    let new_top_dict = top_dict_size(&layout.top_dict);
    let old_top_dict = layout.string.0.saturating_sub(layout.name.1);

    let middle_shift =
        i64::try_from(new_top_dict).unwrap_or(0) - i64::try_from(old_top_dict).unwrap_or(0);
    let new_charstrings_at = shift_by(layout.charstrings.0, middle_shift);
    let tail_shift = middle_shift + i64::try_from(charstrings.len()).unwrap_or(0)
        - i64::try_from(layout.charstrings.1.saturating_sub(layout.charstrings.0)).unwrap_or(0);

    let tail = cff.get(layout.charstrings.1..).unwrap_or_default();
    let fdarray = rebuilt_fdarray(cff, layout, tail_shift);
    let fdarray_at = layout.header_end
        + (layout.name.1 - layout.name.0)
        + new_top_dict
        + (layout.string.1 - layout.string.0)
        + (layout.gsubr.1 - layout.gsubr.0)
        + (layout.middle.1 - layout.middle.0)
        + charstrings.len()
        + tail.len();

    let mut out = Vec::with_capacity(cff.len());
    out.extend_from_slice(slice((0, layout.header_end)));
    out.extend_from_slice(slice(layout.name));
    push_top_dict(
        &mut out,
        &layout.top_dict,
        new_charstrings_at,
        middle_shift,
        tail_shift,
        fdarray.as_ref().map(|_| fdarray_at),
    );
    out.extend_from_slice(slice(layout.string));
    out.extend_from_slice(slice(layout.gsubr));
    out.extend_from_slice(slice(layout.middle));
    out.extend_from_slice(charstrings);
    out.extend_from_slice(tail);
    if let Some(fdarray) = fdarray {
        out.extend_from_slice(&fdarray);
    }
    out
}

/// `FDArray`, with every font dictionary's Private offset moved by `tail_shift`.
///
/// `None` where the program has none, which is every CFF that is not CID-keyed.
fn rebuilt_fdarray(cff: &[u8], layout: &Layout, tail_shift: i64) -> Option<Vec<u8>> {
    let at = layout
        .top_dict
        .iter()
        .find(|entry| entry.op == 0x0C24)
        .and_then(|entry| entry.operands.last())
        .and_then(|offset| usize::try_from(*offset).ok())?;

    let dicts: Vec<Vec<u8>> = (0..index_count(cff, at))
        .map(|i| {
            let mut out = Vec::new();
            for entry in read_dict(&get_index_item(cff, at, i).unwrap_or_default()) {
                if entry.op == 18 {
                    let mut operands = entry.operands.iter();
                    push_fixed(&mut out, operands.next().copied().unwrap_or(0));
                    push_fixed(
                        &mut out,
                        shifted(operands.next().copied().unwrap_or(0), tail_shift),
                    );
                } else {
                    for operand in &entry.operands {
                        push_fixed(&mut out, *operand);
                    }
                }
                push_op(&mut out, entry.op);
            }
            out
        })
        .collect();

    let mut out = Vec::new();
    push_index(&mut out, &dicts.iter().map(Vec::as_slice).collect::<Vec<_>>());
    Some(out)
}

/// `at`, moved by `shift`.
fn shift_by(at: usize, shift: i64) -> usize {
    usize::try_from(i64::try_from(at).unwrap_or(0).saturating_add(shift)).unwrap_or(at)
}

/// How long the rewritten Top DICT will be: five bytes per operand, and one or two per
/// operator, inside an INDEX of one item with four-byte offsets.
fn top_dict_size(entries: &[DictEntry]) -> usize {
    let body: usize =
        entries.iter().map(|e| e.operands.len() * 5 + if e.op > 0xFF { 2 } else { 1 }).sum();
    // count(2) + offSize(1) + two offsets(8) + the item
    2 + 1 + 8 + body
}

/// The Top DICT, with every offset it holds moved to where that thing now is.
fn push_top_dict(
    out: &mut Vec<u8>,
    entries: &[DictEntry],
    charstrings_at: usize,
    middle_shift: i64,
    tail_shift: i64,
    fdarray_at: Option<usize>,
) {
    let mut body = Vec::new();
    for entry in entries {
        match entry.op {
            // CharStrings: where they now are.
            17 => push_fixed(&mut body, i32::try_from(charstrings_at).unwrap_or(0)),
            // charset, Encoding, FDSelect: in the part before the charstrings.
            15 | 16 | 0x0C25 => {
                for operand in &entry.operands {
                    push_fixed(&mut body, shifted(*operand, middle_shift));
                }
            }
            // Private [size offset] and FDArray: in the part after them.
            18 => {
                let mut operands = entry.operands.iter();
                let size = operands.next().copied().unwrap_or(0);
                let offset = operands.next().copied().unwrap_or(0);
                push_fixed(&mut body, size);
                push_fixed(&mut body, shifted(offset, tail_shift));
            }
            // FDArray: rewritten and appended, so it is where it now is rather than
            // where it was moved to.
            0x0C24 => {
                let at = fdarray_at.and_then(|at| i32::try_from(at).ok()).unwrap_or(0);
                push_fixed(&mut body, at);
            }
            _ => {
                for operand in &entry.operands {
                    push_fixed(&mut body, *operand);
                }
            }
        }
        push_op(&mut body, entry.op);
    }
    push_index(out, &[body.as_slice()]);
}

/// An offset moved by `shift`, which stays where it is when it points at nothing.
fn shifted(offset: i32, shift: i64) -> i32 {
    if offset <= 2 {
        // 0, 1 and 2 are the predefined charsets and encodings, not offsets at all.
        return offset;
    }
    i32::try_from(i64::from(offset) + shift).unwrap_or(offset)
}

/// The Private DICT bytes every font dictionary of `program` points at.
///
/// **This exists because the writer could not otherwise be checked.** A subset moves the
/// block those offsets point into, and leaving them behind produces a font that parses,
/// counts its glyphs correctly, keeps its charstrings byte for byte and draws with the
/// wrong stem hints and the wrong nominal widths — or, in a CID-keyed face, picks the
/// wrong font dictionary for a glyph. Nothing above reads far enough to notice; this does,
/// and a test compares what it reads before and after.
///
/// One entry per font dictionary: one for a plain CFF, one per `FDArray` entry for a
/// CID-keyed one. An offset that points outside the program yields an empty entry rather
/// than being dropped, so that a stale offset changes the answer instead of shortening it.
#[must_use]
pub fn private_dicts(program: &[u8]) -> Vec<Vec<u8>> {
    let cff = body(program);
    let Ok(layout) = read_layout(cff) else { return Vec::new() };

    let private_of = |entries: &[DictEntry]| -> Vec<u8> {
        let Some(entry) = entries.iter().find(|e| e.op == 18) else { return Vec::new() };
        let (Some(size), Some(offset)) = (entry.operands.first(), entry.operands.get(1)) else {
            return Vec::new();
        };
        let (Ok(size), Ok(offset)) = (usize::try_from(*size), usize::try_from(*offset)) else {
            return Vec::new();
        };
        cff.get(offset..offset.saturating_add(size)).unwrap_or_default().to_vec()
    };

    match layout.top_dict.iter().find(|e| e.op == 0x0C24).and_then(|e| e.operands.last()) {
        // CID-keyed: one font dictionary per `FDArray` entry, each with its own Private.
        Some(fdarray) => {
            let Ok(at) = usize::try_from(*fdarray) else { return Vec::new() };
            (0..index_count(cff, at))
                .map(|i| match get_index_item(cff, at, i) {
                    Some(font_dict) => private_of(&read_dict(&font_dict)),
                    None => Vec::new(),
                })
                .collect()
        }
        None => vec![private_of(&layout.top_dict)],
    }
}
