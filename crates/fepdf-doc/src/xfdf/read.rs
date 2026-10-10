//! Reading XFDF, by Table 34 (ISO 19444-1 6.7.2), into annotation dictionaries.

use super::{BORDER_STYLES, FLAGS, INTENTS, LISTS, NUMBERS, TEXTS, quadding, subtype_of};
use bytes::Bytes;
use fepdf_model::interpretation::Decision;
use fepdf_model::object::{PdfName, SublimatedData};
use fepdf_model::{Document, Handle, Object, PdfArena, PdfError, PdfResult};
use roxmltree::Node;
use std::collections::BTreeMap;
use std::sync::Arc;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// Puts the annotations of the XFDF file `xml` onto `doc`, as the FDF import does.
///
/// One whose `name` matches an annotation on its page replaces it in place, any other is
/// added, and each is drawn from its entries where it has no appearance.
///
/// **Read whole before anything changes** (ADR-0110): a file that is not XFDF changes
/// nothing.
///
/// # Errors
/// When the file is not UTF-8, not XML, or has no `xfdf` element; or a page will not read.
pub fn apply_import(doc: &Document, xml: &[u8]) -> PdfResult<()> {
    let refused = |why: String| PdfError::refused("ImportXfdf", why);
    let text = std::str::from_utf8(xml)
        .map_err(|_| refused("XFDF is UTF-8 (5.5.2), and this is not".into()))?;
    let tree =
        roxmltree::Document::parse(text).map_err(|e| refused(format!("this is not XML: {e}")))?;
    let root = tree.root_element();
    if root.tag_name().name() != "xfdf" {
        return Err(refused(format!(
            "the root element is <{}>, not <xfdf>",
            root.tag_name().name()
        )));
    }
    let arena = PdfArena::new();
    let mut built: Vec<(Handle<Object>, Option<String>, Option<String>)> = Vec::new();
    let elements = root
        .children()
        .filter(|n| n.tag_name().name() == "annots")
        .flat_map(|annots| annots.children().filter(Node::is_element));
    for node in elements {
        let Some(subtype) = subtype_of(node.tag_name().name()) else {
            doc.record(Decision::ambiguity(
                "ISO 19444-1 6.4.1",
                format!("an <{}> annotation", node.tag_name().name()),
                "left it out: links and projections are not imported, as from FDF",
            ));
            continue;
        };
        let dict = build(doc, &arena, node, subtype, text);
        let handle = arena.alloc_object(Object::Dictionary(arena.alloc_dict(dict)));
        built.push((
            handle,
            attribute(node, "name").map(str::to_owned),
            attribute(node, "inreplyto").map(str::to_owned),
        ));
    }
    answer(&arena, &built);
    let handles: Vec<Handle<Object>> = built.iter().map(|(h, ..)| *h).collect();
    crate::fdf::import_annotations(doc, &arena, &handles)
}

/// Gives each reply its `/IRT`: a reference to the annotation of that name in the file,
/// or the name itself, for the import to find on the page (Table 7).
fn answer(arena: &PdfArena, built: &[(Handle<Object>, Option<String>, Option<String>)]) {
    let by_name: BTreeMap<&str, Handle<Object>> =
        built.iter().filter_map(|(h, name, _)| name.as_deref().map(|n| (n, *h))).collect();
    for (handle, _, answers) in built {
        let Some(answers) = answers else { continue };
        let Some(dict) = arena.get_object(*handle).and_then(|o| o.as_dict_handle()) else {
            continue;
        };
        let value = by_name
            .get(answers.as_str())
            .map_or_else(|| Object::Text(answers.clone()), |h| Object::Reference(*h));
        let mut entries = arena.get_dict(dict).unwrap_or_default();
        entries.insert(arena.name("IRT"), value);
        arena.set_dict(dict, entries);
    }
}

