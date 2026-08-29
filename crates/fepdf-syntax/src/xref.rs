//! Locating a PDF's parts: the header, the cross-reference sections, the trailer.
//!
//! Everything here works on bytes and offsets. Building objects out of what these
//! offsets point at needs the arena, so it belongs to the model; finding them does
//! not, so it belongs here (`ARCHITECTURE.md` §4).
//!
//! The functions are deliberately tolerant. Real files put bytes before the header,
//! disagree with their own `startxref`, and pad entries to the wrong width. Each
//! tolerance is a decision the caller should record — see `ARCHITECTURE.md` §4.3.

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

/// Where one object lives, according to a cross-reference section.
///
/// Both section forms produce this. A classic table (7.5.4) can only say *free* or
/// *at an offset*; a cross-reference stream (7.5.8) can additionally place an object
/// inside an object stream, which is why the two forms share one type rather than
/// each having its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrefRecord {
    /// The slot is free and holds no object.
    Free {
        /// Generation the slot has reached.
        generation: u16,
    },
    /// The object is written at a byte offset in the file.
    InFile {
        /// Byte offset, relative to the header.
        offset: u64,
        /// Generation number.
        generation: u16,
    },
    /// The object is stored inside an object stream (7.5.7).
    InObjectStream {
        /// Object number of the containing `/Type /ObjStm`.
        container: u32,
        /// Position of the object within that stream.
        index: u32,
    },
}

impl XrefRecord {
    /// The byte offset, for records that name one.
    #[must_use]
    pub const fn offset(&self) -> Option<u64> {
        match self {
            Self::InFile { offset, .. } => Some(*offset),
            Self::Free { .. } | Self::InObjectStream { .. } => None,
        }
    }

    /// Whether the record refers to a live object.
    #[must_use]
    pub const fn is_in_use(&self) -> bool {
        !matches!(self, Self::Free { .. })
    }
}

/// A parsed cross-reference section and the byte range of its trailer dictionary.
#[derive(Debug, Clone)]
pub struct XrefTable {
    /// Object number to record.
    pub entries: BTreeMap<u32, XrefRecord>,
    /// Offset just past `trailer`, where the dictionary begins. Absent for a
    /// cross-reference stream, whose stream dictionary serves as the trailer.
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
            let generation = u16::try_from(generation).unwrap_or(0);
            entries.insert(
                number,
                if kind == b'n' {
                    XrefRecord::InFile { offset, generation }
                } else {
                    XrefRecord::Free { generation }
                },
            );
        }
    }
}

/// How a cross-reference stream's binary payload is laid out (ISO 32000-2 7.5.8).
///
/// Both fields come from the stream's dictionary, which only the model layer can
/// parse; this layer is handed the result so that it stays free of the object model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XrefStreamLayout {
    /// `/W`: byte width of the type, second and third fields.
    pub widths: [usize; 3],
    /// `/Index`: `(first object number, count)` pairs. Defaults to one pair covering
    /// `0..Size` when the dictionary omits it.
    pub index: Vec<(u32, u32)>,
}

impl XrefStreamLayout {
    /// The layout implied by `/W` alone, covering `0..size` as the standard's default
    /// `/Index` does.
    #[must_use]
    pub fn covering(widths: [usize; 3], size: u32) -> Self {
        Self { widths, index: vec![(0, size)] }
    }

    /// Bytes per entry.
    #[must_use]
    pub fn entry_width(&self) -> usize {
        self.widths.iter().sum()
    }
}

