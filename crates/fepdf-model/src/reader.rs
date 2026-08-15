//! Building objects out of the byte offsets `fepdf-syntax` located.
//!
//! `fepdf_syntax::xref` says *where* things are; this says *what* they are. The split
//! is the arena: constructing an [`Object`] needs one, finding its offset does not.
//!
//! Every tolerance here is recorded as a [`Decision`], because the choices this module
//! makes are the substance of reading a pre-2.0 file — see `ARCHITECTURE.md` §5.3.

use crate::arena::PdfArena;
use crate::error::{PdfError, PdfResult};
use crate::handle::Handle;
use crate::interpretation::{Decision, DecisionLog};
use crate::object::{Object, SublimatedData};
use crate::parser::Parser;
use bytes::Bytes;
use fepdf_syntax::lexer::{Lexer, Token};
use fepdf_syntax::xref::{self, XrefRecord};
use std::collections::{BTreeMap, BTreeSet};

/// One indirect object as written in the file.
#[derive(Debug, Clone)]
pub struct IndirectObject {
    /// Object number.
    pub number: u32,
    /// Generation number.
    pub generation: u16,
    /// The object itself.
    pub object: Object,
}

/// Reads the `N G obj … endobj` at `offset`.
///
/// Streams are recognised here rather than in the object parser because delimiting
/// one needs `/Length`, which is a document-level question: the value may be an
/// indirect reference, and it is frequently wrong.
pub fn parse_indirect_at(
    bytes: &[u8],
    offset: usize,
    arena: &PdfArena,
    decisions: &mut DecisionLog,
) -> PdfResult<IndirectObject> {
    if offset >= bytes.len() {
        return Err(PdfError::Parse {
            pos: offset,
            message: "object offset lies past the end of the file".into(),
        });
    }

    let mut lexer = Lexer::new(Bytes::copy_from_slice(&bytes[offset..]));
    let number = expect_unsigned(&mut lexer, offset)?;
    let generation = expect_unsigned(&mut lexer, offset)?;
    match lexer.next_token()? {
        Token::Keyword(k) if k == "obj" => {}
        other => {
            return Err(PdfError::Parse {
                pos: offset,
                message: format!("expected `obj`, found {other:?}").into(),
            });
        }
    }

    let body_at = offset + lexer.pos();
    let mut parser = Parser::new(Bytes::copy_from_slice(&bytes[body_at..]), arena);
    let object = parser.parse_object()?;
    let after_body = body_at + parser.position();

    let object = match stream_start(bytes, after_body) {
        Some(data_at) => attach_stream(bytes, data_at, object, arena, decisions)?,
        None => object,
    };

    Ok(IndirectObject {
        number: u32::try_from(number).unwrap_or(0),
        generation: u16::try_from(generation).unwrap_or(0),
        object,
    })
}

/// Where a stream's data begins, if the object is followed by `stream`.
///
/// 7.3.8.1 requires CRLF or LF after the keyword, never CR alone; a lone CR is
/// accepted because producers emit it and the intent is unambiguous.
fn stream_start(bytes: &[u8], after_body: usize) -> Option<usize> {
    let mut i = after_body;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if !bytes[i..].starts_with(b"stream") {
        return None;
    }
    i += 6;
    if bytes[i..].starts_with(b"\r\n") {
        i += 2;
    } else if bytes.get(i) == Some(&b'\n') || bytes.get(i) == Some(&b'\r') {
        i += 1;
    }
    Some(i)
}

/// Turns a stream's dictionary into a stream object, delimiting its data.
fn attach_stream(
    bytes: &[u8],
    data_at: usize,
    dict_object: Object,
    arena: &PdfArena,
    decisions: &mut DecisionLog,
) -> PdfResult<Object> {
    let Object::Dictionary(dict_h) = dict_object else {
        return Err(PdfError::Parse {
            pos: data_at,
            message: "`stream` follows an object that is not a dictionary".into(),
        });
    };

    let declared = arena
        .get_dict(dict_h)
        .and_then(|d| d.get(&arena.name("Length")).cloned())
        .and_then(|o| match o {
            Object::Integer(n) => usize::try_from(n).ok(),
            // An indirect /Length cannot be resolved until every object is loaded,
            // so the data is delimited by scanning instead.
            _ => None,
        });

    let end = resolve_stream_extent(bytes, data_at, declared, decisions);
    let length = end.saturating_sub(data_at);
    if declared != Some(length) {
        // State the extent that was actually used. Leaving an indirect /Length in place
        // would strand the integer object it names, which nothing else references.
        record_length(arena, dict_h, length);
    }

    let data = Bytes::copy_from_slice(&bytes[data_at..end.min(bytes.len())]);
    Ok(Object::Stream(dict_h, std::sync::Arc::new(SublimatedData::Raw(data))))
}

