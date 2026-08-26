//! CMap (Character Map) Parser (ISO 32000-2 Clause 9.7)

use crate::PdfResult;
use std::collections::BTreeMap;

use std::sync::Arc;

#[derive(Debug, Clone, Default)]
/// A parsed CMap: character codes to CIDs or to Unicode.
pub struct CMap {
    /// The CMap's own name, from `/CMapName` or the predefined name it was loaded by.
    pub name: String,
    /// Writing mode: 0 horizontal, 1 vertical.
    pub wmode: i32,
    /// Byte ranges defining how many bytes each code occupies.
    pub codespace_ranges: Vec<(Vec<u8>, Vec<u8>)>,
    /// Direct code-to-text mappings (`bfchar`).
    pub mappings: Arc<BTreeMap<Vec<u8>, String>>,
    /// Direct code-to-CID mappings (`cidchar`).
    pub mappings_cid: Arc<BTreeMap<Vec<u8>, u32>>,
    /// Contiguous code-to-CID ranges (`cidrange`).
    pub cid_ranges: Vec<CMapRange<u32>>,
    /// Contiguous code-to-Unicode ranges (`bfrange`), keyed by starting scalar.
    pub bf_ranges: Vec<CMapRange<u32>>, // Maps to start Unicode scalar
}

#[derive(Debug, Clone)]
/// A contiguous run of codes mapping onto consecutive targets.
pub struct CMapRange<T> {
    /// First code in the run.
    pub start: u32,
    /// Last code in the run, inclusive.
    pub end: u32,
    /// Target the first code maps to; later codes follow consecutively.
    pub base: T,
    /// Byte width of the codes in this run.
    pub len: usize,
}

impl CMap {
    /// Parses a CMap program.
    pub fn parse(data: &[u8]) -> PdfResult<Self> {
        Self::parse_with_depth(data, 0)
    }

    fn parse_with_depth(data: &[u8], depth: usize) -> PdfResult<Self> {
        if depth > 4 {
            return Ok(Self::default());
        }
        let mut cmap = Self::default();
        let tokens = tokenize_cmap(data);

        // Use local maps during parsing to avoid redundant Arc clones (Rule 15)
        let mut mappings = BTreeMap::new();
        let mut mappings_cid = BTreeMap::new();

        let mut i = 0;
        let mut safety = 0;
        while i < tokens.len() {
            safety += 1;
            if safety > 1_000_000 {
                break;
            }
            i = handle_cmap_token(&mut cmap, &mut mappings, &mut mappings_cid, &tokens, i, depth);
        }

        cmap.mappings = Arc::new(mappings);
        cmap.mappings_cid = Arc::new(mappings_cid);

        Ok(cmap)
    }

    /// Maps a character code to the text it represents.
    pub fn map(&self, code: &[u8]) -> Option<String> {
        if let Some(s) = self.mappings.get(code) {
            return Some(s.clone());
        }
        let val = vec_to_u32(code);
        for range in &self.bf_ranges {
            if code.len() == range.len && val >= range.start && val <= range.end {
                let uni_val = range.base + (val - range.start);
                return std::char::from_u32(uni_val).map(|c| c.to_string());
            }
        }
        None
    }

    /// Maps a character code to a CID, returning 0 when unmapped.
    pub fn to_cid(&self, code: &[u8]) -> u32 {
        if let Some(&cid) = self.mappings_cid.get(code) {
            return cid;
        }
        let val = vec_to_u32(code);
        for range in &self.cid_ranges {
            if code.len() == range.len && val >= range.start && val <= range.end {
                return range.base + (val - range.start);
            }
        }

        if self.name.starts_with("Identity") {
            if code.len() == 2 {
                return (u32::from(code[0]) << 8) | u32::from(code[1]);
            } else if code.len() == 1 {
                return u32::from(code[0]);
            }
        }
        if let Some(s) = self.map(code) {
            if let Some(cid_str) = s.strip_prefix("CID:") {
                return cid_str.parse().unwrap_or(0);
            }
            if s.len() == 1 {
                return s.chars().next().map(|c| c as u32).unwrap_or(0);
            }
        }
        if code.len() == 2 {
            return (u32::from(code[0]) << 8) | u32::from(code[1]);
        }
        u32::from(code.first().copied().unwrap_or(0))
    }

