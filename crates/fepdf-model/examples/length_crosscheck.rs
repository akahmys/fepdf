//! Cross-checks each scanned stream extent against the `/Length` the file declares.
//!
//! The reader delimits a stream by scanning to `endstream` whenever `/Length` is an
//! indirect reference, because the referenced object may not be parsed yet. It then
//! overwrites `/Length` with what it scanned, and never compares the two. This asks
//! the question the reader does not: when the file's own `/Length` object is resolved,
//! does it agree?
//!
//! An indirect `/Length` is conforming (ISO 32000-2, 7.3.8.2), so agreement here means
//! the reader is recording an ambiguity where the file has none.

use std::collections::BTreeMap;

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(data) = std::fs::read(&path) else { continue };
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();

        // Object number -> integer value, for every `N 0 obj <int> endobj` in the file.
        let ints = scalar_objects(&data);

        let mut agree = 0;
        let mut disagree = Vec::new();
        let mut unresolved = 0;

        for (stream_at, declared) in indirect_lengths(&data) {
            let Some(target) = ints.get(&declared) else {
                unresolved += 1;
                continue;
            };
            let scanned = scan_extent(&data, stream_at);
            match scanned {
                Some(n) if n == *target => agree += 1,
                Some(n) => disagree.push((declared, *target, n)),
                None => unresolved += 1,
            }
        }

        println!(
            "{name:<26} indirect /Length: {:>4} agree, {:>3} disagree, {:>3} unresolved",
            agree,
            disagree.len(),
            unresolved
        );
        for (obj, declared, scanned) in disagree.iter().take(5) {
            println!("    /Length {obj} 0 R = {declared}, scanned {scanned}");
        }
    }
}

/// `N 0 obj <integer> endobj`, which is what an indirect `/Length` points at.
fn scalar_objects(data: &[u8]) -> BTreeMap<u32, usize> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while let Some(p) = find(data, b" obj", i) {
        i = p + 4;
        let Some(num) = number_before(data, p) else { continue };
        let rest = &data[i..(i + 64).min(data.len())];
        let text = String::from_utf8_lossy(rest);
        let trimmed = text.trim_start();
        if let Some(end) = trimmed.find(|c: char| !c.is_ascii_digit())
            && end > 0
            && trimmed[end..].trim_start().starts_with("endobj")
            && let Ok(v) = trimmed[..end].parse::<usize>()
        {
            out.insert(num, v);
        }
    }
    out
}

/// Byte position just after each `stream` keyword whose `/Length` is indirect, with
/// the object number that `/Length` refers to.
fn indirect_lengths(data: &[u8]) -> Vec<(usize, u32)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = find(data, b"/Length", i) {
        i = p + 7;
        let rest = &data[i..(i + 32).min(data.len())];
        let text = String::from_utf8_lossy(rest);
        let t = text.trim_start();
        // `N G R` — an indirect reference, as opposed to a literal integer.
        let parts: Vec<&str> = t.split_whitespace().take(3).collect();
        if parts.len() == 3
            && parts[2] == "R"
            && let Ok(num) = parts[0].parse::<u32>()
            && let Some(s) = find(data, b"stream", i)
        {
            out.push((after_eol(data, s + 6), num));
        }
    }
    out
}

/// The extent the reader would scan: up to `endstream`, minus the EOL before it.
fn scan_extent(data: &[u8], data_at: usize) -> Option<usize> {
    let at = find(data, b"endstream", data_at)?;
    let mut end = at;
    if end > data_at && data[end - 1] == b'\n' {
        end -= 1;
    }
    if end > data_at && data[end - 1] == b'\r' {
        end -= 1;
    }
    Some(end - data_at)
}

fn after_eol(data: &[u8], mut at: usize) -> usize {
    if at < data.len() && data[at] == b'\r' {
        at += 1;
    }
    if at < data.len() && data[at] == b'\n' {
        at += 1;
    }
    at
}

fn number_before(data: &[u8], at: usize) -> Option<u32> {
    // `N G obj` — step back over the generation, then the object number.
    let s = String::from_utf8_lossy(&data[at.saturating_sub(24)..at]);
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() >= 2 { parts[parts.len() - 2].parse().ok() } else { None }
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}