/// The annotation dictionary an element stands for.
fn build(
    doc: &Document,
    arena: &PdfArena,
    node: Node<'_, '_>,
    subtype: &str,
    source: &str,
) -> Dict {
    let mut dict = Dict::new();
    let name = |n: &str| Object::Name(arena.name(n));
    dict.insert(arena.name("Type"), name("Annot"));
    dict.insert(arena.name("Subtype"), name(subtype));
    if let Some(page) = attribute(node, "page").and_then(|p| p.trim().parse::<i64>().ok()) {
        dict.insert(arena.name("Page"), Object::Integer(page));
    }
    common(arena, node, &mut dict);
    border(arena, node, &mut dict);
    kind(arena, node, subtype, &mut dict);
    children(doc, arena, node, subtype, source, &mut dict);
    dict
}

/// The attributes every kind may carry (Tables 5, 6, 7).
fn common(arena: &PdfArena, node: Node<'_, '_>, dict: &mut Dict) {
    for (attr, key) in TEXTS {
        if let Some(value) = attribute(node, attr) {
            dict.insert(arena.name(key), Object::Text(value.to_owned()));
        }
    }
    for (attr, key) in LISTS {
        if let Some(values) = attribute(node, attr).map(numbers).filter(|v| !v.is_empty()) {
            dict.insert(arena.name(key), reals(arena, &values));
        }
    }
    for (attr, key) in NUMBERS {
        if let Some(value) = attribute(node, attr).and_then(|v| v.trim().parse::<f64>().ok()) {
            dict.insert(arena.name(key), Object::Real(value));
        }
    }
    for (attr, key) in [("color", "C"), ("interior-color", "IC")] {
        if let Some(rgb) = attribute(node, attr).and_then(colour) {
            dict.insert(arena.name(key), reals(arena, &rgb));
        }
    }
    if let Some(words) = attribute(node, "flags") {
        let bits: i64 = words
            .split(',')
            .filter_map(|w| FLAGS.iter().find(|(f, _)| f.eq_ignore_ascii_case(w.trim())))
            .map(|(_, b)| *b)
            .sum();
        dict.insert(arena.name("F"), Object::Integer(bits));
    }
    if attribute(node, "replyType").is_some_and(|t| t.eq_ignore_ascii_case("group")) {
        dict.insert(arena.name("RT"), Object::Name(arena.name("Group")));
    }
}

/// `/BS` and `/BE` from `width`, `style`, `dashes` and `intensity` (Tables 20, 21).
fn border(arena: &PdfArena, node: Node<'_, '_>, dict: &mut Dict) {
    let mut bs = Dict::new();
    if let Some(width) = attribute(node, "width").and_then(|w| w.trim().parse::<f64>().ok()) {
        bs.insert(arena.name("W"), Object::Real(width));
    }
    let style = attribute(node, "style").unwrap_or_default();
    if let Some((_, key)) = BORDER_STYLES.iter().find(|(word, _)| word.eq_ignore_ascii_case(style))
    {
        bs.insert(arena.name("S"), Object::Name(arena.name(key)));
    }
    if let Some(dashes) = attribute(node, "dashes").map(numbers).filter(|d| !d.is_empty()) {
        bs.insert(arena.name("D"), reals(arena, &dashes));
    }
    if !bs.is_empty() {
        dict.insert(arena.name("BS"), Object::Dictionary(arena.alloc_dict(bs)));
    }
    if style.eq_ignore_ascii_case("cloudy") {
        let mut be = Dict::new();
        be.insert(arena.name("S"), Object::Name(arena.name("C")));
        if let Some(i) = attribute(node, "intensity").and_then(|i| i.trim().parse::<f64>().ok()) {
            be.insert(arena.name("I"), Object::Real(i));
        }
        dict.insert(arena.name("BE"), Object::Dictionary(arena.alloc_dict(be)));
    }
}