    /// Decodes the next code, returning its byte length and the text it maps to.
    pub fn decode_next(&self, data: &[u8]) -> (usize, Option<String>) {
        if data.is_empty() {
            return (0, None);
        }
        for (start, end) in &self.codespace_ranges {
            let len = start.len();
            if data.len() >= len {
                let segment = &data[0..len];
                if segment >= start.as_slice() && segment <= end.as_slice() {
                    return (len, self.map(segment));
                }
            }
        }
        (1, self.map(&data[0..1]))
    }

    /// The CMap's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether any codespace range is wider than one byte.
    pub fn is_multibyte(&self) -> bool {
        self.name.contains("Identity")
            || self.name.contains("JIS")
            || self.name.contains("RKSJ")
            || self.name.contains("EUC")
            || self.name.contains("GB-")
            || self.name.contains("KSC-")
            || self.name.contains("UTF-16")
            || self.codespace_ranges.iter().any(|(s, _)| s.len() >= 2)
    }

    /// Like [`CMap::decode_next`], but refuses codes outside a codespace range.
    pub fn decode_next_strict(&self, data: &[u8]) -> Option<(usize, Option<String>)> {
        self.decode_next_with_min_len(data, None)
    }

    /// Decodes the next code, never consuming fewer than `min_len` bytes.
    pub fn decode_next_with_min_len(
        &self,
        data: &[u8],
        min_len: Option<usize>,
    ) -> Option<(usize, Option<String>)> {
        if data.is_empty() {
            return None;
        }

        let target_len = min_len.unwrap_or(1);

        // 1. Try codespace ranges first to determine valid length
        if !self.codespace_ranges.is_empty() {
            for (start, end) in &self.codespace_ranges {
                let len = start.len();
                if len < target_len {
                    continue;
                }
                if data.len() >= len {
                    let segment = &data[0..len];
                    if segment >= start.as_slice() && segment <= end.as_slice() {
                        // Found a valid codespace range. Now look for a mapping.
                        if let Some(m) = self.map(segment) {
                            return Some((len, Some(m)));
                        }
                    }
                }
            }
        }

        // 2. Fallback to direct mapping search if no ranges defined or found
        // Start from longest possible (up to 4) down to target_min_len
        for len in (target_len..=4).rev() {
            if data.len() >= len {
                let segment = &data[0..len];
                if let Some(m) = self.map(segment) {
                    return Some((len, Some(m)));
                }
            }
        }

        // 3. Last resort: if we have a target_len and it's 2, but we didn't find anything,
        // we should probably still consume target_len bytes and return None to indicate unmapped.
        if let Some(tl) = min_len
            && data.len() >= tl
        {
            return Some((tl, None));
        }

        None
    }

    /// The predefined `Identity-H` CMap: two-byte codes used directly as CIDs.
    pub fn identity_h() -> Self {
        Self {
            name: "Identity-H".into(),
            codespace_ranges: vec![(vec![0, 0], vec![0xFF, 0xFF])],
            ..Default::default()
        }
    }

    /// The predefined `Identity-V` CMap: `Identity-H` in vertical writing mode.
    pub fn identity_v() -> Self {
        Self {
            name: "Identity-V".into(),
            wmode: 1,
            codespace_ranges: vec![(vec![0, 0], vec![0xFF, 0xFF])],
            ..Default::default()
        }
    }

    /// The predefined `90ms-RKSJ-H` CMap for Shift-JIS text.
    pub fn rksj_h() -> Self {
        Self { name: "90ms-RKSJ-H".into(), ..Default::default() }
    }

    /// The predefined `UniJIS-UCS2-H` CMap for UCS-2 Japanese text.
    pub fn unijis_h() -> Self {
        // Simple placeholder for UniJIS-UTF16-H
        Self { name: "UniJIS-UTF16-H".into(), ..Default::default() }
    }

