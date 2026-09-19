//! The numbers a font program states about itself.
//!
//! A `/FontDescriptor` and a `/W` array are PDF constructs assembled from these, and
//! reflowing a line needs the same advance widths, so both callers read one reader. What
//! is here is what the tables say; **scaling is not**, because glyph space is a PDF
//! notion and this crate carries none: `units_per_em` comes out and the caller divides.

use crate::reconstruction::find_table_range;

/// What `head`, `hhea`, `OS/2` and `post` state about a program.
// `Eq` is not derivable beside an `f32`, and an angle is one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgramMetrics {
    /// `head.unitsPerEm`: the grid the outlines are drawn on, 1000 or 2048 in most faces.
    pub units_per_em: u16,
    /// `head`'s bounding box over every glyph: `[x_min, y_min, x_max, y_max]`.
    pub bbox: [i16; 4],
    /// `hhea.ascender`.
    pub ascent: i16,
    /// `hhea.descender`, which is negative in a well-formed face.
    pub descent: i16,
    /// `OS/2.sCapHeight`, which only version 2 and later carry.
    pub cap_height: Option<i16>,
    /// `post.italicAngle`, in degrees counter-clockwise from vertical.
    ///
    /// `None` where the program has no `post` table — which a subset this engine produced
    /// does not, since nothing in the PDF font model consults one.
    pub italic_angle: Option<f32>,
    /// `post.isFixedPitch`: every glyph advances the same.
    pub fixed_pitch: bool,
    /// `hhea.numberOfHMetrics`: how many entries `hmtx` states before it repeats the last.
    pub num_h_metrics: u16,
}

/// What the program states, or `None` when it carries no `head` or `hhea` to state it.
#[must_use]
pub fn read_metrics(program: &[u8]) -> Option<ProgramMetrics> {
    let (head, _) = find_table_range(program, b"head")?;
    let (hhea, _) = find_table_range(program, b"hhea")?;
    let post = find_table_range(program, b"post").map(|(at, _)| at);
    let os2 = find_table_range(program, b"OS/2").map(|(at, _)| at);

    Some(ProgramMetrics {
        units_per_em: read_u16(program, head + 18)?,
        bbox: [
            read_i16(program, head + 36)?,
            read_i16(program, head + 38)?,
            read_i16(program, head + 40)?,
            read_i16(program, head + 42)?,
        ],
        ascent: read_i16(program, hhea + 4)?,
        descent: read_i16(program, hhea + 6)?,
        // Only version 2 and later of `OS/2` reach as far as `sCapHeight`, and a shorter
        // table read at that offset answers with whatever follows it.
        cap_height: os2
            .filter(|at| read_u16(program, *at).is_some_and(|version| version >= 2))
            .and_then(|at| read_i16(program, at + 88)),
        italic_angle: post.and_then(|at| read_fixed(program, at + 4)),
        fixed_pitch: post.and_then(|at| read_u32(program, at + 12)).is_some_and(|v| v != 0),
        num_h_metrics: read_u16(program, hhea + 34)?,
    })
}

/// How far `gid` advances, in the units `head.unitsPerEm` sets.
///
/// **`hmtx` stops early on purpose.** A face whose last glyphs all advance the same — a
/// CJK face, where that is most of it — states the advance once and leaves the rest to be
/// read from the final entry. A reader that indexes past the end and gives up reports a
/// zero-width glyph, which sets text on top of itself.
#[must_use]
pub fn advance_width(program: &[u8], gid: u16) -> Option<u16> {
    let (hmtx, hmtx_end) = find_table_range(program, b"hmtx")?;
    let count = read_metrics(program)?.num_h_metrics;
    if count == 0 {
        return None;
    }
    let index = usize::from(gid.min(count - 1));
    let at = hmtx.checked_add(index * 4)?;
    if at + 2 > hmtx_end {
        return None;
    }
    read_u16(program, at)
}

fn read_u16(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?]))
}

fn read_i16(data: &[u8], at: usize) -> Option<i16> {
    read_u16(data, at).map(|v| v as i16)
}

fn read_u32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *data.get(at)?,
        *data.get(at + 1)?,
        *data.get(at + 2)?,
        *data.get(at + 3)?,
    ]))
}

/// A `Fixed` 16.16, as `post.italicAngle` is written.
fn read_fixed(data: &[u8], at: usize) -> Option<f32> {
    let raw = read_u32(data, at)? as i32;
    Some(raw as f32 / 65536.0)
}

#[cfg(test)]
mod tests {
    use super::{advance_width, read_metrics};