/// Decodes the payload of a cross-reference stream.
///
/// `data` must already be decoded — the stream's filters are the model layer's
/// business, since a `/DecodeParms` dictionary has to be resolved to read them.
///
/// A zero-width first field means every entry is type 1, which the standard specifies
/// so that a file of only uncompressed objects need not store the type at all.
pub fn parse_xref_stream_data(
    data: &[u8],
    layout: &XrefStreamLayout,
) -> SyntaxResult<BTreeMap<u32, XrefRecord>> {
    let width = layout.entry_width();
    if width == 0 {
        return Err(SyntaxError::Crypto("cross-reference stream has zero-width entries".into()));
    }

    let mut entries = BTreeMap::new();
    let mut cursor = 0usize;
    for &(first, count) in &layout.index {
        for i in 0..count {
            if cursor + width > data.len() {
                // A truncated payload is worth keeping what was read: the objects
                // already described are still reachable.
                return Ok(entries);
            }
            let row = &data[cursor..cursor + width];
            cursor += width;

            let (kind, rest) = take_field(row, layout.widths[0]);
            let (field2, rest) = take_field(rest, layout.widths[1]);
            let (field3, _) = take_field(rest, layout.widths[2]);

            // 7.5.8.3: an absent type field means type 1.
            let kind = if layout.widths[0] == 0 { 1 } else { kind };
            let number = first.saturating_add(i);
            let record = match kind {
                0 => XrefRecord::Free { generation: u16::try_from(field3).unwrap_or(0) },
                2 => XrefRecord::InObjectStream {
                    container: u32::try_from(field2).unwrap_or(0),
                    index: u32::try_from(field3).unwrap_or(0),
                },
                // Type 1, and anything unrecognised: the standard reserves higher
                // numbers, and a reader that cannot interpret them is told to treat
                // the entry as though it referenced a free object.
                1 => XrefRecord::InFile {
                    offset: field2,
                    generation: u16::try_from(field3).unwrap_or(0),
                },
                _ => XrefRecord::Free { generation: 0 },
            };
            entries.insert(number, record);
        }
    }
    Ok(entries)
}

/// Reads a big-endian field of `width` bytes, returning it and the remainder.
fn take_field(row: &[u8], width: usize) -> (u64, &[u8]) {
    let (head, tail) = row.split_at(width.min(row.len()));
    (head.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b)), tail)
}

/// Where a cross-reference section says its predecessors are (ISO 32000-2 7.5.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct XrefLinks {
    /// `/Prev`: the previous section, for a file built by incremental update.
    pub prev: Option<u64>,
    /// `/XRefStm`: on a hybrid-reference file (7.5.8.4), a cross-reference stream
    /// carrying entries the classic table deliberately omits, so that readers which
    /// predate streams still see a usable — if incomplete — file.
    pub xref_stm: Option<u64>,
}

/// Reads `/Prev` and `/XRefStm` out of a trailer's leading bytes.
///
/// Deliberately textual: the trailer is a dictionary the model layer parses properly,
/// but following the chain only needs two integers, and needing the object model to
/// find the next section would put the whole chain above this layer.
#[must_use]
pub fn read_links(bytes: &[u8], trailer_at: usize) -> XrefLinks {
    let end = bytes.len().min(trailer_at + TRAILER_SCAN_WINDOW);
    let head = &bytes[trailer_at.min(bytes.len())..end];
    XrefLinks { prev: read_key_u64(head, b"/Prev"), xref_stm: read_key_u64(head, b"/XRefStm") }
}

/// The order in which cross-reference sections should be read.
///
/// Oldest first, so that a later section overwrites an earlier definition of the same
/// object — which is what an incremental update means. A section already visited ends
/// the walk, so a `/Prev` cycle cannot hang the reader.
#[must_use]
pub fn section_chain(bytes: &[u8], start: u64) -> Vec<u64> {
    let mut order = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut next = Some(start);

    while let Some(at) = next {
        if !seen.insert(at) {
            break;
        }
        let Ok(offset) = usize::try_from(at) else { break };
        if offset >= bytes.len() {
            break;
        }
        order.push(at);

        // The trailer sits after the table; for a stream the dictionary is the
        // trailer and begins at the section itself.
        let links = parse_xref_table(bytes, offset).map_or_else(
            |_| read_links(bytes, offset),
            |t| read_links(bytes, t.trailer_at.unwrap_or(offset)),
        );
        // A hybrid file's stream supplements the table it sits beside, so it is read
        // just before it rather than being part of the /Prev chain.
        if let Some(stm) = links.xref_stm
            && seen.insert(stm)
        {
            order.push(stm);
        }
        next = links.prev;
    }

    order.reverse();
    order
}