/// Writes the resolved extent back into the stream dictionary as a direct integer.
fn record_length(arena: &PdfArena, dict_h: DictHandle, length: usize) {
    let Some(mut dict) = arena.get_dict(dict_h) else { return };
    dict.insert(arena.name("Length"), Object::Integer(i64::try_from(length).unwrap_or(0)));
    arena.set_dict(dict_h, dict);
}

/// Decides where a stream's data ends, recording why when the file is not clear.
///
/// `/Length` is authoritative when it agrees with the `endstream` keyword. When they
/// disagree the scanned extent wins: a wrong length truncates or overruns real data,
/// whereas the keyword is where the producer actually stopped writing.
fn resolve_stream_extent(
    bytes: &[u8],
    data_at: usize,
    declared: Option<usize>,
    decisions: &mut DecisionLog,
) -> usize {
    let keyword = find_endstream(bytes, data_at);
    let scanned = keyword.map(|at| trim_eol(bytes, data_at, at));
    match (declared, scanned) {
        // /Length defines the extent (7.3.8.2); only an EOL may follow it. Data that
        // itself ends in CR makes the trailing marker ambiguous, so where /Length is
        // consistent with the keyword's position it is believed over the scan.
        (Some(len), Some(_))
            if keyword.is_some_and(|at| separated_by_space(bytes, data_at + len, at)) =>
        {
            data_at + len
        }
        (Some(len), Some(found)) => {
            decisions.push(Decision::repaired(
                "7.3.8.2",
                format!("/Length {len} but endstream is {} bytes in", found - data_at),
                "used the scanned extent, since /Length would truncate or overrun the data",
            ));
            found
        }
        (Some(len), None) if data_at + len <= bytes.len() => {
            decisions.push(Decision::repaired(
                "7.3.8.2",
                "no endstream keyword after the stream data",
                "trusted /Length",
            ));
            data_at + len
        }
        (None, Some(found)) => {
            decisions.push(Decision::ambiguity(
                "7.3.8.2",
                "/Length absent or an indirect reference",
                "delimited the data by scanning to endstream",
            ));
            found
        }
        _ => {
            decisions.push(Decision::violation(
                "7.3.8.2",
                "stream has neither a usable /Length nor an endstream keyword",
                "treated the remainder of the file as the stream data",
            ));
            bytes.len()
        }
    }
}

/// Offset of the `endstream` keyword after `from`, excluding the whitespace before it.
fn find_endstream(bytes: &[u8], from: usize) -> Option<usize> {
    Some(from + bytes[from..].windows(9).position(|w| w == b"endstream")?)
}

/// Backs off the EOL introduced before `endstream`, which is not data (7.3.8.1).
///
/// Only used when `/Length` is missing or wrong: it cannot distinguish data ending in
/// CR from a CRLF marker, and guesses that the marker is longer.
fn trim_eol(bytes: &[u8], from: usize, keyword: usize) -> usize {
    let mut end = keyword;
    if end > from && bytes[end - 1] == b'\n' {
        end -= 1;
    }
    if end > from && bytes[end - 1] == b'\r' {
        end -= 1;
    }
    end
}

/// Whether only whitespace sits between the declared end of the data and `endstream`.
fn separated_by_space(bytes: &[u8], data_end: usize, keyword: usize) -> bool {
    data_end <= keyword && bytes[data_end..keyword].iter().all(u8::is_ascii_whitespace)
}

/// A document as read from bytes, before ingestion normalises it.
///
/// The arena is populated so that **an object's handle is its object number**. The
/// parser already builds `Object::Reference(Handle::new(n))` from `n 0 R`, so no
/// remapping table is needed: references are correct as written. Numbers a file skips
/// become `Null` slots, which costs one pool entry each.
pub struct RawDocument {
    /// Every object, addressed by its own number.
    pub arena: PdfArena,
    /// The version from the header, such as `1.7`.
    pub version: String,
    /// The trailer dictionary, from the newest section that carried one.
    pub trailer: Option<DictHandle>,
    /// What had to be decided to read the file.
    pub decisions: DecisionLog,
}