    /// The `Adobe-Japan1-UCS2` CMap, mapping Adobe-Japan1 CIDs to Unicode.
    pub fn adobe_japan1_ucs2() -> Self {
        static CACHE: std::sync::OnceLock<std::collections::BTreeMap<Vec<u8>, String>> =
            std::sync::OnceLock::new();

        let mappings = CACHE.get_or_init(|| {
            let mut map = std::collections::BTreeMap::new();
            // Try to load from cid2code.txt in the resources directory
            let Some(cmaps) = crate::resources::locate(crate::resources::Resource::Cmaps) else {
                // Nothing to read. The empty map is returned, and the caller that wanted
                // a Japanese mapping records why it did not get one — see
                // `resources::not_found_message`. Returning silently is what left 192
                // pages of `bokutokitan.pdf` indistinguishable from blank ones.
                return map;
            };
            let cid2code_path = cmaps.join("Adobe-Japan1-7/cid2code.txt");

            if let Ok(content) = std::fs::read_to_string(&cid2code_path) {
                for line in content.lines() {
                    if line.starts_with('#') || line.is_empty() || line.starts_with("CID") {
                        continue;
                    }
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 24 {
                        let cid_str = parts[0];
                        let ucs2_col = [parts[17], parts[20], parts[23]]
                            .iter()
                            .find(|&&c| c != "*" && !c.is_empty())
                            .copied()
                            .unwrap_or("*");

                        if ucs2_col != "*"
                            && let Ok(cid) = cid_str.parse::<u32>()
                        {
                            let hex = ucs2_col.split(',').next().unwrap_or(ucs2_col);
                            if let Ok(val) = u32::from_str_radix(hex, 16)
                                && let Some(c) = std::char::from_u32(val)
                            {
                                let cid_bytes = if cid <= 0xFFFF {
                                    vec![(cid >> 8) as u8, (cid & 0xFF) as u8]
                                } else {
                                    cid.to_be_bytes().to_vec()
                                };
                                map.insert(cid_bytes, c.to_string());
                            }
                        }
                    }
                }
            }

            // No fallback: return empty map. Let to_unicode_inner handle PUA.
            map
        });

        Self {
            name: "Adobe-Japan1-UCS2".into(),
            mappings: Arc::new(mappings.clone()),
            ..Default::default()
        }
    }

    /// Loads a predefined CMap by name, from the bundled Adobe resources.
    pub fn load_named(name: &str) -> Option<Self> {
        Self::load_named_recursive(name, 0)
    }

    fn load_named_recursive(name: &str, depth: usize) -> Option<Self> {
        if depth > 4 {
            return None;
        } // Recursion guard

        // 1. Check programmatic presets (where no file exists)
        if let Some(cmap) = match name {
            "Identity-H" => Some(Self::identity_h()),
            "Identity-V" => Some(Self::identity_v()),
            "Adobe-Japan1-UCS2" => Some(Self::adobe_japan1_ucs2()),
            _ => None,
        } {
            return Some(cmap);
        }

        // 2. Dynamic loading from synced cmap-resources repository
        let resource_dir = crate::resources::locate(crate::resources::Resource::Cmaps)?;

        let mut search_paths = vec![resource_dir.clone()];
        search_paths.push(resource_dir.join("Adobe-Japan1-7/CMap"));
        search_paths.push(resource_dir.join("Adobe-Japan1-6/CMap"));
        search_paths.push(resource_dir.join("Adobe-Japan1-4/CMap"));

        for base in search_paths {
            let file_path = base.join(name);
            if let Ok(data) = std::fs::read(&file_path)
                && let Ok(mut cmap) = Self::parse_recursive(&data, depth)
            {
                cmap.name = name.to_string();
                return Some(cmap);
            }
        }
        None
    }

    fn parse_recursive(data: &[u8], depth: usize) -> PdfResult<Self> {
        let mut cmap = Self::default();
        let tokens = tokenize_cmap(data);
        let mut i = 0;
        while i < tokens.len() {
            let token = tokens[i];
            if token == b"usecmap" && i > 0 {
                let parent_name =
                    String::from_utf8_lossy(tokens[i - 1]).trim_start_matches('/').to_string();
                if let Some(parent_cmap) = Self::load_named_recursive(&parent_name, depth + 1) {
                    let mut new_mappings = (*cmap.mappings).clone();
                    for (k, v) in parent_cmap.mappings.iter() {
                        new_mappings.insert(k.clone(), v.clone());
                    }
                    cmap.mappings = Arc::new(new_mappings);
                }
            }
            i += 1;
        }
        Ok(cmap)
    }
}

