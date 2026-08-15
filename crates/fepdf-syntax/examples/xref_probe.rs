//! Reports what the byte-level file-structure layer sees in a given PDF.
//!
//! A diagnostic for the reader that is replacing lopdf: it shows which sections
//! parse, and what the recovery scan finds when they do not.

use fepdf_syntax::xref::{self, XrefRecord, XrefStreamLayout};

fn main() {
    for path in std::env::args().skip(1) {
        let data = std::fs::read(&path).unwrap_or_default();
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();

        let header = xref::find_header(&data);
        let startxref = xref::find_startxref(&data);
        let scanned = xref::scan_indirect_objects(&data).len();

        let section = startxref
            .and_then(|o| usize::try_from(o).ok())
            .filter(|&o| o < data.len())
            .map_or_else(|| "オフセット不正".to_string(), |o| describe_section(&data, o));

        let chain = startxref.map_or_else(Vec::new, |o| xref::section_chain(&data, o));
        println!(
            "{name:<26} header={:<14} startxref={:<10} sections={:<2} section={section:<26} scanned={scanned}",
            header.map_or_else(|| "なし".to_string(), |h| format!("{}@{}", h.version, h.offset)),
            startxref.map_or_else(|| "なし".to_string(), |v| v.to_string()),
            chain.len(),
        );
    }
}

/// Describes whichever cross-reference form sits at `at`.
fn describe_section(data: &[u8], at: usize) -> String {
    if let Ok(table) = xref::parse_xref_table(data, at) {
        let free = table.entries.values().filter(|r| !r.is_in_use()).count();
        return format!("table {} entries ({free} free)", table.entries.len());
    }

    // A cross-reference stream begins with an indirect object header. Reading its
    // dictionary needs the object parser, so only report the shape from here.
    let head = &data[at..data.len().min(at + 512)];
    if find(head, b"/XRef").is_some() {
        let widths = read_w_array(head);
        return widths.map_or_else(
            || "xref stream (/W unreadable)".to_string(),
            |w| {
                let layout = XrefStreamLayout::covering(w, 0);
                format!("xref stream /W {:?} entry={}B", w, layout.entry_width())
            },
        );
    }
    "不明".to_string()
}

/// Reads `/W [a b c]` out of a stream dictionary's leading bytes.
fn read_w_array(head: &[u8]) -> Option<[usize; 3]> {
    let at = find(head, b"/W")? + 2;
    let open = at + find(&head[at..], b"[")? + 1;
    let close = open + find(&head[open..], b"]")?;
    let nums: Vec<usize> = String::from_utf8_lossy(&head[open..close])
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    (nums.len() == 3).then(|| [nums[0], nums[1], nums[2]])
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Silences an unused-import warning when only some paths are exercised.
const _: Option<XrefRecord> = None;
