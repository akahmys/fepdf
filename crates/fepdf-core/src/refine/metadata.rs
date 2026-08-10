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
}

fn generate_and_write_uuids(info: &BTreeMap<PdfName, RefinedObject>, writer: &mut XmpWriter) {
    // RR-15 Limit: Dispatcher - hashes metadata elements to generate unique Document and Instance UUIDs in XMP format
    let mut doc_hasher = md5::Context::new();
    let title_val = get_info_field(info, "Title").unwrap_or_default();
    doc_hasher.consume(title_val.as_bytes());
    // Deliberately keeps the pre-rename spelling. This salt is an input to the XMP
    // DocumentID, so changing it would give every document a different ID and break
    // reproducibility against files produced before the rename. It is an identifier
    // for the hash, not a product name.
    doc_hasher.consume(b"ferruginous-pdf2.0-stable-document-id-salt");
    let doc_bytes = doc_hasher.finalize().0;

    let mut inst_hasher = md5::Context::new();
    inst_hasher.consume(doc_bytes);
    let salt = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    inst_hasher.consume(salt.to_be_bytes());
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
    let mut create_dt = get_info_field(info, "CreationDate").and_then(|v| parse_date_string(&v));
    let mut modify_dt = get_info_field(info, "ModDate").and_then(|v| parse_date_string(&v));

    if create_dt.is_none() {
        create_dt = modify_dt;
    }
    if modify_dt.is_none() {
        modify_dt = create_dt;
    }

    let fallback_dt = xmp_writer::DateTime {
        year: 2026,
        month: Some(5),
        day: Some(26),
        hour: Some(6),
        minute: Some(0),
        second: Some(0),
        timezone: Some(xmp_writer::Timezone::Utc),
    };

    let final_create = create_dt.unwrap_or(fallback_dt);
    let final_modify = modify_dt.unwrap_or(fallback_dt);

    writer.create_date(final_create);
    writer.modify_date(final_modify);
    writer.metadata_date(final_modify);
}

/// Renders an `/Info` dictionary as an XMP packet.
pub fn info_to_xmp(info: &BTreeMap<PdfName, RefinedObject>) -> String {
    let mut writer = XmpWriter::new();

    write_basic_fields(info, &mut writer);
    writer.format("application/pdf");
    generate_and_write_uuids(info, &mut writer);
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
