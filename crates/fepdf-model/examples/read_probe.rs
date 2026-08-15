//! Reads every object a file's cross-reference names, reporting what had to be decided.
//!
//! A diagnostic for the reader: it exercises both cross-reference
//! forms, expands object streams, and prints the interpretation decisions taken.

use fepdf_model::arena::PdfArena;
use fepdf_model::interpretation::DecisionLog;
use fepdf_model::reader;
use fepdf_syntax::xref::{self, XrefRecord};
use std::collections::{BTreeMap, BTreeSet};

fn main() {
    for path in std::env::args().skip(1) {
        let data = std::fs::read(&path).unwrap_or_default();
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        let arena = PdfArena::new();
        let mut log = DecisionLog::default();

        let records = collect_records(&data, &arena, &mut log);
        let (ok, failed) = read_direct(&data, &records, &arena, &mut log);
        let (compressed, expanded) = read_compressed(&data, &records, &arena, &mut log);

        println!(
            "{name:<26} 直接={ok:<6} 失敗={failed:<4} 圧縮内={compressed:<6} 展開={expanded:<6} 判断={}",
            log.entries().len()
        );
    }
}

/// Every cross-reference record, from the section chain or, failing that, a scan.
fn collect_records(
    data: &[u8],
    arena: &PdfArena,
    log: &mut DecisionLog,
) -> BTreeMap<u32, XrefRecord> {
    let mut records = BTreeMap::new();
    if let Some(start) = xref::find_startxref(data) {
        for at in xref::section_chain(data, start) {
            if let Ok(offset) = usize::try_from(at)
                && let Ok(section) = reader::read_xref_section(data, offset, arena, log)
            {
                records.extend(section.entries);
            }
        }
    }
    if records.is_empty() {
        records.extend(
            xref::scan_indirect_objects(data)
                .into_iter()
                .map(|(n, o)| (n, XrefRecord::InFile { offset: o, generation: 0 })),
        );
    }
    records
}

/// Parses every object written directly in the file.
fn read_direct(
    data: &[u8],
    records: &BTreeMap<u32, XrefRecord>,
    arena: &PdfArena,
    log: &mut DecisionLog,
) -> (u32, u32) {
    let (mut ok, mut failed) = (0, 0);
    for record in records.values() {
        let Some(offset) = record.offset().and_then(|o| usize::try_from(o).ok()) else {
            continue;
        };
        if reader::parse_indirect_at(data, offset, arena, log).is_ok() {
            ok += 1;
        } else {
            failed += 1;
        }
    }
    (ok, failed)
}

/// Expands every object stream the cross-reference pointed into.
fn read_compressed(
    data: &[u8],
    records: &BTreeMap<u32, XrefRecord>,
    arena: &PdfArena,
    log: &mut DecisionLog,
) -> (u32, u32) {
    let mut containers = BTreeSet::new();
    let mut compressed = 0;
    for record in records.values() {
        if let XrefRecord::InObjectStream { container, .. } = record {
            containers.insert(*container);
            compressed += 1;
        }
    }

    let mut expanded = 0;
    for container in containers {
        let Some(offset) =
            records.get(&container).and_then(|r| r.offset()).and_then(|o| usize::try_from(o).ok())
        else {
            continue;
        };
        if let Ok(obj) = reader::parse_indirect_at(data, offset, arena, log)
            && let Ok(inner) = reader::expand_object_stream(&obj.object, arena, log)
        {
            expanded += u32::try_from(inner.len()).unwrap_or(0);
        }
    }
    (compressed, expanded)
}
