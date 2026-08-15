//! Reads every object a file's cross-reference names, reporting what had to be decided.
//!
//! A diagnostic for the reader replacing lopdf.

use fepdf_model::arena::PdfArena;
use fepdf_model::interpretation::DecisionLog;
use fepdf_model::reader;
use fepdf_syntax::xref::{self, XrefRecord};

fn main() {
    for path in std::env::args().skip(1) {
        let data = std::fs::read(&path).unwrap_or_default();
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();

        let mut records = std::collections::BTreeMap::new();
        if let Some(start) = xref::find_startxref(&data) {
            for at in xref::section_chain(&data, start) {
                if let Ok(usize_at) = usize::try_from(at)
                    && let Ok(table) = xref::parse_xref_table(&data, usize_at)
                {
                    records.extend(table.entries);
                }
            }
        }
        if records.is_empty() {
            records.extend(
                xref::scan_indirect_objects(&data)
                    .into_iter()
                    .map(|(n, o)| (n, XrefRecord::InFile { offset: o, generation: 0 })),
            );
        }

        let arena = PdfArena::new();
        let mut log = DecisionLog::default();
        let (mut ok, mut failed, mut compressed) = (0u32, 0u32, 0u32);
        for record in records.values() {
            match record {
                XrefRecord::InFile { offset, .. } => {
                    let parsed = usize::try_from(*offset)
                        .ok()
                        .map(|o| reader::parse_indirect_at(&data, o, &arena, &mut log));
                    if matches!(parsed, Some(Ok(_))) {
                        ok += 1;
                    } else {
                        failed += 1;
                    }
                }
                XrefRecord::InObjectStream { .. } => compressed += 1,
                XrefRecord::Free { .. } => {}
            }
        }
        println!(
            "{name:<26} 読めた={ok:<6} 失敗={failed:<5} 圧縮内={compressed:<6} 判断={}",
            log.entries().len()
        );
    }
}
