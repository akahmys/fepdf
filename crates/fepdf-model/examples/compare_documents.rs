//! Compares two PDFs by what they mean rather than by how they are written.
//!
//! Object numbers and dictionary key order are representation, not content: a reader
//! change perturbs both without changing the document. This walks the graph from the
//! catalogue, numbering objects by the order they are first reached and sorting keys by
//! name, so two files that say the same thing produce the same text.

use fepdf_model::arena::PdfArena;
use fepdf_model::handle::Handle;
use fepdf_model::object::{Object, SublimatedData};
use std::collections::BTreeMap;
use std::fmt::Write as _;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [left, right] = args.as_slice() else {
        eprintln!("usage: compare_documents <a.pdf> <b.pdf>");
        std::process::exit(2);
    };

    let a = canonical(left)?;
    let b = canonical(right)?;
    if a == b {
        println!("identical: {} objects reachable", a.lines().count());
        return Ok(());
    }

    let differences: Vec<_> =
        a.lines().zip(b.lines()).enumerate().filter(|(_, (x, y))| x != y).collect();
    let numeric = differences.iter().filter(|(_, (x, y))| skeleton(x) == skeleton(y)).count();
    println!(
        "DIFFERS: {} of {} reachable objects ({numeric} differ only in digits), {} vs {} lines",
        differences.len(),
        a.lines().count(),
        a.lines().count(),
        b.lines().count()
    );
    for (index, (x, y)) in differences.iter().take(4) {
        println!("  #{index}\n    a: {}\n    b: {}", clip(x), clip(y));
    }
    std::process::exit(1);
}

/// A line with every digit removed, to tell a numeric difference from a structural one.
fn skeleton(line: &str) -> String {
    line.chars().filter(|c| !c.is_ascii_digit()).collect()
}

/// Shortens a line so a large stream does not fill the terminal.
fn clip(line: &str) -> String {
    if line.len() > 300 { format!("{}…", &line[..300]) } else { line.to_string() }
}

/// One line per object, in the order the catalogue reaches them.
fn canonical(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let data = bytes::Bytes::from(std::fs::read(path)?);
    let doc = fepdf_model::Document::open(data, &Default::default())?;
    let arena = doc.arena();

    let mut order = BTreeMap::new();
    let mut queue = vec![*doc.root_handle()];
    order.insert(*doc.root_handle(), 0_usize);
    let mut out = String::new();

    let mut cursor = 0;
    while cursor < queue.len() {
        let handle = queue[cursor];
        cursor += 1;
        let object = arena.get_object(handle).unwrap_or(Object::Null);
        let mut line = String::new();
        write_object(&mut line, arena, &object, &mut order, &mut queue, 0);
        let _ = writeln!(out, "{line}");
    }
    Ok(out)
}

/// Writes one value, assigning reached references their position in the walk.
fn write_object(
    out: &mut String,
    arena: &PdfArena,
    object: &Object,
    order: &mut BTreeMap<Handle<Object>, usize>,
    queue: &mut Vec<Handle<Object>>,
    depth: usize,
) {
    if depth > 64 {
        out.push('…');
        return;
    }
    match object {
        Object::Reference(h) => {
            let next = order.len();
            let index = *order.entry(*h).or_insert_with(|| {
                queue.push(*h);
                next
            });
            let _ = write!(out, "@{index}");
        }
        Object::Dictionary(h) => {
            write_dict(out, arena, *h, order, queue, depth);
        }
        Object::Stream(h, data) => {
            write_dict(out, arena, *h, order, queue, depth);
            let _ = write!(out, "|stream:{}", payload(arena, data));
        }
        Object::Array(h) => {
            out.push('[');
            for item in arena.get_array(*h).unwrap_or_default() {
                write_object(out, arena, &item, order, queue, depth + 1);
                out.push(' ');
            }
            out.push(']');
        }
        Object::Name(n) => {
            let _ = write!(
                out,
                "/{}",
                arena.get_name(*n).map_or(String::new(), |v| v.as_str().to_string())
            );
        }
        Object::String(b) | Object::Hex(b) => {
            let _ = write!(out, "<{}>", hex(b));
        }
        Object::Text(t) => {
            let _ = write!(out, "({t})");
        }
        Object::Boolean(b) => {
            let _ = write!(out, "{b}");
        }
        Object::Integer(i) => {
            let _ = write!(out, "{i}");
        }
        Object::Real(f) => {
            let _ = write!(out, "{f:.6}");
        }
        Object::Null => out.push_str("null"),
    }
}

/// Writes a dictionary with its keys in name order, so interning cannot show through.
fn write_dict(
    out: &mut String,
    arena: &PdfArena,
    handle: Handle<BTreeMap<Handle<fepdf_model::object::PdfName>, Object>>,
    order: &mut BTreeMap<Handle<Object>, usize>,
    queue: &mut Vec<Handle<Object>>,
    depth: usize,
) {
    let dict = arena.get_dict(handle).unwrap_or_default();
    let mut named: Vec<(String, Object)> = dict
        .into_iter()
        .map(|(k, v)| (arena.get_name(k).map_or(String::new(), |n| n.as_str().to_string()), v))
        .collect();
    named.sort_by(|a, b| a.0.cmp(&b.0));

    out.push_str("<<");
    for (key, value) in named {
        // /Length restates the payload that follows it, which is compared directly.
        if key == "Length" {
            continue;
        }
        let _ = write!(out, "/{key} ");
        write_object(out, arena, &value, order, queue, depth + 1);
        out.push(' ');
    }
    out.push_str(">>");
}

/// A stream's payload, decoded where the engine holds it decoded.
fn payload(arena: &PdfArena, data: &std::sync::Arc<SublimatedData>) -> String {
    match data.as_ref() {
        SublimatedData::Raw(b) => format!("raw:{}:{}", b.len(), digest(b)),
        SublimatedData::Compressed { original_len, .. } => {
            arena.get_stream_bytes(data).map_or_else(
                |_| format!("compressed:{original_len}"),
                |b| format!("raw:{}:{}", b.len(), digest(&b)),
            )
        }
        SublimatedData::Commands { items } => format!("commands:{}", items.len()),
        SublimatedData::Image { width, height, data: pixels, .. } => {
            format!("image:{width}x{height}:{}", digest(pixels))
        }
    }
}

/// A short digest, enough to tell two payloads apart.
fn digest(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Hex for a string object, truncated so a long one stays readable.
fn hex(bytes: &[u8]) -> String {
    let shown: String = bytes.iter().take(64).map(|b| format!("{b:02x}")).collect();
    if bytes.len() > 64 { format!("{shown}…{}", bytes.len()) } else { shown }
}