/// A handle to a dictionary, spelt out because the type is otherwise unwieldy.
pub type DictHandle = Handle<BTreeMap<Handle<crate::object::PdfName>, Object>>;

/// Reads a whole document: sections, objects, object streams, trailer.
///
/// Recovery is not a separate mode. When the cross-reference is unusable the file is
/// scanned for `N G obj` instead, and that substitution is recorded rather than
/// silently applied.
pub fn load_document(bytes: &[u8]) -> PdfResult<RawDocument> {
    let mut decisions = DecisionLog::default();

    let header = xref::find_header(bytes);
    let version = header.as_ref().map_or_else(
        || {
            decisions.push(Decision::violation(
                "7.5.2",
                "no %PDF- header within the first 1024 bytes",
                "assumed 1.7 and read the file anyway",
            ));
            "1.7".to_string()
        },
        |h| h.version.clone(),
    );
    // Offsets in a file with a prefix are relative to the header (7.5.2).
    let base = header.as_ref().map_or(0, |h| h.offset);

    let arena = PdfArena::new();
    let (records, trailer) = locate_objects(bytes, base, &arena, &mut decisions);
    populate_arena(bytes, base, &records, &arena, &mut decisions);

    Ok(RawDocument { arena, version, trailer, decisions })
}

/// Collects every cross-reference record, falling back to a scan of the file.
fn locate_objects(
    bytes: &[u8],
    base: usize,
    arena: &PdfArena,
    decisions: &mut DecisionLog,
) -> (BTreeMap<u32, XrefRecord>, Option<DictHandle>) {
    let mut records = BTreeMap::new();
    let mut trailer = None;

    if let Some(start) = xref::find_startxref(bytes) {
        // Oldest first, so a later section overrides an earlier definition.
        for at in xref::section_chain(bytes, start.saturating_add(base as u64)) {
            let Ok(offset) = usize::try_from(at) else { continue };
            if let Ok(section) = read_xref_section(bytes, offset, arena, decisions) {
                records.extend(section.entries);
                trailer = section.trailer.or(trailer);
            }
        }
    }

    if records.is_empty() {
        let scanned = xref::scan_indirect_objects(bytes);
        decisions.push(Decision::repaired(
            "7.5.4",
            "the cross-reference could not be read",
            format!("scanned the file and found {} objects", scanned.len()),
        ));
        records.extend(
            scanned.into_iter().map(|(n, o)| (n, XrefRecord::InFile { offset: o, generation: 0 })),
        );
    }
    (records, trailer)
}

/// Places every object in the arena at the slot matching its number.
fn populate_arena(
    bytes: &[u8],
    base: usize,
    records: &BTreeMap<u32, XrefRecord>,
    arena: &PdfArena,
    decisions: &mut DecisionLog,
) {
    let highest = records.keys().copied().max().unwrap_or(0);
    for _ in 0..=highest {
        arena.alloc_object(Object::Null);
    }

    let mut containers = BTreeSet::new();
    for (&number, record) in records {
        match record {
            XrefRecord::InFile { offset, .. } => {
                place_direct(bytes, base, number, *offset, arena, decisions);
            }
            XrefRecord::InObjectStream { container, .. } => {
                containers.insert(*container);
            }
            XrefRecord::Free { .. } => {}
        }
    }

    for container in containers {
        place_from_container(bytes, base, container, records, arena, decisions);
    }
}

/// Parses one object written directly in the file and stores it under its number.
fn place_direct(
    bytes: &[u8],
    base: usize,
    number: u32,
    offset: u64,
    arena: &PdfArena,
    decisions: &mut DecisionLog,
) {
    let Ok(at) = usize::try_from(offset) else { return };
    match parse_indirect_at(bytes, at.saturating_add(base), arena, decisions) {
        Ok(parsed) => {
            if parsed.number != number {
                // The cross-reference and the file disagree; the object header is
                // what the bytes actually say, so it wins.
                decisions.push(Decision::repaired(
                    "7.5.4",
                    format!(
                        "cross-reference lists object {number} at an offset holding object {}",
                        parsed.number
                    ),
                    "trusted the object header over the cross-reference",
                ));
            }
            arena.set_object(Handle::new(parsed.number), parsed.object);
        }
        Err(e) => decisions.push(Decision::violation(
            "7.5.4",
            format!("object {number} at offset {offset} did not parse: {e}"),
            "left the slot null",
        )),
    }
}

