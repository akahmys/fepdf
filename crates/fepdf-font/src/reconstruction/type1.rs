//! A Type 1 program made a CFF one: the PFB segments, the eexec decryption, and each
//! charstring converted to Type 2.

use super::type1_charstring::{Type1Program, convert_glyph};
use super::{FontInfo, FontReconstructor, ReconstructedFont, Type1Data, Type1Segments};
use crate::{FontError, FontResult};
use std::collections::BTreeMap;

impl FontReconstructor {
    pub(super) fn transcode_type1_to_cff(
        data: &[u8],
        resource: &impl FontInfo,
    ) -> FontResult<ReconstructedFont> {
        let segments = if data.first() == Some(&0x80) {
            Self::parse_pfb(data)?
        } else {
            Self::split_cleartext(data, resource.type1_cleartext_length())?
        };
        log::info!(
            "[RECONSTRUCT] Type 1 segments extracted for {}: ASCII={} bytes, Binary={} bytes, Trailer={} bytes",
            resource.base_font(),
            segments.ascii.len(),
            segments.binary.len(),
            segments.trailer.len()
        );

        // 1. Decrypt the eexec segment
        let decrypted_eexec = Self::decrypt_type1(&segments.binary, 55665, 4);

        // 2. Parse Type 1 Data
        let t1_data = Self::parse_type1_data(&segments.ascii, &decrypted_eexec)?;

        // 3. Run each charstring, and write its outline as Type 2
        let program = Type1Program {
            charstrings: &t1_data.charstrings,
            subrs: &t1_data.subrs,
            len_iv: t1_data.len_iv,
        };
        let glyphs: Vec<(String, Vec<u8>)> = t1_data
            .charstrings
            .iter()
            .map(|(name, t1_bytes)| (name.clone(), convert_glyph(t1_bytes, &program)))
            .collect();

        // 4. Serialize to CFF
        let font_name = resource.base_font().rsplit('+').next().unwrap_or("Type1");
        let cff_data = Self::serialize_cff(font_name, &glyphs);

        // 5. Wrap in SFNT
        Self::wrap_naked_outline(*b"CFF ", &cff_data, resource)
    }

    /// The glyphs as a CFF program (Adobe Technical Note 5176): `.notdef` first, as 5176
    /// requires of glyph 0, then the rest in name order, each named in a format 0 charset.
    ///
    /// **The charset is what makes the program usable.** A glyph is found by name from
    /// here on (`inspect_cff` reads `name_to_gid` out of the charset), so a program without
    /// one gives every glyph the predefined ISOAdobe charset's name for its index.
    pub(super) fn serialize_cff(font_name: &str, glyphs: &[(String, Vec<u8>)]) -> Vec<u8> {
        let notdef = [(String::from(".notdef"), vec![14u8])];
        let has_notdef = glyphs.iter().any(|(name, _)| name == ".notdef");
        let ordered: Vec<&(String, Vec<u8>)> = glyphs
            .iter()
            .filter(|(name, _)| name == ".notdef")
            .chain(notdef.iter().filter(|_| !has_notdef))
            .chain(glyphs.iter().filter(|(name, _)| name != ".notdef"))
            .collect();
        let (charset, custom) = Self::cff_charset(ordered.iter().skip(1).map(|(name, _)| name));

        let mut name_index = Vec::new();
        Self::push_cff_index(&mut name_index, &[font_name.as_bytes()]);
        let mut string_index = Vec::new();
        Self::push_cff_index(&mut string_index, &custom);
        let mut gsubr_index = Vec::new();
        Self::push_cff_index(&mut gsubr_index, &[]);
        let mut charstrings_index = Vec::new();
        let charstrings: Vec<&[u8]> = ordered.iter().map(|(_, cs)| cs.as_slice()).collect();
        Self::push_cff_index(&mut charstrings_index, &charstrings);
        let mut private_dict = Vec::new();
        Self::push_cff_dict_entry(&mut private_dict, 20, &[0]); // defaultWidthX
        Self::push_cff_dict_entry(&mut private_dict, 21, &[0]); // nominalWidthX

        let top_dict = |at: [usize; 3]| Self::cff_top_dict(at, private_dict.len());
        let mut top_index = Vec::new();
        Self::push_cff_index(&mut top_index, &[&top_dict([0, 0, 0])]);

        let header = [1u8, 0, 4, 4];
        let charset_at = header.len()
            + name_index.len()
            + top_index.len()
            + string_index.len()
            + gsubr_index.len();
        let charstrings_at = charset_at + charset.len();
        let private_at = charstrings_at + charstrings_index.len();
        top_index.clear();
        Self::push_cff_index(
            &mut top_index,
            &[&top_dict([charset_at, charstrings_at, private_at])],
        );

        [
            &header[..],
            &name_index,
            &top_index,
            &string_index,
            &gsubr_index,
            &charset,
            &charstrings_index,
            &private_dict,
        ]
        .concat()
    }