    /// A program stating `upem`, a bounding box, an ascent and a descent, `metrics`
    /// entries of `hmtx`, and optionally a `post` and an `OS/2` of `os2_version`.
    fn program(upem: u16, metrics: &[u16], post: bool, os2_version: Option<u16>) -> Vec<u8> {
        let mut head = vec![0u8; 54];
        head[18..20].copy_from_slice(&upem.to_be_bytes());
        head[36..38].copy_from_slice(&(-11i16).to_be_bytes());
        head[38..40].copy_from_slice(&(-22i16).to_be_bytes());
        head[40..42].copy_from_slice(&33i16.to_be_bytes());
        head[42..44].copy_from_slice(&44i16.to_be_bytes());

        let mut hhea = vec![0u8; 36];
        hhea[4..6].copy_from_slice(&880i16.to_be_bytes());
        hhea[6..8].copy_from_slice(&(-120i16).to_be_bytes());
        hhea[34..36].copy_from_slice(&(metrics.len() as u16).to_be_bytes());

        let mut hmtx = Vec::new();
        for advance in metrics {
            hmtx.extend_from_slice(&advance.to_be_bytes());
            hmtx.extend_from_slice(&0i16.to_be_bytes()); // the left side bearing
        }

        let mut tables = vec![(*b"head", head), (*b"hhea", hhea), (*b"hmtx", hmtx)];
        if post {
            let mut table = vec![0u8; 32];
            table[4..8].copy_from_slice(&(-12i32 * 65536).to_be_bytes()); // -12 degrees
            table[12..16].copy_from_slice(&1u32.to_be_bytes()); // isFixedPitch
            tables.push((*b"post", table));
        }
        if let Some(version) = os2_version {
            let mut table = vec![0u8; 96];
            table[0..2].copy_from_slice(&version.to_be_bytes());
            table[88..90].copy_from_slice(&700i16.to_be_bytes()); // sCapHeight
            tables.push((*b"OS/2", table));
        }
        sfnt(&tables)
    }

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

    #[test]
    fn the_grid_and_the_box_are_read() {
        let m = read_metrics(&program(2048, &[500], true, None)).expect("head and hhea are there");
        assert_eq!(m.units_per_em, 2048);
        assert_eq!(m.bbox, [-11, -22, 33, 44]);
        assert_eq!(m.ascent, 880);
        assert_eq!(m.descent, -120, "a descent is negative and must not come back unsigned");
    }

    #[test]
    fn the_italic_angle_is_a_fixed_point_number() {
        let m = read_metrics(&program(1000, &[500], true, None)).expect("it reads");
        assert_eq!(m.italic_angle, Some(-12.0));
        assert!(m.fixed_pitch);
    }

    /// A subset this engine writes drops `post`, so the angle has to be absent rather
    /// than zero — zero is upright, which is a different claim.
    #[test]
    fn a_program_without_post_states_no_angle() {
        let m = read_metrics(&program(1000, &[500], false, None)).expect("it reads");
        assert_eq!(m.italic_angle, None);
        assert!(!m.fixed_pitch);
    }

    /// `sCapHeight` is only in version 2 and later; reading it out of version 1 reads
    /// whatever happens to follow the table.
    #[test]
    fn the_cap_height_is_read_only_where_the_table_states_one() {
        assert_eq!(
            read_metrics(&program(1000, &[500], true, Some(2))).and_then(|m| m.cap_height),
            Some(700)
        );
        assert_eq!(
            read_metrics(&program(1000, &[500], true, Some(1))).and_then(|m| m.cap_height),
            None
        );
        assert_eq!(
            read_metrics(&program(1000, &[500], true, None)).and_then(|m| m.cap_height),
            None
        );
    }

    #[test]
    fn each_glyph_advances_by_what_hmtx_states() {
        let p = program(1000, &[500, 600, 700], true, None);
        assert_eq!(advance_width(&p, 0), Some(500));
        assert_eq!(advance_width(&p, 1), Some(600));
        assert_eq!(advance_width(&p, 2), Some(700));
    }

    /// **The tail of `hmtx` is one entry standing for every glyph after it.** A CJK face
    /// states one advance for thousands of glyphs this way, and a reader that stops at
    /// the end of the array gives all of them a width of nothing.
    #[test]
    fn a_glyph_past_the_end_of_hmtx_takes_the_last_advance() {
        let p = program(1000, &[500, 1000], true, None);
        assert_eq!(advance_width(&p, 2), Some(1000));
        assert_eq!(advance_width(&p, 9999), Some(1000));
    }
}

/// Which face of a collection is the one a reader means by the family's name.
///
/// **The regular weight, upright**, chosen by what each face states rather than by its
/// position: `OS/2.usWeightClass` nearest 400 and `head.macStyle` bit 1 clear. Face 0 is
/// that face in all four of the collections this machine carries — Helvetica of six,
/// Times of four, Hiragino Mincho of four, measured 2026-09-19 — which is a convention and
/// not a rule, and a collection that listed a bold first would otherwise be set in bold
/// without a word about it.
///
/// A program that is not a collection has one face, and the answer is 0.
#[must_use]
pub fn regular_face(program: &[u8]) -> u32 {
    let count = crate::reconstruction::face_count(program);
    (0..count)
        .filter(|index| !is_italic(program, *index))
        .min_by_key(|index| weight_distance_from_regular(program, *index))
        .unwrap_or(0)
}