/// How far past a trailer keyword its dictionary is scanned for links.
const TRAILER_SCAN_WINDOW: usize = 2048;

/// Reads `key` followed by an unsigned decimal.
fn read_key_u64(head: &[u8], key: &[u8]) -> Option<u64> {
    let at = head.windows(key.len()).position(|w| w == key)? + key.len();
    let rest = &head[at..];
    // Guard against /PrevSomething matching /Prev.
    if rest.first().is_some_and(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let digits: Vec<u8> = rest
        .iter()
        .skip_while(|b| b.is_ascii_whitespace())
        .take_while(|b| b.is_ascii_digit())
        .copied()
        .collect();
    String::from_utf8_lossy(&digits).parse().ok()
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
        assert_eq!(t.entries[&0], XrefRecord::Free { generation: 65535 });
        assert_eq!(t.entries[&1], XrefRecord::InFile { offset: 15, generation: 0 });
        assert_eq!(t.entries[&2], XrefRecord::InFile { offset: 200, generation: 7 });
        assert!(t.trailer_at.is_some());
    }

    #[test]
    fn xref_table_tolerates_wrong_entry_padding() {
        // The standard specifies twenty-byte entries; producers get it wrong, so the
        // fields are read as tokens rather than by fixed width.
        let data = b"xref\n0 2\n0 65535 f\n15 0 n\ntrailer\n<<>>";
        let t = parse_xref_table(data, 0).unwrap();
        assert_eq!(t.entries.len(), 2);
        assert_eq!(t.entries[&1].offset(), Some(15));
        assert!(t.entries[&1].is_in_use());
    }

    #[test]
    fn xref_table_handles_multiple_subsections() {
        let data = b"xref\n0 1\n0000000000 65535 f \n5 2\n0000000100 00000 n \n0000000200 00000 n \ntrailer\n<<>>";
        let t = parse_xref_table(data, 0).unwrap();
        assert_eq!(t.entries.len(), 3);
        assert_eq!(t.entries[&5].offset(), Some(100));
        assert_eq!(t.entries[&6].offset(), Some(200));
        assert!(!t.entries.contains_key(&1));
    }

    #[test]
    fn a_non_table_is_refused_rather_than_misread() {
        assert!(parse_xref_table(b"not an xref at all", 0).is_err());
    }

    #[test]
    fn xref_stream_reads_all_three_entry_types() {
        // /W [1 2 1]: type, then a two-byte field, then a one-byte field.
        let layout = XrefStreamLayout::covering([1, 2, 1], 3);
        let data = [
            0x00, 0x00, 0x00, 0xFF, // free, generation 255
            0x01, 0x01, 0x00, 0x00, // at offset 0x0100
            0x02, 0x00, 0x09, 0x04, // inside object stream 9, index 4
        ];
        let e = parse_xref_stream_data(&data, &layout).unwrap();
        assert_eq!(e[&0], XrefRecord::Free { generation: 255 });
        assert_eq!(e[&1], XrefRecord::InFile { offset: 0x0100, generation: 0 });
        assert_eq!(e[&2], XrefRecord::InObjectStream { container: 9, index: 4 });
    }

    #[test]
    fn a_zero_width_type_field_means_every_entry_is_type_one() {
        // 7.5.8.3: a file of only uncompressed objects need not store the type.
        let layout = XrefStreamLayout::covering([0, 2, 1], 2);
        let data = [0x00, 0x10, 0x00, 0x00, 0x20, 0x03];
        let e = parse_xref_stream_data(&data, &layout).unwrap();
        assert_eq!(e[&0], XrefRecord::InFile { offset: 0x10, generation: 0 });
        assert_eq!(e[&1], XrefRecord::InFile { offset: 0x20, generation: 3 });
    }

    #[test]
    fn index_subsections_place_entries_at_their_own_numbers() {
        let layout = XrefStreamLayout { widths: [1, 1, 1], index: vec![(0, 1), (7, 2)] };
        let data = [0x01, 0x0A, 0x00, 0x01, 0x0B, 0x00, 0x01, 0x0C, 0x00];
        let e = parse_xref_stream_data(&data, &layout).unwrap();
        assert_eq!(e.len(), 3);
        assert_eq!(e[&0].offset(), Some(0x0A));
        assert_eq!(e[&7].offset(), Some(0x0B));
        assert_eq!(e[&8].offset(), Some(0x0C));
        assert!(!e.contains_key(&1));
    }

    #[test]
    fn a_truncated_payload_keeps_what_was_readable() {
        // The objects already described are still reachable, so returning them beats
        // discarding the section.
        let layout = XrefStreamLayout::covering([1, 1, 1], 4);
        let data = [0x01, 0x0A, 0x00, 0x01, 0x0B, 0x00, 0x01];
        let e = parse_xref_stream_data(&data, &layout).unwrap();
        assert_eq!(e.len(), 2);
    }

    #[test]
    fn an_unrecognised_type_is_treated_as_free() {
        // 7.5.8.3 reserves higher type numbers and tells a reader that cannot
        // interpret them to treat the entry as a reference to a free object.
        let layout = XrefStreamLayout::covering([1, 1, 1], 1);
        let e = parse_xref_stream_data(&[0x07, 0x0A, 0x00], &layout).unwrap();
        assert_eq!(e[&0], XrefRecord::Free { generation: 0 });
    }

    #[test]
    fn zero_width_entries_are_refused_rather_than_looping() {
        let layout = XrefStreamLayout::covering([0, 0, 0], 5);
        assert!(parse_xref_stream_data(&[1, 2, 3], &layout).is_err());
    }

    #[test]
    fn links_are_read_from_a_trailer() {
        let data = b"trailer\n<< /Size 9 /Prev 1234 /XRefStm 5678 >>";
        let l = read_links(data, 7);
        assert_eq!(l.prev, Some(1234));
        assert_eq!(l.xref_stm, Some(5678));
    }

    #[test]
    fn a_longer_key_is_not_mistaken_for_prev() {
        let data = b"<< /PrevPage 3 >>";
        assert_eq!(read_links(data, 0).prev, None);
    }

    #[test]
    fn the_chain_is_walked_oldest_first() {
        // Later sections must overwrite earlier ones, so they are read last.
        let mut data = vec![b' '; 400];
        data.splice(0..40, *b"xref\n0 0\ntrailer\n<< /Size 1 >>        ");
        data.splice(200..248, *b"xref\n0 0\ntrailer\n<< /Size 1 /Prev 0 >>        ");
        let order = section_chain(&data, 200);
        assert_eq!(order, vec![0, 200], "the /Prev target is read before the section naming it");
    }

    #[test]
    fn a_cyclic_prev_terminates() {
        // A file whose sections point at each other must not hang the reader.
        let mut data = vec![b' '; 400];
        data.splice(0..48, *b"xref\n0 0\ntrailer\n<< /Prev 200 >>              ");
        data.splice(200..246, *b"xref\n0 0\ntrailer\n<< /Prev 0 >>              ");
        let order = section_chain(&data, 0);
        assert_eq!(order.len(), 2, "each section is visited once");
    }

    #[test]
    fn a_hybrid_files_stream_is_read_beside_its_table() {
        // 7.5.8.4: the stream carries entries the table omits for older readers, so
        // it must be read, and read before the table it accompanies.
        let mut data = vec![b' '; 400];
        data.splice(0..52, *b"xref\n0 0\ntrailer\n<< /XRefStm 300 >>              ");
        let order = section_chain(&data, 0);
        assert!(order.contains(&300), "the supplementary stream is not skipped");
        assert!(
            order.iter().position(|&o| o == 300) < order.iter().position(|&o| o == 0),
            "the stream is read before the table beside it"
        );
    }

    #[test]
    fn an_offset_past_the_end_stops_the_walk() {
        assert!(section_chain(b"short", 9_000).is_empty());
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
