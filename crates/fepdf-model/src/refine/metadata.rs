//! Metadata Refinement: Conversion of /Info to XMP using xmp-writer.

use crate::object::PdfName;
use crate::refine::RefinedObject;
use bytes::Bytes;
use std::collections::BTreeMap;
use xmp_writer::XmpWriter;

fn parse_tz_part(tz: char, tz_part: &str) -> Option<xmp_writer::Timezone> {
    if tz == 'Z' {
        Some(xmp_writer::Timezone::Utc)
    } else {
        let sign = if tz == '+' { 1 } else { -1 };
        let tz_digits: String = tz_part.chars().filter(|c| c.is_ascii_digit()).collect();
        if tz_digits.len() >= 2 {
            let tz_h = tz_digits[0..2].parse::<i8>().ok().map(|h| h * sign);
            let mut tz_min = 0;
            if tz_digits.len() >= 4
                && let Ok(m) = tz_digits[2..4].parse::<i8>()
            {
                tz_min = m;
            }
            tz_h.map(|h| xmp_writer::Timezone::Local { hour: h, minute: tz_min })
        } else {
            None
        }
    }
}

fn parse_legacy_pdf_date(s: &str) -> Option<xmp_writer::DateTime> {
    let mut clean_s = s;
    if clean_s.starts_with("D:") {
        clean_s = &clean_s[2..];
    }

    let mut digits = String::new();
    let mut tz_char = None;
    let mut tz_part = "";

    for (i, c) in clean_s.char_indices() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else if c == 'Z' || c == '+' || c == '-' {
            tz_char = Some(c);
            tz_part = &clean_s[i..];
            break;
        }
    }

    if digits.len() < 4 {
        return None;
    }

    let year = digits[0..4].parse::<u16>().ok()?;
    let month = if digits.len() >= 6 { digits[4..6].parse::<u8>().ok() } else { None };
    let day = if digits.len() >= 8 { digits[6..8].parse::<u8>().ok() } else { None };
    let hour = if digits.len() >= 10 { digits[8..10].parse::<u8>().ok() } else { None };
    let minute = if digits.len() >= 12 { digits[10..12].parse::<u8>().ok() } else { None };
    let second = if digits.len() >= 14 { digits[12..14].parse::<u8>().ok() } else { None };

    let timezone = tz_char.and_then(|tz| parse_tz_part(tz, tz_part));

    Some(xmp_writer::DateTime { year, month, day, hour, minute, second, timezone })
}

fn parse_iso8601_date(s: &str) -> Option<xmp_writer::DateTime> {
    let year = s[0..4].parse::<u16>().ok()?;
    let month = s[5..7].parse::<u8>().ok();
    let day = s[8..10].parse::<u8>().ok();

    let mut hour = None;
    let mut minute = None;
    let mut second = None;
    let mut timezone = None;

    if s.len() >= 16 && (s.chars().nth(10) == Some('T') || s.chars().nth(10) == Some(' ')) {
        hour = s[11..13].parse::<u8>().ok();
        minute = s[14..16].parse::<u8>().ok();

        let mut rest = &s[16..];
        if rest.starts_with(':') && rest.len() >= 3 {
            second = rest[1..3].parse::<u8>().ok();
            rest = &rest[3..];
        }

        if !rest.is_empty() {
            if rest.starts_with('Z') {
                timezone = Some(xmp_writer::Timezone::Utc);
            } else if rest.starts_with('+') || rest.starts_with('-') {
                let sign = if rest.starts_with('+') { 1 } else { -1 };
                let tz_digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
                if tz_digits.len() >= 2 {
                    let tz_h = tz_digits[0..2].parse::<i8>().ok().map(|h| h * sign);
                    let mut tz_min = 0;
                    if tz_digits.len() >= 4
                        && let Ok(m) = tz_digits[2..4].parse::<i8>()
                    {
                        tz_min = m;
                    }
                    if let Some(h) = tz_h {
                        timezone = Some(xmp_writer::Timezone::Local { hour: h, minute: tz_min });
                    }
                }
            }
        }
    }

    Some(xmp_writer::DateTime { year, month, day, hour, minute, second, timezone })
}

/// Parses a legacy PDF date string (e.g. "D:20031003221948+09'00'") or a standard ISO 8601
/// date string into a `xmp_writer::DateTime`.
pub fn parse_date_string(s: &str) -> Option<xmp_writer::DateTime> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // 1. Check if it's a legacy PDF date (starts with D: or contains only digits and offset indicators)
    if s.starts_with("D:")
        || (s.len() >= 4 && s.chars().take(4).all(|c| c.is_ascii_digit()) && !s.contains('-'))
    {
        return parse_legacy_pdf_date(s);
    }

    // 2. Otherwise try parsing as ISO 8601 (e.g., "YYYY-MM-DDTHH:mm:ssZ" or "YYYY-MM-DDTHH:mm:ss+HH:mm")
    if s.len() >= 10 && s.chars().nth(4) == Some('-') && s.chars().nth(7) == Some('-') {
        return parse_iso8601_date(s);
    }

    None
}