/// How far a face's stated weight is from 400, which is what "regular" means (Table 122
/// gives PDF the same scale).
fn weight_distance_from_regular(program: &[u8], index: u32) -> u32 {
    let Some(weight) = table_u16(program, index, b"OS/2", 4) else { return u32::MAX - 1 };
    u32::from(weight).abs_diff(400)
}

/// `head.macStyle` bit 1: the face is italic or oblique.
fn is_italic(program: &[u8], index: u32) -> bool {
    table_u16(program, index, b"head", 44).is_some_and(|style| style & 0x0002 != 0)
}

/// A `uint16` at `offset` into the table `tag` of face `index`.
fn table_u16(program: &[u8], index: u32, tag: &[u8; 4], offset: usize) -> Option<u16> {
    let base = crate::reconstruction::sfnt_base_at(program, index);
    let (start, end) = crate::reconstruction::find_table_range_at(program, tag, base)?;
    if start + offset + 2 > end {
        return None;
    }
    read_u16(program, start + offset)
}

#[cfg(test)]
mod regular_face_tests {
    use super::regular_face;

    /// A face stating `weight` and, where `italic`, the `head` bit that says so.
    fn face(weight: u16, italic: bool) -> Vec<([u8; 4], Vec<u8>)> {
        let mut os2 = vec![0u8; 78];
        os2[4..6].copy_from_slice(&weight.to_be_bytes());
        let mut head = vec![0u8; 54];
        head[44..46].copy_from_slice(&(if italic { 2u16 } else { 0 }).to_be_bytes());
        vec![(*b"OS/2", os2), (*b"head", head)]
    }

    /// A collection of `faces`, each laid out as its own table directory.
    fn collection(faces: &[Vec<([u8; 4], Vec<u8>)>]) -> Vec<u8> {
        let header = 12 + faces.len() * 4;
        let mut directories = Vec::new();
        let mut bodies = Vec::new();
        let mut offsets = Vec::new();
        for tables in faces {
            offsets.push(header + directories.len() + bodies.len());
            // Reserve the directory, then lay the tables out after every directory.
            directories.extend(std::iter::repeat_n(0u8, 12 + tables.len() * 16));
        }
        let mut at = header + directories.len();
        directories.clear();
        for (i, tables) in faces.iter().enumerate() {
            offsets[i] = header + directories.len();
            let mut directory = Vec::new();
            directory.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
            directory.extend_from_slice(&u16::try_from(tables.len()).unwrap_or(0).to_be_bytes());
            directory.extend_from_slice(&[0; 6]);
            for (tag, data) in tables {
                directory.extend_from_slice(tag);
                directory.extend_from_slice(&[0; 4]);
                directory.extend_from_slice(&u32::try_from(at).unwrap_or(0).to_be_bytes());
                directory.extend_from_slice(&u32::try_from(data.len()).unwrap_or(0).to_be_bytes());
                at += data.len();
                bodies.extend_from_slice(data);
            }
            directories.extend_from_slice(&directory);
        }

        let mut out = Vec::new();
        out.extend_from_slice(b"ttcf");
        out.extend_from_slice(&0x0002_0000_u32.to_be_bytes());
        out.extend_from_slice(&u32::try_from(faces.len()).unwrap_or(0).to_be_bytes());
        for offset in &offsets {
            out.extend_from_slice(&u32::try_from(*offset).unwrap_or(0).to_be_bytes());
        }
        out.extend_from_slice(&directories);
        out.extend_from_slice(&bodies);
        out
    }

    /// **The face taken is the one that says it is regular, not the one that is first.**
    #[test]
    fn a_collection_listing_bold_first_still_yields_the_regular_face() {
        let program = collection(&[face(700, false), face(400, false)]);
        assert_eq!(regular_face(&program), 1);
    }

    /// An italic of the right weight is not the regular face either.
    #[test]
    fn an_italic_is_passed_over() {
        let program = collection(&[face(400, true), face(400, false)]);
        assert_eq!(regular_face(&program), 1);
    }

    #[test]
    fn a_collection_listing_the_regular_first_yields_it() {
        let program = collection(&[face(400, false), face(700, false)]);
        assert_eq!(regular_face(&program), 0);
    }

    /// A program that is not a collection has one face, whatever it weighs.
    #[test]
    fn a_single_face_is_the_answer_however_it_is_drawn() {
        let mut plain = vec![0u8; 20];
        plain[0..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
        assert_eq!(regular_face(&plain), 0);
    }
}