/// Expands one object stream and stores everything it carried.
fn place_from_container(
    bytes: &[u8],
    base: usize,
    container: u32,
    records: &BTreeMap<u32, XrefRecord>,
    arena: &PdfArena,
    decisions: &mut DecisionLog,
) {
    let Some(offset) = records.get(&container).and_then(XrefRecord::offset) else {
        return;
    };
    let Ok(at) = usize::try_from(offset) else { return };
    let Ok(parsed) = parse_indirect_at(bytes, at.saturating_add(base), arena, decisions) else {
        decisions.push(Decision::violation(
            "7.5.7",
            format!("object stream {container} did not parse"),
            "the objects it carried are unavailable",
        ));
        return;
    };
    match expand_object_stream(&parsed.object, arena, decisions) {
        Ok(inner) => {
            for object in inner {
                // A later revision may have replaced one of these with a direct object
                // (7.5.6). The merged cross-reference says where each object now lives,
                // and a container must not overwrite what it no longer holds.
                if current_container(records, object.number) == Some(container) {
                    arena.set_object(Handle::new(object.number), object.object);
                }
            }
        }
        Err(e) => decisions.push(Decision::violation(
            "7.5.7",
            format!("object stream {container} could not be expanded: {e}"),
            "the objects it carried are unavailable",
        )),
    }
}

/// The object stream the newest cross-reference entry places an object in.
fn current_container(records: &BTreeMap<u32, XrefRecord>, number: u32) -> Option<u32> {
    match records.get(&number)? {
        XrefRecord::InObjectStream { container, .. } => Some(*container),
        XrefRecord::InFile { .. } | XrefRecord::Free { .. } => None,
    }
}

/// A cross-reference section, whichever form the file used.
#[derive(Debug, Clone)]
pub struct XrefSection {
    /// Where each object lives.
    pub entries: BTreeMap<u32, XrefRecord>,
    /// The trailer dictionary. For a cross-reference stream this is the stream's own
    /// dictionary, which is why reading one has to happen at this layer.
    pub trailer: Option<DictHandle>,
}

/// Reads whichever cross-reference form sits at `offset`.
///
/// A classic table is pure bytes and handled by the syntax layer; a cross-reference
/// stream is an indirect object whose dictionary carries `/W` and `/Index` and whose
/// payload is filtered, so it can only be read once objects and filters exist.
pub fn read_xref_section(
    bytes: &[u8],
    offset: usize,
    arena: &PdfArena,
    decisions: &mut DecisionLog,
) -> PdfResult<XrefSection> {
    if let Ok(table) = fepdf_syntax::xref::parse_xref_table(bytes, offset) {
        let trailer = table.trailer_at.and_then(|at| parse_trailer_dict(bytes, at, arena));
        return Ok(XrefSection { entries: table.entries, trailer });
    }

    let indirect = parse_indirect_at(bytes, offset, arena, decisions)?;
    let Object::Stream(dict_h, data) = &indirect.object else {
        return Err(PdfError::Parse {
            pos: offset,
            message: "cross-reference section is neither a table nor a stream".into(),
        });
    };
    let Some(dict) = arena.get_dict(*dict_h) else {
        return Err(PdfError::Arena("cross-reference stream has no dictionary".into()));
    };

    let layout = stream_layout(&dict, arena).ok_or_else(|| PdfError::Parse {
        pos: offset,
        message: "cross-reference stream has no usable /W".into(),
    })?;
    let raw = arena.get_stream_bytes(data).unwrap_or_default();
    let decoded = crate::filters::process_arena_filters(&raw, &dict, arena)?;
    let entries = fepdf_syntax::xref::parse_xref_stream_data(&decoded, &layout)?;

    Ok(XrefSection { entries, trailer: Some(*dict_h) })
}

