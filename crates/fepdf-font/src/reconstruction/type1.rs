//! A Type 1 program made a CFF one: the PFB segments, the eexec decryption, and each
//! charstring converted to Type 2.

use super::{FontInfo, FontReconstructor, ReconstructedFont, Type1Data, Type1Segments};
use crate::{FontError, FontResult};
use std::collections::BTreeMap;

impl FontReconstructor {
    pub(super) fn transcode_type1_to_cff(
        data: &[u8],
        resource: &impl FontInfo,
    ) -> FontResult<ReconstructedFont> {
        let segments = Self::parse_pfb(data)?;
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

        // 3. Transcode CharStrings from T1 to T2
        let mut t2_charstrings = Vec::new();
        let mut glyph_names = Vec::new();
        for (name, t1_bytes) in t1_data.charstrings {
            let t2_bytes = Self::convert_t1_to_t2(&t1_bytes, &t1_data.subrs, t1_data.len_iv);
            t2_charstrings.push(t2_bytes);
            glyph_names.push(name);
        }

        // 4. Serialize to CFF
        let cff_data = Self::serialize_cff(&glyph_names, &t2_charstrings);

        // 5. Wrap in SFNT
        Self::wrap_naked_outline(*b"CFF ", &cff_data, resource)
    }

    pub(super) fn serialize_cff(_names: &[String], charstrings: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        // Header
        out.extend_from_slice(&[1, 0, 4, 4]); // major, minor, hdrSize, offSize=4

        // Name INDEX
        Self::push_cff_index(&mut out, &[b"TranscodedFont"]);

        // Top DICT INDEX (Pre-calculate offsets)
        // We'll build the rest first to know the offsets.
        let mut charstrings_buf = Vec::new();
        let charstring_refs: Vec<&[u8]> = charstrings.iter().map(|v| v.as_slice()).collect();
        Self::push_cff_index(&mut charstrings_buf, &charstring_refs);

        let mut private_dict = Vec::new();
        Self::push_cff_dict_entry(&mut private_dict, 20, &[0]); // defaultWidthX
        Self::push_cff_dict_entry(&mut private_dict, 21, &[0]); // nominalWidthX

        // 1st Pass: Build Top DICT with fixed-size placeholders for offsets
        let mut top_dict = Vec::new();
        Self::push_cff_dict_number_fixed(&mut top_dict, 0); // Placeholder CharStrings
        top_dict.push(17);
        Self::push_cff_dict_number_fixed(&mut top_dict, 0); // Placeholder Private size
        Self::push_cff_dict_number_fixed(&mut top_dict, 0); // Placeholder Private offset
        top_dict.push(18);

        let top_dict_size = top_dict.len();
        let top_dict_index_header_size = 2 + 1 + 1 + 4; // count(2) + offSize(1) + offset1(1) + offset2(4)

        let mut string_idx = Vec::new();
        Self::push_cff_index(&mut string_idx, &[]);
        let mut gsubr_idx = Vec::new();
        Self::push_cff_index(&mut gsubr_idx, &[]);

        let charstrings_pos = out.len()
            + top_dict_index_header_size
            + top_dict_size
            + string_idx.len()
            + gsubr_idx.len();
        let private_pos = charstrings_pos + charstrings_buf.len();

        // 2nd Pass: Build actual Top DICT using fixed-size numbers to match calculated size
        top_dict.clear();
        Self::push_cff_dict_number_fixed(&mut top_dict, charstrings_pos as i32);
        top_dict.push(17);
        Self::push_cff_dict_number_fixed(&mut top_dict, private_dict.len() as i32);
        Self::push_cff_dict_number_fixed(&mut top_dict, private_pos as i32);
        top_dict.push(18);

        Self::push_cff_index(&mut out, &[&top_dict]);
        out.extend_from_slice(&string_idx);
        out.extend_from_slice(&gsubr_idx);
        out.extend_from_slice(&charstrings_buf);
        out.extend_from_slice(&private_dict);

        out
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
                    let plain = Self::decrypt_charstring(bytes, data.len_iv);
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
        let mut len_iv = 4;

        let mut full_text = Vec::with_capacity(ascii.len() + binary.len());
        full_text.extend_from_slice(ascii);
        full_text.extend_from_slice(binary);

        if let Some(pos) = Self::find_subslice(&full_text, b"/lenIV") {
            let chunk = &full_text[pos..std::cmp::min(pos + 20, full_text.len())];
            if let Some(val) = Self::extract_number(chunk) {
                len_iv = val as usize;
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

    pub(super) fn convert_t1_to_t2(t1_bytes: &[u8], subrs: &[Vec<u8>], len_iv: usize) -> Vec<u8> {
        let mut t2_bytes = Vec::new();
        let mut stack = Vec::new();
        let mut width_written = false;

        let decrypted = Self::decrypt_charstring(t1_bytes, len_iv);

        Self::convert_recursive(
            &decrypted,
            subrs,
            len_iv,
            &mut t2_bytes,
            &mut stack,
            &mut width_written,
            0,
        );

        // Ensure it ends with endchar if not already present
        if t2_bytes.last() != Some(&14) {
            t2_bytes.push(14);
        }

        t2_bytes
    }

    pub(super) fn parse_t1_number(t1_bytes: &[u8], b: u8, i: usize) -> (i32, usize) {
        if b <= 246 {
            (i32::from(b) - 139, i + 1)
        } else if b <= 250 {
            ((i32::from(b) - 247) * 256 + i32::from(t1_bytes[i + 1]) + 108, i + 2)
        } else if b <= 254 {
            (-(i32::from(b) - 251) * 256 - i32::from(t1_bytes[i + 1]) - 108, i + 2)
        } else {
            let v = i32::from_be_bytes([
                t1_bytes[i + 1],
                t1_bytes[i + 2],
                t1_bytes[i + 3],
                t1_bytes[i + 4],
            ]);
            (v, i + 5)
        }
    }

    pub(super) fn handle_escape_sequence(
        b2: u8,
        t2_bytes: &mut Vec<u8>,
        stack: &mut Vec<i32>,
        width_written: &mut bool,
    ) {
        match b2 {
            6 => {
                if stack.len() >= 5 {
                    let adx = stack[1];
                    let ady = stack[2];
                    let bchar = stack[3];
                    let achar = stack[4];
                    Self::push_t2_number(t2_bytes, adx);
                    Self::push_t2_number(t2_bytes, ady);
                    Self::push_t2_number(t2_bytes, bchar);
                    Self::push_t2_number(t2_bytes, achar);
                    t2_bytes.push(14);
                }
                stack.clear();
            }
            7 => {
                if stack.len() >= 4 {
                    let wx = stack[2];
                    if !*width_written {
                        Self::push_t2_number(t2_bytes, wx);
                        *width_written = true;
                    }
                }
                stack.clear();
            }
            _ => {
                stack.clear();
            }
        }
    }

    pub(super) fn push_operator(t2_bytes: &mut Vec<u8>, stack: &mut Vec<i32>, op: u8) {
        for &val in stack.iter() {
            Self::push_t2_number(t2_bytes, val);
        }
        t2_bytes.push(op);
        stack.clear();
    }

    pub(super) fn handle_callsubr(
        subrs: &[Vec<u8>],
        len_iv: usize,
        t2_bytes: &mut Vec<u8>,
        stack: &mut Vec<i32>,
        width_written: &mut bool,
        depth: usize,
    ) {
        if let Some(idx) = stack.pop()
            && idx >= 0
            && (idx as usize) < subrs.len()
        {
            let decrypted = Self::decrypt_charstring(&subrs[idx as usize], len_iv);
            Self::convert_recursive(
                &decrypted,
                subrs,
                len_iv,
                t2_bytes,
                stack,
                width_written,
                depth + 1,
            );
        }
    }

    pub(super) fn handle_hsbw(
        t2_bytes: &mut Vec<u8>,
        stack: &mut Vec<i32>,
        width_written: &mut bool,
    ) {
        if stack.len() >= 2 {
            let width = stack[stack.len() - 1];
            if !*width_written {
                Self::push_t2_number(t2_bytes, width);
                *width_written = true;
            }
        }
        stack.clear();
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_operator(
        b: u8,
        subrs: &[Vec<u8>],
        len_iv: usize,
        t2_bytes: &mut Vec<u8>,
        stack: &mut Vec<i32>,
        width_written: &mut bool,
        depth: usize,
        i_ref: &mut usize,
        t1_bytes: &[u8],
    ) -> bool {
        match b {
            1 | 3 | 4 | 5 | 6 | 7 | 8 | 21 | 22 | 30 | 31 => {
                Self::push_operator(t2_bytes, stack, b);
            }
            9 => {
                stack.clear();
            }
            10 => {
                Self::handle_callsubr(subrs, len_iv, t2_bytes, stack, width_written, depth);
            }
            11 => {
                return true;
            }
            13 => {
                Self::handle_hsbw(t2_bytes, stack, width_written);
            }
            14 => {
                Self::push_operator(t2_bytes, stack, 14);
            }
            12 => {
                if *i_ref < t1_bytes.len() {
                    let b2 = t1_bytes[*i_ref];
                    *i_ref += 1;
                    Self::handle_escape_sequence(b2, t2_bytes, stack, width_written);
                }
            }
            _ => {
                stack.clear();
            }
        }
        false
    }

    pub(super) fn convert_recursive(
        t1_bytes: &[u8],
        subrs: &[Vec<u8>],
        len_iv: usize,
        t2_bytes: &mut Vec<u8>,
        stack: &mut Vec<i32>,
        width_written: &mut bool,
        depth: usize,
    ) {
        if depth > 10 {
            return;
        }

        let mut i = 0;
        while i < t1_bytes.len() {
            let b = t1_bytes[i];
            if b >= 32 {
                let (val, next_i) = Self::parse_t1_number(t1_bytes, b, i);
                stack.push(val);
                i = next_i;
            } else {
                i += 1;
                if Self::handle_operator(
                    b,
                    subrs,
                    len_iv,
                    t2_bytes,
                    stack,
                    width_written,
                    depth,
                    &mut i,
                    t1_bytes,
                ) {
                    return;
                }
            }
        }
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
