//! Locating a PDF's parts: the header, the cross-reference sections, the trailer.
//!
//! Everything here works on bytes and offsets. Building objects out of what these
//! offsets point at needs the arena, so it belongs to the model; finding them does
//! not, so it belongs here (`ARCHITECTURE.md` §4).
//!
//! The functions are deliberately tolerant. Real files put bytes before the header,
//! disagree with their own `startxref`, and pad entries to the wrong width. Each
//! tolerance is a decision the caller should record — see `ARCHITECTURE.md` §5.3.

use crate::{SyntaxError, SyntaxResult};
use std::collections::BTreeMap;

/// How far into a file the `%PDF-` header may appear (ISO 32000-2 7.5.2 allows a
/// reader to scan; files routinely arrive with bytes prepended).
pub const HEADER_SEARCH_WINDOW: usize = 1024;

/// How far back from the end `startxref` is looked for.
const TAIL_SEARCH_WINDOW: usize = 2048;

/// Where a document's header sits and what version it claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Byte offset of `%PDF-`. Non-zero when the file has a prefix, in which case
    /// every offset in the file is relative to it.
    pub offset: usize,
    /// The declared version, such as `1.7`.
    pub version: String,
}

/// One cross-reference entry: where an object is, or that it is free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XrefEntry {
    /// Byte offset of the object, or the next free object number when `!in_use`.
    pub offset: u64,
    /// Generation number.
    pub generation: u16,
    /// Whether the entry refers to a live object.
    pub in_use: bool,
}

/// A parsed cross-reference section and the byte range of its trailer dictionary.
#[derive(Debug, Clone)]
pub struct XrefTable {
    /// Object number to entry.
    pub entries: BTreeMap<u32, XrefEntry>,
    /// Offset just past `trailer`, where the dictionary begins.
    pub trailer_at: Option<usize>,
}

/// Finds the header, scanning rather than demanding it at offset zero.
#[must_use]
pub fn find_header(bytes: &[u8]) -> Option<Header> {
    let window = &bytes[..bytes.len().min(HEADER_SEARCH_WINDOW + 8)];
    let offset = window.windows(5).position(|w| w == b"%PDF-")?;
    let rest = &bytes[offset + 5..];
    let len =
        rest.iter().position(|b| !(b.is_ascii_digit() || *b == b'.')).unwrap_or(rest.len()).min(8);
    let version = String::from_utf8_lossy(&rest[..len]).into_owned();
    if version.is_empty() {
        return None;
    }
    Some(Header { offset, version })
}

/// Reads the offset given by the last `startxref` in the file.
///
/// Returns `None` when it is absent or unreadable; the caller then has to
/// reconstruct, which is a decision worth recording.
#[must_use]
pub fn find_startxref(bytes: &[u8]) -> Option<u64> {
    let from = bytes.len().saturating_sub(TAIL_SEARCH_WINDOW);
    let tail = &bytes[from..];
    let at = tail.windows(9).rposition(|w| w == b"startxref").map(|p| from + p + 9)?;
    let digits: Vec<u8> = bytes[at..]
        .iter()
        .skip_while(|b| b.is_ascii_whitespace())
        .take_while(|b| b.is_ascii_digit())
        .copied()
        .collect();
    String::from_utf8_lossy(&digits).parse().ok()
}

/// Parses a classic cross-reference table (ISO 32000-2 7.5.4) starting at `at`.
///
/// `at` must address the `xref` keyword. Subsection headers are `first count`, and
/// each entry is nominally twenty bytes, but the width is not relied upon: producers
/// get the padding wrong often enough that fields are read by token instead.
pub fn parse_xref_table(bytes: &[u8], at: usize) -> SyntaxResult<XrefTable> {
    let mut cursor = at;
    if !bytes[cursor..].starts_with(b"xref") {
        return Err(SyntaxError::Crypto("not a cross-reference table".into()));
    }
    cursor += 4;

    let mut entries = BTreeMap::new();
    loop {
        cursor = skip_whitespace(bytes, cursor);
        if bytes[cursor..].starts_with(b"trailer") {
            return Ok(XrefTable { entries, trailer_at: Some(cursor + 7) });
        }
        let Some((first, next)) = read_u64(bytes, cursor) else {
            // No subsection header and no trailer: the section ends here.
            return Ok(XrefTable { entries, trailer_at: None });
        };
        cursor = skip_whitespace(bytes, next);
        let Some((count, next)) = read_u64(bytes, cursor) else {
            return Ok(XrefTable { entries, trailer_at: None });
        };
        cursor = next;

        for i in 0..count {
            cursor = skip_whitespace(bytes, cursor);
            let Some((offset, next)) = read_u64(bytes, cursor) else {
                return Ok(XrefTable { entries, trailer_at: None });
            };
            cursor = skip_whitespace(bytes, next);
            let Some((generation, next)) = read_u64(bytes, cursor) else {
                return Ok(XrefTable { entries, trailer_at: None });
            };
            cursor = skip_whitespace(bytes, next);
            let kind = bytes.get(cursor).copied().unwrap_or(b'n');
            cursor += 1;

            let number = u32::try_from(first + i).unwrap_or(u32::MAX);
            entries.insert(
                number,
                XrefEntry {
                    offset,
                    generation: u16::try_from(generation).unwrap_or(0),
                    in_use: kind == b'n',
                },
            );
        }
    }
}