    /// The Top DICT: `charset`, `CharStrings` and `Private` at the offsets `at` gives in
    /// that order. Every offset is a five-byte integer, so the DICT is the same length
    /// whatever they are, and its INDEX can be measured before they are known.
    fn cff_top_dict(at: [usize; 3], private_len: usize) -> Vec<u8> {
        let fixed = |dict: &mut Vec<u8>, value: usize| {
            Self::push_cff_dict_number_fixed(dict, i32::try_from(value).unwrap_or(0));
        };
        let mut dict = Vec::new();
        fixed(&mut dict, at[0]);
        dict.push(15);
        fixed(&mut dict, at[1]);
        dict.push(17);
        fixed(&mut dict, private_len);
        fixed(&mut dict, at[2]);
        dict.push(18);
        dict
    }

    /// A format 0 charset naming every glyph after `.notdef`, and the String INDEX entries
    /// it needs: a name among the 391 standard strings is its SID, and any other is
    /// numbered on from 391 in the order it is first met.
    fn cff_charset<'a>(names: impl Iterator<Item = &'a String>) -> (Vec<u8>, Vec<&'a [u8]>) {
        let standard = crate::cff_standard::CFF_STANDARD_STRINGS;
        let mut custom: Vec<&[u8]> = Vec::new();
        let mut charset = vec![0u8];
        for name in names {
            let sid = standard.iter().position(|s| s == name).unwrap_or_else(|| {
                custom.push(name.as_bytes());
                standard.len() + custom.len() - 1
            });
            charset.extend_from_slice(&u16::try_from(sid).unwrap_or(u16::MAX).to_be_bytes());
        }
        (charset, custom)
    }

    pub(crate) fn push_cff_index(out: &mut Vec<u8>, entries: &[&[u8]]) {
        let count = entries.len() as u16;
        out.extend_from_slice(&count.to_be_bytes());
        if count == 0 {
            return;
        }

        out.push(4); // offSize
        let mut offset = 1u32;
        out.extend_from_slice(&offset.to_be_bytes());
        for entry in entries {
            offset += entry.len() as u32;
            out.extend_from_slice(&offset.to_be_bytes());
        }
        for entry in entries {
            out.extend_from_slice(entry);
        }
    }

    pub(super) fn push_cff_dict_entry(out: &mut Vec<u8>, op: u8, args: &[i32]) {
        for &arg in args {
            Self::push_cff_dict_number(out, arg);
        }
        out.push(op);
    }

    /// The one-and-two-byte integer forms a DICT and a charstring encode alike.
    ///
    /// Returns `false` when `val` falls outside them, which is where the two formats
    /// part: a DICT continues with 28/`i16` or 29/`i32`, a charstring has only 28/`i16`
    /// because its 255 introduces a 16.16 fixed-point value rather than an integer.
    pub(super) fn push_cff_small_number(out: &mut Vec<u8>, val: i32) -> bool {
        if (-107..=107).contains(&val) {
            out.push((val + 139) as u8);
        } else if (108..=1131).contains(&val) {
            let v = val - 108;
            out.push((v / 256 + 247) as u8);
            out.push((v % 256) as u8);
        } else if (-1131..=-108).contains(&val) {
            let v = -val - 108;
            out.push((v / 256 + 251) as u8);
            out.push((v % 256) as u8);
        } else {
            return false;
        }
        true
    }

    pub(super) fn push_cff_dict_number(out: &mut Vec<u8>, val: i32) {
        if Self::push_cff_small_number(out, val) {
            return;
        }
        if let Ok(narrow) = i16::try_from(val) {
            out.push(28);
            out.extend_from_slice(&narrow.to_be_bytes());
        } else {
            out.push(29);
            out.extend_from_slice(&val.to_be_bytes());
        }
    }

    pub(super) fn push_cff_dict_number_fixed(out: &mut Vec<u8>, val: i32) {
        out.push(29);
        out.extend_from_slice(&val.to_be_bytes());
    }

    pub(super) fn parse_subrs(full_text: &[u8], pos: usize, subrs: &mut Vec<Vec<u8>>) {
        let mut search_pos = pos;
        while let Some(dup_pos) = Self::find_subslice(&full_text[search_pos..], b"dup") {
            let current_dup = search_pos + dup_pos;
            let chunk = &full_text[current_dup..std::cmp::min(current_dup + 50, full_text.len())];
            let chunk_str = String::from_utf8_lossy(chunk);
            let parts: Vec<&str> = chunk_str.split_whitespace().collect();
            if parts.len() >= 3
                && parts[0] == "dup"
                && let Ok(index) = parts[1].parse::<usize>()
                && let Some((data, next_pos)) =
                    Self::extract_rd_data(full_text, current_dup + 4 + parts[1].len())
            {
                if index >= subrs.len() {
                    subrs.resize(index + 1, Vec::new());
                }
                subrs[index] = data;
                search_pos = next_pos;
                continue;
            }
            search_pos = current_dup + 3;
            if search_pos >= full_text.len()
                || &full_text[search_pos..std::cmp::min(search_pos + 3, full_text.len())] == b"def"
            {
                break;
            }
        }
    }

    pub(super) fn parse_charstrings(
        full_text: &[u8],
        pos: usize,
        charstrings: &mut BTreeMap<String, Vec<u8>>,
    ) {
        let mut search_pos = pos;
        while let Some(name_pos) = Self::find_next_name(full_text, search_pos) {
            let name = Self::extract_name(full_text, name_pos);
            if name == "CharStrings" || name == "dict" || name == "begin" || name == "end" {
                search_pos = name_pos + name.len() + 1;
                continue;
            }

            // `name_pos` is the slash, so the name ends one byte further on than its length.
            // Reading from `name_pos + name.len()` took the name's last character for the
            // charstring's length, failed to parse it, and so read no charstring at all.
            if let Some((data, next_pos)) =
                Self::extract_rd_data(full_text, name_pos + 1 + name.len())
            {
                charstrings.insert(name, data);
                search_pos = next_pos;
            } else {
                search_pos = name_pos + name.len() + 1;
            }

            if search_pos >= full_text.len()
                || &full_text[search_pos..std::cmp::min(search_pos + 3, full_text.len())] == b"end"
            {
                break;
            }
        }
    }

    /// Each glyph's advance width in character space, as the `hsbw` or `sbw` its charstring
    /// opens with states it; a glyph whose charstring opens otherwise is left out.
    pub(crate) fn type1_advances(ascii: &[u8], encrypted: &[u8]) -> Option<BTreeMap<String, f64>> {
        let decrypted = Self::decrypt_type1(encrypted, 55665, 4);
        let data = Self::parse_type1_data(ascii, &decrypted).ok()?;
        Some(
            data.charstrings
                .iter()
                .filter_map(|(name, bytes)| {
                    let plain = match data.len_iv {
                        Some(n) => Self::decrypt_charstring(bytes, n),
                        None => bytes.clone(),
                    };
                    Some((name.clone(), f64::from(Self::opening_advance(&plain)?)))
                })
                .collect(),
        )
    }

    /// The advance a Type 1 charstring's first operator states: `hsbw`'s second operand,
    /// or `sbw`'s third. Numbers are read with their bounds checked.
    pub(super) fn opening_advance(charstring: &[u8]) -> Option<i32> {
        let mut stack: Vec<i32> = Vec::new();
        let mut at = 0;
        while let Some(&byte) = charstring.get(at) {
            let next = |k: usize| charstring.get(at + k).copied().map(i32::from);
            let (value, width) = match byte {
                32..=246 => (i32::from(byte) - 139, 1),
                247..=250 => ((i32::from(byte) - 247) * 256 + next(1)? + 108, 2),
                251..=254 => (-(i32::from(byte) - 251) * 256 - next(1)? - 108, 2),
                255 => {
                    let b = charstring.get(at + 1..at + 5)?;
                    (i32::from_be_bytes([b[0], b[1], b[2], b[3]]), 5)
                }
                13 => return stack.get(1).copied(),
                12 if charstring.get(at + 1) == Some(&7) => return stack.get(2).copied(),
                _ => return None,
            };
            stack.push(value);
            at += width;
        }
        None
    }

    /// The names a Type 1 program's `/CharStrings` defines, from its cleartext portion and
    /// its eexec-encrypted portion as bytes.
    pub(crate) fn type1_charstring_names(
        ascii: &[u8],
        encrypted: &[u8],
    ) -> Option<std::collections::BTreeSet<String>> {
        let decrypted = Self::decrypt_type1(encrypted, 55665, 4);
        let data = Self::parse_type1_data(ascii, &decrypted).ok()?;
        Some(data.charstrings.into_keys().collect())
    }

    pub(super) fn parse_type1_data(ascii: &[u8], binary: &[u8]) -> FontResult<Type1Data> {
        let mut charstrings = BTreeMap::new();
        let mut subrs = Vec::new();
        // `/lenIV` is 4 where the Private dictionary does not say; a negative one means
        // the charstrings are not encrypted at all.
        let mut len_iv = Some(4);

        let mut full_text = Vec::with_capacity(ascii.len() + binary.len());
        full_text.extend_from_slice(ascii);
        full_text.extend_from_slice(binary);

        if let Some(pos) = Self::find_subslice(&full_text, b"/lenIV") {
            let chunk = &full_text[pos..std::cmp::min(pos + 20, full_text.len())];
            if let Some(val) = Self::extract_number(chunk) {
                len_iv = usize::try_from(val).ok();
            }
        }

        if let Some(pos) = Self::find_subslice(&full_text, b"/Subrs") {
            Self::parse_subrs(&full_text, pos, &mut subrs);
        }

        if let Some(pos) = Self::find_subslice(&full_text, b"/CharStrings") {
            Self::parse_charstrings(&full_text, pos, &mut charstrings);
        }

        Ok(Type1Data { charstrings, subrs, len_iv })
    }

    /// Writes `val` as a Type 2 charstring operand.
    ///
    /// **28 is the only integer form a Type 2 charstring has.** This wrote 255 followed
    /// by a big-endian `i32`, which is Type 1\'s convention; in Type 2, 255 introduces a
    /// 16.16 fixed-point value, so every operand outside ±1131 was read back at 1/65536
    /// of what was meant. The 28 form was never emitted at all.
    ///
    /// An operand beyond `i16` is not representable in Type 2 by either form — 16.16
    /// fixed holds an `i16` integer part — so it saturates. A Type 1 charstring can
    /// state one; a glyph outline in any realistic em square does not.
    pub(super) fn push_t2_number(out: &mut Vec<u8>, val: i32) {
        if Self::push_cff_small_number(out, val) {
            return;
        }
        out.push(28);
        let narrow = val.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        out.extend_from_slice(&narrow.to_be_bytes());
    }

    pub(super) fn decrypt_charstring(data: &[u8], len_iv: usize) -> Vec<u8> {
        Self::decrypt_type1(data, 4330, len_iv)
    }

    pub(super) fn find_subslice(data: &[u8], sub: &[u8]) -> Option<usize> {
        data.windows(sub.len()).position(|w| w == sub)
    }

    pub(super) fn extract_number(data: &[u8]) -> Option<i32> {
        let s = String::from_utf8_lossy(data);
        s.split_whitespace().find_map(|p| p.parse::<i32>().ok())
    }

    pub(super) fn find_next_name(data: &[u8], start: usize) -> Option<usize> {
        data[start..].iter().position(|&b| b == b'/').map(|p| start + p)
    }

    pub(super) fn extract_name(data: &[u8], pos: usize) -> String {
        let mut end = pos + 1;
        while end < data.len()
            && !data[end].is_ascii_whitespace()
            && data[end] != b'/'
            && data[end] != b'{'
            && data[end] != b'['
        {
            end += 1;
        }
        String::from_utf8_lossy(&data[pos + 1..end]).to_string()
    }

    pub(super) fn extract_rd_data(data: &[u8], pos: usize) -> Option<(Vec<u8>, usize)> {
        // Look for "<number> RD" or "<number> -|"
        let mut i = pos;
        while i < data.len() && data[i].is_ascii_whitespace() {
            i += 1;
        }
        let start_num = i;
        while i < data.len() && !data[i].is_ascii_whitespace() {
            i += 1;
        }
        let num_str = String::from_utf8_lossy(&data[start_num..i]);
        let len = num_str.parse::<usize>().ok()?;

        while i < data.len() && data[i].is_ascii_whitespace() {
            i += 1;
        }
        let op_start = i;
        while i < data.len() && !data[i].is_ascii_whitespace() {
            i += 1;
        }
        let op = &data[op_start..i];

        if op == b"RD" || op == b"-|" {
            let data_start = i + 1; // Usually a space after RD
            if data_start < data.len() && data_start + len <= data.len() {
                return Some((data[data_start..data_start + len].to_vec(), data_start + len));
            }
        }
        None
    }

    pub(super) fn decrypt_type1(data: &[u8], mut r: u16, n: usize) -> Vec<u8> {
        if data.len() <= n {
            return Vec::new();
        }
        let mut output = Vec::with_capacity(data.len() - n);
        let c1: u16 = 52845;
        let c2: u16 = 22719;

        for (i, &b) in data.iter().enumerate() {
            let plain = b ^ (r >> 8) as u8;
            if i >= n {
                output.push(plain);
            }
            r = u16::from(b).wrapping_add(r).wrapping_mul(c1).wrapping_add(c2);
        }
        output
    }

    /// A program as `/FontFile` holds it (9.9): the clear text, then the eexec-encrypted
    /// portion in binary or hexadecimal, then the trailer, with no PFB segment headers.
    ///
    /// `/Length1` says where the clear text ends. Where it is missing or past the end,
    /// the clear text ends after `eexec` and the white space that follows it, which is
    /// where the Type 1 specification puts the start of the encrypted portion.
    pub(super) fn split_cleartext(
        data: &[u8],
        length1: Option<usize>,
    ) -> FontResult<Type1Segments> {
        let after_eexec = || {
            let at = Self::find_subslice(data, b"eexec")? + b"eexec".len();
            let skipped = data.get(at..)?.iter().take_while(|b| b.is_ascii_whitespace()).count();
            Some(at + skipped)
        };
        let split = length1
            .filter(|&n| n > 0 && n < data.len())
            .or_else(after_eexec)
            .ok_or_else(|| FontError::Other("Type 1 program has no eexec portion".into()))?;
        let (ascii, rest) = data.split_at(split);
        Ok(Type1Segments {
            ascii: ascii.to_vec(),
            binary: crate::program_glyphs::eexec_portion(rest),
            trailer: Vec::new(),
        })
    }

    pub(super) fn parse_pfb(data: &[u8]) -> FontResult<Type1Segments> {
        let mut ascii = Vec::new();
        let mut binary = Vec::new();
        let mut trailer = Vec::new();
        let mut pos = 0;

        while pos + 6 <= data.len() {
            if data[pos] != 0x80 {
                break;
            }
            let tag = data[pos + 1];
            let len =
                u32::from_le_bytes([data[pos + 2], data[pos + 3], data[pos + 4], data[pos + 5]])
                    as usize;
            pos += 6;

            if pos + len > data.len() {
                return Err(FontError::Other("Malformed PFB: segment exceeds data length".into()));
            }

            match tag {
                1 => ascii.extend_from_slice(&data[pos..pos + len]),
                2 => binary.extend_from_slice(&data[pos..pos + len]),
                3 => trailer.extend_from_slice(&data[pos..pos + len]),
                _ => {}
            }
            pos += len;
        }

        if ascii.is_empty() && binary.is_empty() {
            return Err(FontError::Other("Malformed PFB: no valid segments found".into()));
        }

        Ok(Type1Segments { ascii, binary, trailer })
    }
}
