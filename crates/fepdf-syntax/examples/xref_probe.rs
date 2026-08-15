//! Probes the new xref layer against real files.
use fepdf_syntax::xref;

fn main() {
    for path in std::env::args().skip(1) {
        let data = std::fs::read(&path).unwrap_or_default();
        let h = xref::find_header(&data);
        let sx = xref::find_startxref(&data);
        let scanned = xref::scan_indirect_objects(&data).len();
        let table = sx
            .and_then(|o| usize::try_from(o).ok())
            .filter(|&o| o < data.len())
            .and_then(|o| xref::parse_xref_table(&data, o).ok())
            .map(|t| t.entries.len());
        println!(
            "{:<26} header={:<18} startxref={:<10} table={:<8} scanned={}",
            path.rsplit('/').next().unwrap_or(&path),
            h.map_or_else(|| "なし".to_string(), |h| format!("{}@{}", h.version, h.offset)),
            sx.map_or_else(|| "なし".to_string(), |v| v.to_string()),
            table.map_or_else(|| "—".to_string(), |n| n.to_string()),
            scanned
        );
    }
}