fn handle_cmap_token(
    cmap: &mut CMap,
    mappings: &mut BTreeMap<Vec<u8>, String>,
    mappings_cid: &mut BTreeMap<Vec<u8>, u32>,
    tokens: &[&[u8]],
    i: usize,
    depth: usize,
) -> usize {
    match tokens[i] {
        b"begincodespacerange" => handle_codespacerange(cmap, tokens, i),
        b"beginbfchar" => handle_bfchar(cmap, mappings, tokens, i),
        b"beginbfrange" => handle_bfrange(cmap, mappings, tokens, i),
        b"begincidchar" => handle_cidchar(mappings_cid, tokens, i),
        b"begincidrange" => handle_cidrange(cmap, mappings_cid, tokens, i),
        b"/CMapName" => handle_cmap_name(cmap, tokens, i),
        b"/WMode" => handle_wmode(cmap, tokens, i),
        b"usecmap" => {
            handle_usecmap(cmap, mappings, mappings_cid, tokens, i, depth);
            i + 1
        }
        _ => i + 1,
    }
}

fn handle_usecmap(
    cmap: &mut CMap,
    mappings: &mut BTreeMap<Vec<u8>, String>,
    mappings_cid: &mut BTreeMap<Vec<u8>, u32>,
    tokens: &[&[u8]],
    i: usize,
    depth: usize,
) -> usize {
    if i > 0 {
        let parent_name =
            String::from_utf8_lossy(tokens[i - 1]).trim_start_matches('/').to_string();
        if let Some(parent_cmap) = CMap::load_named_recursive(&parent_name, depth + 1) {
            for (k, v) in parent_cmap.mappings.iter() {
                mappings.insert(k.clone(), v.clone());
            }
            for (k, v) in parent_cmap.mappings_cid.iter() {
                mappings_cid.insert(k.clone(), *v);
            }
            cmap.codespace_ranges.extend(parent_cmap.codespace_ranges.clone());
            if cmap.wmode == 0 {
                cmap.wmode = parent_cmap.wmode;
            }
            cmap.bf_ranges.extend(parent_cmap.bf_ranges.clone());
            cmap.cid_ranges.extend(parent_cmap.cid_ranges);
        }
    }
    i + 1
}

fn handle_cmap_name(cmap: &mut CMap, tokens: &[&[u8]], i: usize) -> usize {
    if i + 1 < tokens.len() {
        cmap.name = String::from_utf8_lossy(tokens[i + 1]).trim_start_matches('/').to_string();
        i + 2
    } else {
        i + 1
    }
}

fn handle_wmode(cmap: &mut CMap, tokens: &[&[u8]], i: usize) -> usize {
    if i + 1 < tokens.len() {
        cmap.wmode = std::str::from_utf8(tokens[i + 1]).unwrap_or("0").parse().unwrap_or(0);
        i + 2
    } else {
        i + 1
    }
}

fn handle_codespacerange(cmap: &mut CMap, tokens: &[&[u8]], i: usize) -> usize {
    let mut next_i = i + 1;
    let count = get_count(tokens, i);
    for _ in 0..count {
        if next_i + 1 < tokens.len() {
            let start = parse_cmap_bytes(tokens[next_i]);
            let end = parse_cmap_bytes(tokens[next_i + 1]);
            cmap.codespace_ranges.push((start, end));
            next_i += 2;
        }
    }
    next_i
}

fn handle_bfchar(
    _cmap: &CMap,
    mappings: &mut BTreeMap<Vec<u8>, String>,
    tokens: &[&[u8]],
    i: usize,
) -> usize {
    let mut next_i = i + 1;
    let count = get_count(tokens, i);
    for _ in 0..count {
        if next_i + 1 < tokens.len() {
            let mut src = parse_cmap_bytes(tokens[next_i]);
            let dst = if tokens[next_i + 1].starts_with(b"/") {
                glyph_name_to_unicode(tokens[next_i + 1])
            } else {
                parse_cmap_string(tokens[next_i + 1])
            };

            // NORMALIZATION: If key is shorter than expected codespace, pad it (Legacy Distiller case)
            if src.len() == 1
                && _cmap.codespace_ranges.iter().any(|(s, _): &(Vec<u8>, Vec<u8>)| s.len() == 2)
            {
                src.insert(0, 0);
            }

            mappings.insert(src, dst);
            next_i += 2;
        }
    }
    next_i + 1
}