/// What only some kinds carry: icons, intent, line ends and points, symbol, justification,
/// caption, and a redaction's overlay repeat.
fn kind(arena: &PdfArena, node: Node<'_, '_>, subtype: &str, dict: &mut Dict) {
    let name = |n: &str| Object::Name(arena.name(n));
    if let Some(icon) = attribute(node, "icon") {
        dict.insert(arena.name("Name"), name(icon));
    }
    if let Some(intent) = attribute(node, "intent") {
        let key = INTENTS.iter().find(|(w, _)| *w == intent).map_or(intent, |(_, n)| *n);
        dict.insert(arena.name("IT"), name(key));
    }
    let (head, tail) = (attribute(node, "head"), attribute(node, "tail"));
    if head.is_some() || tail.is_some() {
        let ends = vec![name(head.unwrap_or("None")), name(tail.unwrap_or("None"))];
        dict.insert(arena.name("LE"), Object::Array(arena.alloc_array(ends)));
    }
    if let (Some(start), Some(end)) = (attribute(node, "start"), attribute(node, "end")) {
        let mut l = numbers(start);
        l.extend(numbers(end));
        dict.insert(arena.name("L"), reals(arena, &l));
    }
    if let Some(symbol) = attribute(node, "symbol") {
        dict.insert(
            arena.name("Sy"),
            name(if symbol.eq_ignore_ascii_case("paragraph") { "P" } else { "None" }),
        );
    }
    if let Some(word) =
        attribute(node, "justification").filter(|_| subtype == "FreeText" || subtype == "Redact")
    {
        dict.insert(arena.name("Q"), Object::Integer(quadding(word)));
    }
    let yes = |v: &str| v.eq_ignore_ascii_case("yes") || v.eq_ignore_ascii_case("true");
    if let Some(caption) = attribute(node, "caption") {
        dict.insert(arena.name("Cap"), Object::Boolean(yes(caption)));
    }
    if let Some(style) = attribute(node, "caption-style") {
        dict.insert(arena.name("CP"), name(style));
    }
    let offset = |a: &str| attribute(node, a).and_then(|v| v.trim().parse::<f64>().ok());
    if let (Some(h), Some(v)) = (offset("caption-offset-h"), offset("caption-offset-v")) {
        dict.insert(arena.name("CO"), reals(arena, &[h, v]));
    }
    if let Some(repeat) = attribute(node, "overlay-text-repeat") {
        dict.insert(arena.name("Repeat"), Object::Boolean(yes(repeat)));
    }
}

/// The child elements: words, rich text, default appearance and style, vertices, ink, and
/// a file's or a sound's data.
fn children(
    doc: &Document,
    arena: &PdfArena,
    node: Node<'_, '_>,
    subtype: &str,
    source: &str,
    dict: &mut Dict,
) {
    for child in node.children().filter(Node::is_element) {
        let text = child.text().unwrap_or_default();
        match child.tag_name().name() {
            "contents" => {
                dict.insert(arena.name("Contents"), Object::Text(text.to_owned()));
            }
            "contents-richtext" => {
                let inner: String =
                    child.children().filter_map(|c| source.get(c.range())).collect();
                dict.insert(arena.name("RC"), Object::Text(inner.trim().to_owned()));
            }
            "defaultappearance" => {
                dict.insert(
                    arena.name("DA"),
                    Object::String(Bytes::copy_from_slice(text.trim().as_bytes())),
                );
            }
            "defaultstyle" => {
                dict.insert(arena.name("DS"), Object::Text(text.trim().to_owned()));
            }
            "vertices" => {
                dict.insert(arena.name("Vertices"), reals(arena, &numbers(text)));
            }
            "inklist" => {
                let strokes: Vec<Object> = child
                    .children()
                    .filter(|g| g.tag_name().name() == "gesture")
                    .map(|g| reals(arena, &numbers(g.text().unwrap_or_default())))
                    .collect();
                dict.insert(arena.name("InkList"), Object::Array(arena.alloc_array(strokes)));
            }
            "data" => data(doc, arena, node, child, subtype, dict),
            _ => {}
        }
    }
}