/// Reads `/W` and `/Index` out of a cross-reference stream's dictionary.
fn stream_layout(
    dict: &BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>,
    arena: &PdfArena,
) -> Option<fepdf_syntax::xref::XrefStreamLayout> {
    let widths: Vec<usize> = integer_array(dict.get(&arena.name("W"))?, arena)?;
    if widths.len() != 3 {
        return None;
    }
    let widths = [widths[0], widths[1], widths[2]];

    // 7.5.8.2: /Index defaults to one subsection covering 0..Size.
    let index = match dict.get(&arena.name("Index")).and_then(|o| integer_array(o, arena)) {
        Some(flat) if flat.len() >= 2 => flat
            .chunks_exact(2)
            .filter_map(|p| Some((u32::try_from(p[0]).ok()?, u32::try_from(p[1]).ok()?)))
            .collect(),
        _ => {
            let size = integer_entry(dict, arena, "Size").unwrap_or(0);
            vec![(0, u32::try_from(size).unwrap_or(0))]
        }
    };
    Some(fepdf_syntax::xref::XrefStreamLayout { widths, index })
}

/// Reads an array of integers, resolving it through the arena.
fn integer_array(object: &Object, arena: &PdfArena) -> Option<Vec<usize>> {
    let Object::Array(h) = object.resolve(arena) else { return None };
    Some(
        arena
            .get_array(h)?
            .iter()
            .filter_map(|o| match o.resolve(arena) {
                Object::Integer(n) => usize::try_from(n).ok(),
                _ => None,
            })
            .collect(),
    )
}

/// Parses the dictionary following a `trailer` keyword.
fn parse_trailer_dict(
    bytes: &[u8],
    at: usize,
    arena: &PdfArena,
) -> Option<crate::handle::Handle<BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>>>
{
    let mut parser = Parser::new(Bytes::copy_from_slice(&bytes[at.min(bytes.len())..]), arena);
    match parser.parse_object().ok()? {
        Object::Dictionary(h) => Some(h),
        _ => None,
    }
}

/// Expands an object stream (ISO 32000-2 7.5.7) into the objects it carries.
///
/// The stream begins with `/N` pairs of `object-number offset`, then the objects
/// themselves at `/First + offset`. The pairs are read rather than trusted against
/// `/N`: a producer that miscounts still wrote the pairs it wrote.
pub fn expand_object_stream(
    stream: &Object,
    arena: &PdfArena,
    decisions: &mut DecisionLog,
) -> PdfResult<Vec<IndirectObject>> {
    let Object::Stream(dict_h, data) = stream else {
        return Err(PdfError::Parse {
            pos: 0,
            message: "object stream expansion needs a stream object".into(),
        });
    };
    let Some(dict) = arena.get_dict(*dict_h) else {
        return Err(PdfError::Arena("object stream has no dictionary".into()));
    };

    let raw = arena.get_stream_bytes(data).unwrap_or_default();
    let decoded = crate::filters::process_arena_filters(&raw, &dict, arena)?;

    let first = integer_entry(&dict, arena, "First")
        .ok_or_else(|| PdfError::Parse { pos: 0, message: "object stream has no /First".into() })?;
    let declared = integer_entry(&dict, arena, "N");

    let pairs = read_pairs(&decoded, first);
    if let Some(n) = declared
        && n != pairs.len()
    {
        decisions.push(Decision::repaired(
            "7.5.7",
            format!("/N says {n} objects but {} pairs are present", pairs.len()),
            "used the pairs actually written",
        ));
    }

    Ok(build_stream_objects(&decoded, first, &pairs, arena, decisions))
}

/// Parses each object a stream's pair table points at, recording what it cannot read.
fn build_stream_objects(
    decoded: &[u8],
    first: usize,
    pairs: &[(u32, usize)],
    arena: &PdfArena,
    decisions: &mut DecisionLog,
) -> Vec<IndirectObject> {
    let mut objects = Vec::with_capacity(pairs.len());
    for &(number, at) in pairs {
        let start = first.saturating_add(at);
        if start >= decoded.len() {
            decisions.push(Decision::violation(
                "7.5.7",
                format!("object {number} is listed at offset {at}, past the end of the stream"),
                "skipped it",
            ));
            continue;
        }
        let mut parser = Parser::new(Bytes::copy_from_slice(&decoded[start..]), arena);
        match parser.parse_object() {
            // 7.5.7: an object inside an object stream always has generation 0.
            Ok(object) => objects.push(IndirectObject { number, generation: 0, object }),
            Err(e) => decisions.push(Decision::violation(
                "7.5.7",
                format!("object {number} inside an object stream did not parse: {e}"),
                "skipped it",
            )),
        }
    }
    objects
}