fn get_info_field(info: &BTreeMap<PdfName, RefinedObject>, key: &str) -> Option<String> {
    info.get(&PdfName::new(key))
        .map(|obj| match obj {
            RefinedObject::Text(s) => s.clone(),
            RefinedObject::String(s) | RefinedObject::Hex(s) => {
                crate::refine::text::recover_string(s)
            }
            _ => String::new(),
        })
        .filter(|s| !s.is_empty())
}

fn write_basic_fields(info: &BTreeMap<PdfName, RefinedObject>, writer: &mut XmpWriter) {
    if let Some(val) = get_info_field(info, "Title") {
        writer.title([(None, val.as_str())]);
    }
    if let Some(val) = get_info_field(info, "Author") {
        writer.creator([val.as_str()]);
    }
    if let Some(val) = get_info_field(info, "Subject") {
        writer.description([(None, val.as_str())]);
    }
    if let Some(val) = get_info_field(info, "Keywords") {
        writer.pdf_keywords(val.as_str());
    }
    if let Some(val) = get_info_field(info, "Creator") {
        writer.creator_tool(val.as_str());
    }
    if let Some(val) = get_info_field(info, "Producer") {
        writer.producer(val.as_str());
    }
    if let Some(val) = get_info_field(info, "Rights") {
        writer.rights([(None, val.as_str())]);
    }
}

fn generate_and_write_uuids(
    info: &BTreeMap<PdfName, RefinedObject>,
    source_id: Option<&str>,
    stamped_at: u64,
    writer: &mut XmpWriter,
) {
    // RR-15 Limit: Dispatcher - hashes metadata elements to generate unique Document and Instance UUIDs in XMP format
    //
    // **The document is this save's** (ADR-0012), so its ID is drawn from what makes it
    // one: the document it was derived from, everything its metadata says, and the moment
    // it was stamped. It was the title's hash alone, so every untitled document shared one
    // ID and two of one title collided (ROADMAP Y-F4). One input stamped at one moment
    // still writes one ID.
    let mut doc_hasher = md5::Context::new();
    // The salt keeps the pre-rename spelling: it names the hash, not the product.
    doc_hasher.consume(b"ferruginous-pdf2.0-stable-document-id-salt");
    doc_hasher.consume(source_id.unwrap_or_default().as_bytes());
    for key in info.keys() {
        if let Some(text) = get_info_field(info, key.as_str()) {
            doc_hasher.consume(key.as_str().as_bytes());
            doc_hasher.consume(text.as_bytes());
        }
    }
    doc_hasher.consume(stamped_at.to_be_bytes());
    let doc_bytes = doc_hasher.finalize().0;

    // The instance is this rendition, so it is salted with the time the rendition is
    // stamped with. That time is the caller's to give, which is what lets one input
    // written twice at the same stamp come out byte for byte the same.
    let mut inst_hasher = md5::Context::new();
    inst_hasher.consume(doc_bytes);
    inst_hasher.consume(stamped_at.to_be_bytes());
    let inst_bytes = inst_hasher.finalize().0;

    let doc_uuid = format!(
        "uuid:{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        doc_bytes[0],
        doc_bytes[1],
        doc_bytes[2],
        doc_bytes[3],
        doc_bytes[4],
        doc_bytes[5],
        doc_bytes[6],
        doc_bytes[7],
        doc_bytes[8],
        doc_bytes[9],
        doc_bytes[10],
        doc_bytes[11],
        doc_bytes[12],
        doc_bytes[13],
        doc_bytes[14],
        doc_bytes[15]
    );
    let inst_uuid = format!(
        "uuid:{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        inst_bytes[0],
        inst_bytes[1],
        inst_bytes[2],
        inst_bytes[3],
        inst_bytes[4],
        inst_bytes[5],
        inst_bytes[6],
        inst_bytes[7],
        inst_bytes[8],
        inst_bytes[9],
        inst_bytes[10],
        inst_bytes[11],
        inst_bytes[12],
        inst_bytes[13],
        inst_bytes[14],
        inst_bytes[15]
    );

    writer.document_id(&doc_uuid);
    writer.instance_id(&inst_uuid);
}

fn parse_and_write_dates(info: &BTreeMap<PdfName, RefinedObject>, writer: &mut XmpWriter) {
    // **A date nobody stated is not written.** A document with none was given
    // 2026-05-26T06:00:00Z as all three, and one with only a creation date was given it as
    // its modification too (ROADMAP Y-F5). The modification is the save's, which the
    // caller supplies as `ModDate`; `MetadataDate` is the same moment, since the packet is
    // written then.
    if let Some(created) = get_info_field(info, "CreationDate").and_then(|v| parse_date_string(&v))
    {
        writer.create_date(created);
    }
    if let Some(modified) = get_info_field(info, "ModDate").and_then(|v| parse_date_string(&v)) {
        writer.modify_date(modified);
        writer.metadata_date(modified);
    }
}