/// A `data` element: a file attachment's file as `/FS`, or a sound's samples as
/// `/Sound` (Tables 15, 16, 24, 32).
fn data(
    doc: &Document,
    arena: &PdfArena,
    node: Node<'_, '_>,
    child: Node<'_, '_>,
    subtype: &str,
    dict: &mut Dict,
) {
    let Some(bytes) = decoded(doc, child) else { return };
    let name = |n: &str| Object::Name(arena.name(n));
    let mut stream = Dict::new();
    if subtype == "Sound" {
        stream.insert(arena.name("Type"), name("Sound"));
        let number = |a: &str| attribute(node, a).and_then(|v| v.trim().parse::<f64>().ok());
        for (attr, key) in [("rate", "R"), ("channels", "C"), ("bits", "B")] {
            if let Some(v) = number(attr) {
                stream.insert(arena.name(key), Object::Real(v));
            }
        }
        let encoding =
            match attribute(node, "encoding").unwrap_or("raw").to_ascii_lowercase().as_str() {
                "signed" => "Signed",
                "mulaw" => "muLaw",
                "alaw" => "ALaw",
                _ => "Raw",
            };
        stream.insert(arena.name("E"), name(encoding));
    } else {
        stream.insert(arena.name("Type"), name("EmbeddedFile"));
        if let Some(mime) = attribute(child, "mimetype") {
            stream.insert(arena.name("Subtype"), name(mime));
        }
    }
    let body =
        Object::Stream(arena.alloc_dict(stream), Arc::new(SublimatedData::Raw(Bytes::from(bytes))));
    let held = arena.alloc_object(body);
    if subtype == "Sound" {
        dict.insert(arena.name("Sound"), Object::Reference(held));
        return;
    }
    let file = attribute(node, "file").unwrap_or("attachment").to_owned();
    let mut ef = Dict::new();
    ef.insert(arena.name("F"), Object::Reference(held));
    let mut spec = Dict::new();
    spec.insert(arena.name("Type"), name("Filespec"));
    spec.insert(arena.name("F"), Object::Text(file.clone()));
    spec.insert(arena.name("UF"), Object::Text(file));
    spec.insert(arena.name("EF"), Object::Dictionary(arena.alloc_dict(ef)));
    dict.insert(arena.name("FS"), Object::Dictionary(arena.alloc_dict(spec)));
}

/// A `data` element's bytes: hexadecimal or ASCII (5.8.4), inflated where `filter` names
/// `FlateDecode`. Any other filter is recorded and the data left out.
fn decoded(doc: &Document, child: Node<'_, '_>) -> Option<Vec<u8>> {
    let content = child.text().unwrap_or_default();
    let raw: Vec<u8> =
        if attribute(child, "encoding").is_some_and(|e| e.eq_ignore_ascii_case("hex")) {
            let digits: Vec<u8> = content.bytes().filter(u8::is_ascii_hexdigit).collect();
            digits
                .chunks(2)
                .filter_map(|pair| {
                    std::str::from_utf8(pair).ok().and_then(|p| u8::from_str_radix(p, 16).ok())
                })
                .collect()
        } else {
            content.as_bytes().to_vec()
        };
    match attribute(child, "filter").map(str::trim) {
        None | Some("") => Some(raw),
        Some("FlateDecode") => fepdf_model::filters::flate::inflate(&raw).ok(),
        Some(other) => {
            doc.record(Decision::ambiguity(
                "ISO 19444-1 5.8.4",
                format!("a data element filtered by {other}"),
                "left the data out",
            ));
            None
        }
    }
}

/// An attribute by name, whatever case the file wrote its first letter in: Table 4 calls
/// `page` "Page", and Table 11 calls `symbol` "Symbol".
fn attribute<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes().find(|a| a.name().eq_ignore_ascii_case(name)).map(|a| a.value())
}

/// Numbers separated by commas, semicolons or white space.
fn numbers(text: &str) -> Vec<f64> {
    text.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter_map(|n| n.trim().parse::<f64>().ok())
        .collect()
}

/// `#RRGGBB` as three components from 0 to 1 (Table 5).
fn colour(text: &str) -> Option<Vec<f64>> {
    let hex = text.trim().strip_prefix('#')?;
    let channel = |at: usize| {
        hex.get(at..at + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .map(|v| f64::from(v) / 255.0)
    };
    Some(vec![channel(0)?, channel(2)?, channel(4)?])
}

fn reals(arena: &PdfArena, values: &[f64]) -> Object {
    Object::Array(arena.alloc_array(values.iter().map(|v| Object::Real(*v)).collect()))
}