/// Byte offsets of every `N G obj` in the file, in the order they appear.
///
/// This is the recovery path: when the cross-reference cannot be trusted, the objects
/// themselves are still findable. A later definition of the same number wins, which
/// is what an incremental update means.
#[must_use]
pub fn scan_indirect_objects(bytes: &[u8]) -> BTreeMap<u32, u64> {
    let mut found = BTreeMap::new();
    let mut i = 0usize;
    while let Some(p) = bytes[i..].windows(3).position(|w| w == b"obj") {
        let at = i + p;
        i = at + 3;
        // Walk back over "N G " to the object number.
        let Some(head) = object_head(bytes, at) else { continue };
        found.insert(head.0, head.1 as u64);
    }
    found
}

/// Reads the `N G` preceding an `obj` keyword, returning the number and its offset.
fn object_head(bytes: &[u8], obj_at: usize) -> Option<(u32, usize)> {
    let mut i = obj_at;
    i = skip_back_whitespace(bytes, i)?;
    let gen_end = i;
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == gen_end {
        return None;
    }
    i = skip_back_whitespace(bytes, i)?;
    let num_end = i;
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == num_end {
        return None;
    }
    let number: u32 = std::str::from_utf8(&bytes[i..num_end]).ok()?.parse().ok()?;
    Some((number, i))
}

fn skip_back_whitespace(bytes: &[u8], mut i: usize) -> Option<usize> {
    while i > 0 && bytes[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    (i > 0).then_some(i)
}

fn skip_whitespace(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Reads an unsigned decimal at `i`, returning it and the offset just past it.
fn read_u64(bytes: &[u8], i: usize) -> Option<(u64, usize)> {
    let mut end = i;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == i {
        return None;
    }
    let value = std::str::from_utf8(&bytes[i..end]).ok()?.parse().ok()?;
    Some((value, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_found_at_the_start() {
        let h = find_header(b"%PDF-1.7\n1 0 obj").unwrap();
        assert_eq!(h, Header { offset: 0, version: "1.7".into() });
    }

    #[test]
    fn header_is_found_after_prepended_bytes() {
        // Mail gateways and scanners prepend bytes; the header is still the header,
        // and every offset in the file is then relative to it.
        let mut data = vec![b'#'; 300];
        data.extend_from_slice(b"%PDF-2.0\n");
        let h = find_header(&data).unwrap();
        assert_eq!(h.offset, 300);
        assert_eq!(h.version, "2.0");
    }

    #[test]
    fn header_beyond_the_window_is_not_a_header() {
        let mut data = vec![b'#'; HEADER_SEARCH_WINDOW + 64];
        data.extend_from_slice(b"%PDF-1.4\n");
        assert!(find_header(&data).is_none());
    }

    #[test]
    fn startxref_reads_the_last_occurrence() {
        // Incremental updates leave earlier ones behind; the last is authoritative.
        let data = b"startxref\n100\n%%EOF\n...\nstartxref\n4242\n%%EOF";
        assert_eq!(find_startxref(data), Some(4242));
    }

    #[test]
    fn startxref_absent_is_reported_not_guessed() {
        assert_eq!(find_startxref(b"%PDF-1.7\nno trailer here"), None);
    }

    #[test]
    fn xref_table_reads_entries_and_locates_the_trailer() {
        let data = b"xref\n0 3\n0000000000 65535 f \n0000000015 00000 n \n0000000200 00007 n \ntrailer\n<< /Size 3 >>";
        let t = parse_xref_table(data, 0).unwrap();
        assert_eq!(t.entries.len(), 3);
        assert_eq!(t.entries[&0], XrefEntry { offset: 0, generation: 65535, in_use: false });
        assert_eq!(t.entries[&1], XrefEntry { offset: 15, generation: 0, in_use: true });
        assert_eq!(t.entries[&2], XrefEntry { offset: 200, generation: 7, in_use: true });
        assert!(t.trailer_at.is_some());
    }

    #[test]
    fn xref_table_tolerates_wrong_entry_padding() {
        // The standard specifies twenty-byte entries; producers get it wrong, so the
        // fields are read as tokens rather than by fixed width.
        let data = b"xref\n0 2\n0 65535 f\n15 0 n\ntrailer\n<<>>";
        let t = parse_xref_table(data, 0).unwrap();
        assert_eq!(t.entries.len(), 2);
        assert_eq!(t.entries[&1].offset, 15);
        assert!(t.entries[&1].in_use);
    }

    #[test]
    fn xref_table_handles_multiple_subsections() {
        let data = b"xref\n0 1\n0000000000 65535 f \n5 2\n0000000100 00000 n \n0000000200 00000 n \ntrailer\n<<>>";
        let t = parse_xref_table(data, 0).unwrap();
        assert_eq!(t.entries.len(), 3);
        assert_eq!(t.entries[&5].offset, 100);
        assert_eq!(t.entries[&6].offset, 200);
        assert!(!t.entries.contains_key(&1));
    }

    #[test]
    fn a_non_table_is_refused_rather_than_misread() {
        assert!(parse_xref_table(b"not an xref at all", 0).is_err());
    }

    #[test]
    fn scanning_finds_objects_when_the_xref_cannot_be_trusted() {
        let data = b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\n12 0 obj\n[1 2]\nendobj\n";
        let found = scan_indirect_objects(data);
        assert_eq!(found.len(), 2);
        assert!(found.contains_key(&1));
        assert!(found.contains_key(&12));
    }

    #[test]
    fn scanning_prefers_the_later_definition() {
        // An incremental update redefines an object; the last one written wins.
        let data = b"1 0 obj\n<<>>\nendobj\n1 0 obj\n[9]\nendobj\n";
        let found = scan_indirect_objects(data);
        assert_eq!(found.len(), 1);
        assert!(found[&1] > 10, "expected the second definition's offset");
    }
}