fn handle_bfrange(
    cmap: &mut CMap,
    mappings: &mut BTreeMap<Vec<u8>, String>,
    tokens: &[&[u8]],
    i: usize,
) -> usize {
    let mut next_i = i + 1;
    let count = get_count(tokens, i);
    for _ in 0..count {
        if next_i + 2 < tokens.len() {
            let start = parse_cmap_bytes(tokens[next_i]);
            let end = parse_cmap_bytes(tokens[next_i + 1]);
            let dst_base = tokens[next_i + 2];
            next_i += 3;
            process_bfrange_entry(cmap, mappings, &start, &end, dst_base, &mut next_i, tokens);
        }
    }
    next_i + 1
}

fn process_bfrange_entry(
    cmap: &mut CMap,
    new_map: &mut BTreeMap<Vec<u8>, String>,
    start: &[u8],
    end: &[u8],
    dst_base: &[u8],
    next_i: &mut usize,
    tokens: &[&[u8]],
) {
    let mut s_raw = start.to_vec();
    let mut e_raw = end.to_vec();

    // NORMALIZATION: Pad keys if shorter than codespace
    if s_raw.len() == 1 && cmap.codespace_ranges.iter().any(|(s, _)| s.len() == 2) {
        s_raw.insert(0, 0);
        e_raw.insert(0, 0);
    }
    let (s_val, e_val) = (vec_to_u32(&s_raw), vec_to_u32(&e_raw));

    if dst_base.starts_with(b"<") || dst_base.starts_with(b"(") {
        let mut u_val = vec_to_u32(&parse_cmap_bytes(dst_base));
        if e_val >= s_val && e_val - s_val > 100 {
            cmap.bf_ranges.push(CMapRange {
                start: s_val,
                end: e_val,
                base: u_val,
                len: s_raw.len(),
            });
        } else if e_val >= s_val {
            for code in s_val..=e_val {
                let uni = std::char::from_u32(u_val)
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| String::from_utf16_lossy(&[u_val as u16]));
                new_map.insert(u32_to_vec(code, s_raw.len()), uni);
                u_val += 1;
            }
        }
    } else if dst_base == b"[" {
        let s_val = vec_to_u32(&s_raw);
        let mut offset = 0;
        while *next_i < tokens.len() && tokens[*next_i] != b"]" {
            new_map.insert(
                u32_to_vec(s_val + offset, s_raw.len()),
                parse_cmap_string(tokens[*next_i]),
            );
            *next_i += 1;
            offset += 1;
        }
        if *next_i < tokens.len() && tokens[*next_i] == b"]" {
            *next_i += 1;
        }
    }
}

fn handle_cidchar(mappings_cid: &mut BTreeMap<Vec<u8>, u32>, tokens: &[&[u8]], i: usize) -> usize {
    let mut next_i = i + 1;
    let count = get_count(tokens, i);
    for _ in 0..count {
        if next_i + 1 < tokens.len() {
            let src = parse_cmap_bytes(tokens[next_i]);
            let cid = std::str::from_utf8(tokens[next_i + 1]).unwrap_or("0").parse().unwrap_or(0);
            mappings_cid.insert(src, cid);
            next_i += 2;
        }
    }
    next_i
}

fn handle_cidrange(
    cmap: &mut CMap,
    mappings_cid: &mut BTreeMap<Vec<u8>, u32>,
    tokens: &[&[u8]],
    i: usize,
) -> usize {
    let mut next_i = i + 1;
    let count = get_count(tokens, i);
    for _ in 0..count {
        if next_i + 2 < tokens.len() {
            let (start, end) =
                (parse_cmap_bytes(tokens[next_i]), parse_cmap_bytes(tokens[next_i + 1]));
            let cid_base =
                std::str::from_utf8(tokens[next_i + 2]).unwrap_or("0").parse().unwrap_or(0);
            next_i += 3;
            let (s_val, e_val) = (vec_to_u32(&start), vec_to_u32(&end));
            if e_val >= s_val && e_val - s_val > 100 {
                cmap.cid_ranges.push(CMapRange {
                    start: s_val,
                    end: e_val,
                    base: cid_base,
                    len: start.len(),
                });
            } else if e_val >= s_val {
                for v in s_val..=e_val {
                    mappings_cid.insert(u32_to_vec(v, start.len()), cid_base + (v - s_val));
                }
            }
        }
    }
    next_i
}

fn get_count(tokens: &[&[u8]], i: usize) -> usize {
    if i > 0 { std::str::from_utf8(tokens[i - 1]).unwrap_or("0").parse().unwrap_or(0) } else { 0 }
}