/// Renders the packet, recording what the document was derived from.
///
/// Saving produces a new document, not an edited one (ADR-0012): the arena already
/// differs from the file by the time anything is written, and no path produces a
/// faithful copy. `xmpMM:DerivedFrom` and `xmpMM:OriginalDocumentID` are how XMP says
/// exactly that, and they outlive a message on a terminal — a reader of the output can
/// tell where it came from without having watched it being made.
///
/// `stamped_at` is the moment the packet speaks for, in seconds since the Unix epoch;
/// [`crate::metadata::seconds_now`] when the caller has no reason to fix it.
pub fn info_to_xmp_derived(
    info: &BTreeMap<PdfName, RefinedObject>,
    provenance: &crate::document::Provenance,
    stamped_at: u64,
) -> String {
    let mut writer = XmpWriter::new();

    write_basic_fields(info, &mut writer);
    writer.format("application/pdf");
    generate_and_write_uuids(info, provenance.source_id.as_deref(), stamped_at, &mut writer);
    parse_and_write_dates(info, &mut writer);

    if let Some(parent) = &provenance.source_id {
        writer.derived_from().document_id(parent);
    }
    if let Some(root) = &provenance.original_id {
        writer.original_doc_id(root);
    }

    writer.finish(None)
}

/// Renders the packet's fields and dates and nothing about which document it is.
///
/// **For a packet written over one the document already has, outside a save**: on opening,
/// and when a declaration is stated. The document is still the one the file is, so its
/// `xmpMM:` identity is the file's, carried over by `xmp_carry::carry_identity`. Rendered
/// with `info_to_xmp_derived` it was drawn from the clock with no derivation, and a file
/// read twice named two documents (ROADMAP Y-F29).
pub fn info_to_xmp_kept(info: &BTreeMap<PdfName, RefinedObject>) -> String {
    let mut writer = XmpWriter::new();
    write_basic_fields(info, &mut writer);
    writer.format("application/pdf");
    parse_and_write_dates(info, &mut writer);
    writer.finish(None)
}

/// Creates a RefinedObject representing the Metadata stream.
pub fn create_metadata_stream(xmp: String) -> RefinedObject {
    let mut dict = BTreeMap::new();
    dict.insert(PdfName::new("Type"), RefinedObject::Name(PdfName::new("Metadata")));
    dict.insert(PdfName::new("Subtype"), RefinedObject::Name(PdfName::new("XML")));

    RefinedObject::Stream(dict, Bytes::from(xmp))
}

#[cfg(test)]
mod provenance {
    //! What the output records about where it came from (ADR-0012).

    use super::*;
    use crate::document::Provenance;

    fn packet(provenance: &Provenance) -> String {
        info_to_xmp_derived(&BTreeMap::new(), provenance, 0)
    }

    #[test]
    fn a_first_save_names_the_source_as_both_parent_and_root() {
        let p = Provenance {
            source_id: Some("uuid:aaa".into()),
            original_id: Some("uuid:aaa".into()),
            signatures: 0,
        };
        let xmp = packet(&p);
        assert!(xmp.contains("DerivedFrom"), "{xmp}");
        assert!(xmp.contains("OriginalDocumentID"), "{xmp}");
        assert_eq!(xmp.matches("uuid:aaa").count(), 2, "parent and root are the same file");
    }

    #[test]
    fn a_later_save_keeps_the_root_and_moves_the_parent() {
        // The defect this guards: writing the parent into both fields loses where the
        // chain began, and two saves are enough to do it.
        let p = Provenance {
            source_id: Some("uuid:middle".into()),
            original_id: Some("uuid:root".into()),
            signatures: 0,
        };
        let xmp = packet(&p);
        let derived = xmp.find("DerivedFrom").expect("present");
        let original = xmp.find("OriginalDocumentID").expect("present");
        assert!(xmp[derived..original].contains("uuid:middle"), "parent is the immediate one");
        assert!(xmp[original..].contains("uuid:root"), "root survives the generation");
        assert!(!xmp[original..].contains("uuid:middle"), "root is not overwritten");
    }

    #[test]
    fn a_document_with_no_source_claims_no_origin() {
        // Nothing to derive from means nothing to say, not an empty element.
        let xmp = packet(&Provenance::default());
        assert!(!xmp.contains("DerivedFrom"), "{xmp}");
        assert!(!xmp.contains("OriginalDocumentID"), "{xmp}");
    }

    #[test]
    fn the_packet_does_not_grow_with_generations() {
        // The record is two single values, not a list: repeated saves overwrite them.
        // A growing packet would be a different design and a worse one for a tool that
        // is run in a loop.
        let first = packet(&Provenance {
            source_id: Some("uuid:aaa".into()),
            original_id: Some("uuid:aaa".into()),
            signatures: 0,
        });
        let tenth = packet(&Provenance {
            source_id: Some("uuid:jjj".into()),
            original_id: Some("uuid:aaa".into()),
            signatures: 0,
        });
        assert_eq!(first.len(), tenth.len());
    }
}