/// Reads the `number offset` pairs preceding `first`.
fn read_pairs(decoded: &[u8], first: usize) -> Vec<(u32, usize)> {
    let head = &decoded[..first.min(decoded.len())];
    let numbers: Vec<u64> = String::from_utf8_lossy(head)
        .split_ascii_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    numbers
        .chunks_exact(2)
        .filter_map(|p| Some((u32::try_from(p[0]).ok()?, usize::try_from(p[1]).ok()?)))
        .collect()
}

/// Reads an integer entry from a dictionary.
fn integer_entry(
    dict: &BTreeMap<crate::handle::Handle<crate::object::PdfName>, Object>,
    arena: &PdfArena,
    key: &str,
) -> Option<usize> {
    match dict.get(&arena.name(key))? {
        Object::Integer(n) => usize::try_from(*n).ok(),
        _ => None,
    }
}

/// Reads the next token as an unsigned integer.
fn expect_unsigned(lexer: &mut Lexer, at: usize) -> PdfResult<i64> {
    match lexer.next_token()? {
        Token::Integer(n) if n >= 0 => Ok(n),
        other => Err(PdfError::Parse {
            pos: at,
            message: format!("expected an object number, found {other:?}").into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(src: &[u8]) -> (IndirectObject, DecisionLog) {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        let obj = parse_indirect_at(src, 0, &arena, &mut log).expect("should parse");
        (obj, log)
    }

    #[test]
    fn a_plain_object_carries_its_number_and_generation() {
        let (obj, log) = read(b"12 3 obj\n42\nendobj\n");
        assert_eq!(obj.number, 12);
        assert_eq!(obj.generation, 3);
        assert_eq!(obj.object, Object::Integer(42));
        assert!(log.is_conforming(), "a well-formed object needs no decisions");
    }

    #[test]
    fn a_stream_with_a_correct_length_is_read_without_comment() {
        let (obj, log) = read(b"1 0 obj\n<< /Length 5 >>\nstream\nHELLO\nendstream\nendobj\n");
        let Object::Stream(_, data) = obj.object else { panic!("expected a stream") };
        let SublimatedData::Raw(bytes) = data.as_ref() else { panic!("expected raw data") };
        assert_eq!(&bytes[..], b"HELLO");
        assert!(log.is_conforming());
    }

    #[test]
    fn a_wrong_length_is_repaired_and_recorded() {
        // The most common real-world non-conformance: /Length disagrees with the data.
        let (obj, log) = read(b"1 0 obj\n<< /Length 2 >>\nstream\nHELLO\nendstream\nendobj\n");
        let Object::Stream(_, data) = obj.object else { panic!("expected a stream") };
        let SublimatedData::Raw(bytes) = data.as_ref() else { panic!("expected raw data") };
        assert_eq!(&bytes[..], b"HELLO", "the scanned extent wins over a wrong /Length");
        assert_eq!(log.entries().len(), 1);
        assert!(log.entries()[0].found.contains("/Length 2"));
    }

    #[test]
    fn an_indirect_length_is_resolved_by_scanning() {
        // /Length may reference an object that is not loaded yet, so the data is
        // delimited by scanning; that is a reading chosen, not an error.
        let (obj, log) = read(b"1 0 obj\n<< /Length 9 0 R >>\nstream\nHELLO\nendstream\nendobj\n");
        let Object::Stream(_, data) = obj.object else { panic!("expected a stream") };
        let SublimatedData::Raw(bytes) = data.as_ref() else { panic!("expected raw data") };
        assert_eq!(&bytes[..], b"HELLO");
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].severity, crate::interpretation::Severity::Ambiguity);
    }

    #[test]
    fn a_lone_carriage_return_after_stream_is_accepted() {
        // 7.3.8.1 requires CRLF or LF; producers emit a bare CR and the intent is
        // unambiguous, so it is accepted.
        let (obj, _) = read(b"1 0 obj\n<< /Length 5 >>\nstream\rHELLO\nendstream\nendobj\n");
        let Object::Stream(_, data) = obj.object else { panic!("expected a stream") };
        let SublimatedData::Raw(bytes) = data.as_ref() else { panic!("expected raw data") };
        assert_eq!(&bytes[..], b"HELLO");
    }

    #[test]
    fn a_stream_with_no_endstream_and_no_length_is_a_violation() {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        let src = b"1 0 obj\n<< /Foo 1 >>\nstream\nHELLO";
        let obj = parse_indirect_at(src, 0, &arena, &mut log).expect("should still parse");
        let Object::Stream(..) = obj.object else { panic!("expected a stream") };
        assert_eq!(log.entries()[0].severity, crate::interpretation::Severity::Violation);
    }

    /// Builds an uncompressed `/Type /ObjStm` carrying `objects`.
    fn object_stream(arena: &PdfArena, objects: &[(u32, &str)], declared_n: Option<i64>) -> Object {
        let mut body = String::new();
        let mut pairs = String::new();
        for (number, text) in objects {
            pairs.push_str(&format!("{number} {} ", body.len()));
            body.push_str(text);
            body.push(' ');
        }
        let first = pairs.len();
        let data = format!("{pairs}{body}");

        let mut dict = BTreeMap::new();
        dict.insert(
            arena.name("Type"),
            Object::Name(arena.intern_name(crate::object::PdfName::new("ObjStm"))),
        );
        dict.insert(arena.name("First"), Object::Integer(first as i64));
        if let Some(n) = declared_n {
            dict.insert(arena.name("N"), Object::Integer(n));
        }
        let dict_h = arena.alloc_dict(dict);
        Object::Stream(
            dict_h,
            std::sync::Arc::new(SublimatedData::Raw(Bytes::from(data.into_bytes()))),
        )
    }

    #[test]
    fn an_object_stream_yields_the_objects_it_carries() {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        let stream = object_stream(&arena, &[(4, "42"), (7, "true")], Some(2));

        let objects = expand_object_stream(&stream, &arena, &mut log).unwrap();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].number, 4);
        assert_eq!(objects[0].object, Object::Integer(42));
        assert_eq!(objects[1].number, 7);
        assert_eq!(objects[1].object, Object::Boolean(true));
        assert!(log.is_conforming());
    }

    #[test]
    fn objects_in_a_stream_have_generation_zero() {
        // 7.5.7: an object in an object stream always has generation 0, so a file
        // claiming otherwise cannot be honoured.
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        let stream = object_stream(&arena, &[(4, "1")], Some(1));
        let objects = expand_object_stream(&stream, &arena, &mut log).unwrap();
        assert_eq!(objects[0].generation, 0);
    }

    #[test]
    fn a_miscounted_n_is_repaired_towards_what_was_written() {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        let stream = object_stream(&arena, &[(4, "42"), (7, "1")], Some(9));

        let objects = expand_object_stream(&stream, &arena, &mut log).unwrap();
        assert_eq!(objects.len(), 2, "the pairs actually present win over /N");
        assert_eq!(log.entries().len(), 1);
        assert!(log.entries()[0].found.contains("/N says 9"));
    }

    #[test]
    fn an_entry_pointing_past_the_stream_is_recorded_and_skipped() {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        let mut dict = BTreeMap::new();
        dict.insert(arena.name("First"), Object::Integer(6));
        dict.insert(arena.name("N"), Object::Integer(1));
        let dict_h = arena.alloc_dict(dict);
        let stream = Object::Stream(
            dict_h,
            std::sync::Arc::new(SublimatedData::Raw(Bytes::from_static(b"4 900 42"))),
        );

        let objects = expand_object_stream(&stream, &arena, &mut log).unwrap();
        assert!(objects.is_empty());
        assert_eq!(log.entries()[0].severity, crate::interpretation::Severity::Violation);
    }

    #[test]
    fn a_stream_without_first_is_refused() {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        let dict_h = arena.alloc_dict(BTreeMap::new());
        let stream = Object::Stream(
            dict_h,
            std::sync::Arc::new(SublimatedData::Raw(Bytes::from_static(b""))),
        );
        assert!(expand_object_stream(&stream, &arena, &mut log).is_err());
    }

    #[test]
    fn an_offset_past_the_end_is_refused() {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        assert!(parse_indirect_at(b"1 0 obj\n1\nendobj", 900, &arena, &mut log).is_err());
    }

    #[test]
    fn a_non_object_at_the_offset_is_refused() {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        assert!(parse_indirect_at(b"trailer\n<<>>", 0, &arena, &mut log).is_err());
    }

    /// Builds a file whose second revision replaces object 2 and deletes object 3.
    ///
    /// Object 2 first lives in an object stream, which is what made the ordering bug
    /// possible: expanding the container after reading the newer direct object put the
    /// superseded version back.
    fn incrementally_updated() -> Vec<u8> {
        let mut out = Vec::new();
        let push = |out: &mut Vec<u8>, s: &str| out.extend_from_slice(s.as_bytes());

        push(&mut out, "%PDF-2.0\n");
        // Revision 1: an object stream carrying objects 2 and 3.
        let payload = "2 0 3 12 << /V (old) >> << /V (three) >>";
        let stm_at = out.len();
        push(
            &mut out,
            &format!(
                "4 0 obj\n<< /Type /ObjStm /N 2 /First 8 /Length {} >>\nstream\n{payload}\nendstream\nendobj\n",
                payload.len()
            ),
        );
        let first_xref = out.len();
        push(&mut out, "xref\n0 5\n0000000000 65535 f \n0000000000 65535 f \n");
        push(&mut out, "0000000000 00000 n \n0000000000 00000 n \n");
        push(&mut out, &format!("{stm_at:010} 00000 n \n"));
        push(&mut out, &format!("trailer\n<< /Size 5 >>\nstartxref\n{first_xref}\n%%EOF\n"));

        // Revision 2: object 2 rewritten in the file, object 3 freed.
        let new_two = out.len();
        push(&mut out, "2 0 obj\n<< /V (new) >>\nendobj\n");
        let second_xref = out.len();
        push(&mut out, "xref\n0 1\n0000000000 65535 f \n2 2\n");
        push(&mut out, &format!("{new_two:010} 00000 n \n0000000000 00001 f \n"));
        push(
            &mut out,
            &format!(
                "trailer\n<< /Size 5 /Prev {first_xref} >>\nstartxref\n{second_xref}\n%%EOF\n"
            ),
        );
        out
    }

    /// Reads `/V` from the object at `number`, if it is a dictionary holding one.
    fn value_of(arena: &PdfArena, number: u32) -> Option<String> {
        let Object::Dictionary(d) = arena.get_object(Handle::new(number))? else { return None };
        match arena.get_dict(d)?.get(&arena.name("V"))? {
            Object::String(b) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        }
    }

    #[test]
    fn a_newer_revision_wins_over_the_object_stream_that_first_held_it() {
        let bytes = incrementally_updated();
        let doc = load_document(&bytes).expect("the file should read");
        assert_eq!(value_of(&doc.arena, 2).as_deref(), Some("new"));
    }

    #[test]
    fn an_object_freed_by_a_later_revision_is_not_resurrected() {
        let bytes = incrementally_updated();
        let doc = load_document(&bytes).expect("the file should read");
        assert!(matches!(doc.arena.get_object(Handle::new(3)), Some(Object::Null) | None));
    }

    #[test]
    fn an_object_handle_is_its_object_number() {
        let bytes = incrementally_updated();
        let doc = load_document(&bytes).expect("the file should read");
        // Object 4 is the container; it stays addressable under its own number.
        assert!(matches!(doc.arena.get_object(Handle::new(4)), Some(Object::Stream(_, _))));
    }

    #[test]
    fn a_length_that_disagrees_with_endstream_is_repaired_and_recorded() {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        // /Length says 3, but the data is five bytes.
        let bytes = b"1 0 obj\n<< /Length 3 >>\nstream\nabcde\nendstream\nendobj\n";
        let parsed = parse_indirect_at(bytes, 0, &arena, &mut log).expect("parses");
        let Object::Stream(_, data) = parsed.object else { panic!("expected a stream") };
        let SublimatedData::Raw(payload) = data.as_ref() else { panic!("expected raw data") };
        assert_eq!(&payload[..], b"abcde");
        assert!(log.entries().iter().any(|d| d.clause == "7.3.8.2"));
    }

    #[test]
    fn a_length_consistent_with_endstream_is_believed_over_the_scan() {
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        // The data ends in CR, so scanning back over the EOL would eat one real byte.
        let bytes = b"1 0 obj\n<< /Length 3 >>\nstream\nab\r\nendstream\nendobj\n";
        let parsed = parse_indirect_at(bytes, 0, &arena, &mut log).expect("parses");
        let Object::Stream(_, data) = parsed.object else { panic!("expected a stream") };
        let SublimatedData::Raw(payload) = data.as_ref() else { panic!("expected raw data") };
        assert_eq!(&payload[..], b"ab\r");
        assert!(log.entries().is_empty(), "a consistent /Length needs no decision");
    }
}