fn tokenize_cmap(data: &[u8]) -> Vec<&[u8]> {
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if b.is_ascii_whitespace() || b == b'\r' {
            i += 1;
            continue;
        }
        if b == b'%' {
            i = skip_comment(data, i);
            continue;
        }
        let start = i;
        match b {
            b'(' => i = tokenize_literal(data, i, &mut tokens),
            b'<' => i = tokenize_hex(data, i, &mut tokens),
            b'/' => i = tokenize_name(data, i, &mut tokens),
            b'[' | b']' | b'{' | b'}' => {
                tokens.push(&data[i..=i]);
                i += 1;
            }
            _ => i = tokenize_other(data, i, &mut tokens, start),
        }
    }
    tokens
}

fn skip_comment(data: &[u8], mut i: usize) -> usize {
    while i < data.len() && data[i] != b'\n' && data[i] != b'\r' {
        i += 1;
    }
    if i < data.len() {
        i += 1;
    }
    i
}

fn tokenize_literal<'a>(data: &'a [u8], mut i: usize, tokens: &mut Vec<&'a [u8]>) -> usize {
    let start = i;
    i += 1;
    let (mut depth, mut escaped) = (1, false);
    while i < data.len() && depth > 0 {
        let b = data[i];
        if escaped {
            escaped = false;
        } else if b == b'\\' {
            escaped = true;
        } else if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth -= 1;
        }
        i += 1;
    }
    tokens.push(&data[start..i]);
    i
}

fn tokenize_hex<'a>(data: &'a [u8], mut i: usize, tokens: &mut Vec<&'a [u8]>) -> usize {
    let start = i;
    i += 1;
    while i < data.len() && data[i] != b'>' {
        i += 1;
    }
    if i < data.len() {
        i += 1;
    }
    tokens.push(&data[start..i]);
    i
}

fn tokenize_name<'a>(data: &'a [u8], mut i: usize, tokens: &mut Vec<&'a [u8]>) -> usize {
    let start = i;
    i += 1;
    while i < data.len() && !is_delimiter(data[i]) {
        i += 1;
    }
    tokens.push(&data[start..i]);
    i
}

fn tokenize_other<'a>(
    data: &'a [u8],
    mut i: usize,
    tokens: &mut Vec<&'a [u8]>,
    start: usize,
) -> usize {
    while i < data.len() && !is_delimiter(data[i]) {
        i += 1;
    }
    if start == i {
        i += 1;
    }
    tokens.push(&data[start..i]);
    i
}

fn is_delimiter(b: u8) -> bool {
    b.is_ascii_whitespace() || b == b'\r' || b"<>[]()/{}".contains(&b)
}

fn _is_escaped(data: &[u8], pos: usize) -> bool {
    let mut count = 0;
    let mut p = pos;
    while p > 0 && data[p - 1] == b'\\' {
        count += 1;
        p -= 1;
    }
    count % 2 == 1
}

fn parse_cmap_bytes(v: &[u8]) -> Vec<u8> {
    if v.starts_with(b"<") {
        parse_hex(v)
    } else if v.starts_with(b"(") {
        parse_literal_bytes(v)
    } else {
        v.to_vec()
    }
}

fn parse_hex(v: &[u8]) -> Vec<u8> {
    // Filter to only hex digits, ignoring whitespace and delimiters
    let s = v
        .iter()
        .filter(|&&b| {
            b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || (b'A'..=b'F').contains(&b)
        })
        .collect::<Vec<_>>();
    let mut bytes = Vec::new();
    for i in (0..s.len()).step_by(2) {
        if i + 1 < s.len() {
            let hi = (*s[i] as char).to_digit(16).unwrap_or(0);
            let lo = (*s[i + 1] as char).to_digit(16).unwrap_or(0);
            bytes.push(((hi << 4) | lo) as u8);
        } else {
            // ISO 32000-2: Odd number of digits: append a '0'
            let hi = (*s[i] as char).to_digit(16).unwrap_or(0);
            bytes.push((hi << 4) as u8);
        }
    }
    bytes
}

fn parse_literal_bytes(v: &[u8]) -> Vec<u8> {
    if v.len() < 2 {
        return Vec::new();
    }
    let content = &v[1..v.len() - 1];
    let mut result = Vec::new();
    let mut j = 0;
    while j < content.len() {
        let b = content[j];
        if b == b'\\' {
            j += 1;
            if j < content.len() {
                match content[j] {
                    b'n' => result.push(b'\n'),
                    b'r' => result.push(b'\r'),
                    b't' => result.push(b'\t'),
                    b'(' | b')' | b'\\' => result.push(content[j]),
                    b'0'..=b'7' => {
                        let mut val = content[j] - b'0';
                        if j + 1 < content.len() && content[j + 1] >= b'0' && content[j + 1] <= b'7'
                        {
                            j += 1;
                            val = (val << 3) | (content[j] - b'0');
                            if j + 1 < content.len()
                                && content[j + 1] >= b'0'
                                && content[j + 1] <= b'7'
                            {
                                j += 1;
                                val = (val << 3) | (content[j] - b'0');
                            }
                        }
                        result.push(val);
                    }
                    _ => result.push(content[j]),
                }
            }
        } else {
            result.push(b);
        }
        j += 1;
    }
    result
}

fn parse_cmap_string(v: &[u8]) -> String {
    if v.starts_with(b"<") {
        parse_unicode_hex(v)
    } else if v.starts_with(b"(") {
        let result = parse_literal_bytes(v);
        if result.len() >= 2 && result.len().is_multiple_of(2) {
            let u16s: Vec<u16> =
                result.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            String::from_utf16_lossy(&u16s)
        } else {
            String::from_utf8_lossy(&result).to_string()
        }
    } else {
        String::from_utf8_lossy(v).to_string()
    }
}

fn parse_unicode_hex(v: &[u8]) -> String {
    let bytes = parse_hex(v);
    if bytes.len() >= 2 && bytes.len().is_multiple_of(2) {
        let u16s: Vec<u16> =
            bytes.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u16s)
    } else {
        String::from_utf8_lossy(&bytes).to_string()
    }
}

fn vec_to_u32(v: &[u8]) -> u32 {
    let mut val = 0u32;
    for &b in v {
        val = (val << 8) | u32::from(b);
    }
    val
}

fn u32_to_vec(val: u32, len: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(len);
    for i in (0..len).rev() {
        v.push(((val >> (i * 8)) & 0xFF) as u8);
    }
    v
}

/// Resolves a glyph name to the text it denotes, including `uniXXXX` forms.
pub fn glyph_name_to_unicode(v: &[u8]) -> String {
    let name = if v.starts_with(b"/") { &v[1..] } else { v };
    let name_str = String::from_utf8_lossy(name);

    crate::agl::lookup(&name_str).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agl_lookup() {
        assert_eq!(glyph_name_to_unicode(b"/bullet"), "\u{2022}");
        assert_eq!(glyph_name_to_unicode(b"bullet"), "\u{2022}");
        assert_eq!(glyph_name_to_unicode(b"/uni2022"), "\u{2022}");
        assert_eq!(glyph_name_to_unicode(b"/u2022"), "\u{2022}");
        assert_eq!(glyph_name_to_unicode(b"/A"), "A");
    }

    #[test]
    fn test_hex_parsing() {
        assert_eq!(parse_hex(b"<ABC>"), vec![0xAB, 0xC0]);
        assert_eq!(parse_hex(b"<ABCD>"), vec![0xAB, 0xCD]);
    }

    #[test]
    fn test_large_cmap_range() {
        let cmap_data = b"
            /CMapName /TestLargeRange def
            1 begincidrange
            <000000> <FFFFFF> 0
            endcidrange
        ";
        // This should now be instant and memory-efficient
        let cmap = CMap::parse(cmap_data).unwrap();
        assert_eq!(cmap.cid_ranges.len(), 1);
        assert_eq!(cmap.mappings_cid.len(), 0);

        // Test mapping
        assert_eq!(cmap.to_cid(&[0x00, 0x00, 0x01]), 1);
        assert_eq!(cmap.to_cid(&[0xFF, 0xFF, 0xFF]), 0xFFFFFF);
    }

    #[test]
    fn test_inverted_range_underflow_safety() {
        let cmap_data = b"
            /CMapName /InvertedRange def
            1 begincidrange
            <0005> <0001> 100
            endcidrange
        ";
        // Should not panic on e_val < s_val
        let cmap = CMap::parse(cmap_data).unwrap();
        assert_eq!(cmap.cid_ranges.len(), 0);
    }
}
