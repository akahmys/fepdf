//! Writing XFDF, by Table 33 (ISO 19444-1 6.7.1).

use super::{
    BORDER_STYLES, FLAGS, INTENTS, LISTS, NAMESPACE, NUMBERS, TEXTS, element_of, justification,
};
use crate::apply::drawn::Entries;
use fepdf_model::interpretation::Decision;
use fepdf_model::{Document, Handle, Object, PdfArena, PdfResult};
use std::collections::BTreeMap;
use std::fmt::Write as _;

type Dict = BTreeMap<Handle<fepdf_model::object::PdfName>, Object>;

/// Every markup annotation of `doc` as an XFDF file: the ones `fdf::export` writes, each
/// named, with its replies and states.
///
/// # Errors
/// When a page will not read, or a stream an annotation carries will not decode.
pub fn export(doc: &Document) -> PdfResult<String> {
    let arena = doc.arena();
    let chosen = crate::fdf::exportable(doc)?;
    // Each one's name, its own or the one the FDF export gives it, so `inreplyto` can
    // name what it answers (Table 7: "the contents of the name attribute").
    let names: BTreeMap<Handle<Object>, String> = chosen
        .iter()
        .map(|c| {
            let own = dict_of(arena, c.handle).and_then(|d| Entries::new(arena, &d).text("NM"));
            (c.handle, own.unwrap_or_else(|| format!("fepdf-p{}-{}", c.page, c.index)))
        })
        .collect();
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xfdf xmlns=\"{NAMESPACE}\" xml:space=\"preserve\">\n<annots>\n"
    );
    for c in &chosen {
        let Some(dict) = dict_of(arena, c.handle) else { continue };
        let entries = Entries::new(arena, &dict);
        let subtype = entries.name("Subtype").unwrap_or_default();
        let Some(element) = element_of(&subtype) else { continue };
        let name = names.get(&c.handle).cloned().unwrap_or_default();
        let mut tag = Tag { element, attributes: String::new(), children: String::new() };
        tag.attribute("page", &c.page.to_string());
        attributes(&mut tag, &entries, &subtype, &name, &names);
        children(doc, &mut tag, &entries, &subtype)?;
        said_nowhere(doc, &entries, &subtype, &name);
        tag.close(&mut out);
    }
    out.push_str("</annots>\n</xfdf>\n");
    Ok(out)
}

/// An element being written.
struct Tag {
    element: &'static str,
    attributes: String,
    children: String,
}

impl Tag {
    fn attribute(&mut self, name: &str, value: &str) {
        let _ = write!(self.attributes, " {name}=\"{}\"", escape(value));
    }

    fn child(&mut self, name: &str, text: &str) {
        let _ = writeln!(self.children, "<{name}>{}</{name}>", escape(text));
    }

    fn close(self, out: &mut String) {
        let _ = writeln!(
            out,
            "<{}{}>\n{}</{}>",
            self.element, self.attributes, self.children, self.element
        );
    }
}

/// Every attribute Table 33 maps an entry of this annotation to.
fn attributes(
    tag: &mut Tag,
    entries: &Entries<'_>,
    subtype: &str,
    name: &str,
    names: &BTreeMap<Handle<Object>, String>,
) {
    for (attribute, key) in TEXTS {
        let value = if *key == "NM" { Some(name.to_owned()) } else { entries.text(key) };
        if let Some(value) = value {
            tag.attribute(attribute, &value);
        }
    }
    for (attribute, key) in LISTS {
        let values = entries.numbers(key);
        if !values.is_empty() {
            tag.attribute(attribute, &list(&values));
        }
    }
    for (attribute, key) in NUMBERS {
        if let Some(value) = entries.number(key) {
            tag.attribute(attribute, &number(value));
        }
    }
    for (attribute, key) in [("color", "C"), ("interior-color", "IC")] {
        if let Some(colour) = hex_colour(&entries.numbers(key)) {
            tag.attribute(attribute, &colour);
        }
    }
    flags(tag, entries);
    border(tag, entries);
    reply(tag, entries, names);
    kind_attributes(tag, entries, subtype);
}

/// `flags` from `/F` (Table 5).
fn flags(tag: &mut Tag, entries: &Entries<'_>) {
    let Some(bits) = entries.get("F").and_then(|f| f.as_integer()) else { return };
    let words: Vec<&str> =
        FLAGS.iter().filter(|(_, bit)| bits & bit != 0).map(|(w, _)| *w).collect();
    if !words.is_empty() {
        tag.attribute("flags", &words.join(","));
    }
}

/// `width`, `style`, `dashes` and `intensity` from `/BS` and `/BE` (Tables 20, 21).
fn border(tag: &mut Tag, entries: &Entries<'_>) {
    if let Some(width) = entries.within("BS", "W").and_then(|w| w.as_f64()) {
        tag.attribute("width", &number(width));
    }
    let cloudy =
        entries.within("BE", "S").and_then(|s| s.as_name()) == Some(entries.arena().name("C"));
    let style = entries
        .within("BS", "S")
        .and_then(|s| s.as_name())
        .and_then(|n| entries.arena().get_name_str(n));
    if cloudy {
        tag.attribute("style", "cloudy");
    } else if let Some(word) = style.and_then(|s| BORDER_STYLES.iter().find(|(_, k)| *k == s)) {
        tag.attribute("style", word.0);
    }
    if let Some(dashes) = entries
        .within("BS", "D")
        .and_then(|d| d.as_array())
        .and_then(|a| entries.arena().get_array(a))
    {
        let values: Vec<f64> = dashes.iter().filter_map(Object::as_f64).collect();
        tag.attribute("dashes", &list(&values));
    }
    if let Some(intensity) = entries.within("BE", "I").and_then(|i| i.as_f64()) {
        tag.attribute("intensity", &number(intensity));
    }
}

/// `inreplyto`, by the name of what it answers, and `replyType` (Table 7).
fn reply(tag: &mut Tag, entries: &Entries<'_>, names: &BTreeMap<Handle<Object>, String>) {
    let answered =
        entries.written("IRT").and_then(|v| v.as_reference()).and_then(|h| names.get(&h));
    if let Some(answered) = answered {
        tag.attribute("inreplyto", answered);
        if entries.name("RT").as_deref() == Some("Group") {
            tag.attribute("replyType", "group");
        }
    }
}

/// What only some kinds carry: icons, line ends, intent, symbol, justification, caption.
fn kind_attributes(tag: &mut Tag, entries: &Entries<'_>, subtype: &str) {
    if let Some(icon) = entries.name("Name") {
        tag.attribute("icon", &icon);
    }
    if let Some(intent) = entries.name("IT") {
        let word = INTENTS.iter().find(|(_, n)| *n == intent).map_or(intent.as_str(), |(w, _)| *w);
        tag.attribute("intent", word);
    }
    let ends = entries.names("LE");
    if subtype == "Line" || subtype == "PolyLine" {
        for (attribute, end) in [("head", ends.first()), ("tail", ends.get(1))] {
            if let Some(end) = end {
                tag.attribute(attribute, end);
            }
        }
    }
    if let [x1, y1, x2, y2] = entries.numbers("L")[..] {
        tag.attribute("start", &list(&[x1, y1]));
        tag.attribute("end", &list(&[x2, y2]));
    }
    if subtype == "Caret" {
        let paragraph = entries.name("Sy").as_deref() == Some("P");
        tag.attribute("symbol", if paragraph { "paragraph" } else { "none" });
    }
    if let Some(q) = entries.get("Q").and_then(|q| q.as_integer()) {
        tag.attribute("justification", justification(subtype, q));
    }
    caption(tag, entries);
    if let Some(Object::Boolean(repeat)) = entries.get("Repeat") {
        tag.attribute("overlay-text-repeat", if repeat { "true" } else { "false" });
    }
}

/// A line's `caption`, `caption-style` and `caption-offset-h`/`-v` (Table 9).
fn caption(tag: &mut Tag, entries: &Entries<'_>) {
    if let Some(Object::Boolean(on)) = entries.get("Cap") {
        tag.attribute("caption", if on { "yes" } else { "no" });
    }
    if let Some(style) = entries.name("CP") {
        tag.attribute("caption-style", &style);
    }
    if let [h, v] = entries.numbers("CO")[..] {
        tag.attribute("caption-offset-h", &number(h));
        tag.attribute("caption-offset-v", &number(v));
    }
}

/// The child elements: words, rich text, default appearance and style, vertices, ink, and
/// a file's or a sound's data.
fn children(doc: &Document, tag: &mut Tag, entries: &Entries<'_>, subtype: &str) -> PdfResult<()> {
    if let Some(contents) = entries.text("Contents") {
        tag.child("contents", &contents);
    }
    // `/RC` is XHTML already (12.7.4.3); carried as markup when it parses, and left out
    // when it does not rather than escaped into text that would read as its own source.
    if let Some(rich) = entries.text("RC").filter(|r| roxmltree::Document::parse(r).is_ok()) {
        let _ = writeln!(tag.children, "<contents-richtext>{rich}</contents-richtext>");
    }
    for (element, key) in [("defaultappearance", "DA"), ("defaultstyle", "DS")] {
        if let Some(value) = entries.text(key) {
            tag.child(element, &value);
        }
    }
    let vertices = entries.numbers("Vertices");
    if !vertices.is_empty() {
        tag.child("vertices", &pairs(&vertices));
    }
    let ink = entries.arrays("InkList");
    if !ink.is_empty() {
        tag.children.push_str("<inklist>\n");
        for stroke in &ink {
            let _ = writeln!(tag.children, "<gesture>{}</gesture>", pairs(stroke));
        }
        tag.children.push_str("</inklist>\n");
    }
    match subtype {
        "FileAttachment" => file_data(doc, tag, entries),
        "Sound" => sound_data(doc, tag, entries),
        _ => Ok(()),
    }
}

/// A file attachment's file: its name as `file`, and its bytes as `data` (Tables 15, 25).
fn file_data(doc: &Document, tag: &mut Tag, entries: &Entries<'_>) -> PdfResult<()> {
    let arena = entries.arena();
    let Some(spec) = entries.get("FS").and_then(|f| f.as_dict_handle()) else { return Ok(()) };
    let text = |key: &str| {
        arena
            .dict_entry(spec, arena.name(key))
            .and_then(|v| crate::apply::fields::text_of(arena, &v))
    };
    if let Some(file) = text("UF").or_else(|| text("F")) {
        tag.attribute("file", &file);
    }
    let stream = arena
        .dict_entry(spec, arena.name("EF"))
        .and_then(|ef| ef.resolve(arena).as_dict_handle())
        .and_then(|ef| {
            arena.dict_entry(ef, arena.name("F")).or_else(|| arena.dict_entry(ef, arena.name("UF")))
        });
    if let Some(stream) = stream {
        let bytes = doc.decode_stream(&stream.resolve(arena))?;
        let mimetype = stream_name(arena, &stream.resolve(arena), "Subtype");
        data(tag, &bytes, mimetype.as_deref());
    }
    Ok(())
}

/// A sound's samples as `data`, and its `rate`, `channels`, `bits` and `encoding`
/// (Table 16).
fn sound_data(doc: &Document, tag: &mut Tag, entries: &Entries<'_>) -> PdfResult<()> {
    let arena = entries.arena();
    let Some(sound) = entries.get("Sound") else { return Ok(()) };
    let Object::Stream(dict, _) = &sound else { return Ok(()) };
    let number_of = |key: &str| arena.dict_entry(*dict, arena.name(key)).and_then(|v| v.as_f64());
    for (attribute, key) in [("rate", "R"), ("channels", "C"), ("bits", "B")] {
        if let Some(value) = number_of(key) {
            tag.attribute(attribute, &number(value));
        }
    }
    if let Some(encoding) = stream_name(arena, &sound, "E") {
        tag.attribute("encoding", &encoding.to_ascii_lowercase());
    }
    let bytes = doc.decode_stream(&sound)?;
    data(tag, &bytes, None);
    Ok(())
}

/// Bytes as a `data` element, by 5.8.4's second method: raw and hexadecimal, so no byte
/// needs escaping. They are written decoded, so no `filter` is named.
fn data(tag: &mut Tag, bytes: &[u8], mimetype: Option<&str>) {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02X}");
    }
    let mime = mimetype.map_or_else(String::new, |m| format!(" mimetype=\"{}\"", escape(m)));
    let _ = writeln!(
        tag.children,
        "<data mode=\"raw\" encoding=\"hex\" length=\"{}\"{mime}>{hex}</data>",
        bytes.len()
    );
}

/// Records what this annotation carries that XFDF has nowhere to put.
fn said_nowhere(doc: &Document, entries: &Entries<'_>, subtype: &str, name: &str) {
    let mut lost = Vec::new();
    if subtype == "FreeText" {
        lost.extend(["CL", "RD"].into_iter().filter(|k| entries.get(k).is_some()));
    }
    if entries.get("Path").is_some() {
        lost.push("Path");
    }
    // A stamp drawn from its name is drawn again from it; one with a picture is not.
    if subtype == "Stamp" && entries.get("AP").is_some() && entries.name("Name").is_none() {
        lost.push("AP");
    }
    if !lost.is_empty() {
        doc.record(Decision::ambiguity(
            "ISO 19444-1 6.7.1",
            format!(
                "annotation {name:?} ({subtype}) carries {lost:?}, which Table 33 maps to nothing"
            ),
            "wrote it to XFDF without them",
        ));
    }
}

fn dict_of(arena: &PdfArena, handle: Handle<Object>) -> Option<Dict> {
    arena.get_object(handle)?.as_dict_handle().and_then(|d| arena.get_dict(d))
}

fn stream_name(arena: &PdfArena, stream: &Object, key: &str) -> Option<String> {
    let Object::Stream(dict, _) = stream else { return None };
    arena.dict_entry(*dict, arena.name(key))?.as_name().and_then(|n| arena.get_name_str(n))
}

/// `#RRGGBB` for a colour of 1, 3 or 4 components (Table 5 states RGB only, so grey and
/// CMYK are converted).
fn hex_colour(components: &[f64]) -> Option<String> {
    let [r, g, b] = match *components {
        [grey] => [grey; 3],
        [r, g, b] => [r, g, b],
        [c, m, y, k] => [(1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k)],
        _ => return None,
    };
    Some(format!("#{:02X}{:02X}{:02X}", byte(r), byte(g), byte(b)))
}

/// A component from 0 to 1 as a byte from 0 to 255.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=1 and scaled to 0..=255 before the cast"
)]
fn byte(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Comma-separated numbers (Tables 5, 7, 10, 21).
fn list(values: &[f64]) -> String {
    values.iter().map(|v| number(*v)).collect::<Vec<_>>().join(",")
}

/// Coordinate pairs, `x,y;x,y` (6.5.12, 6.5.31).
fn pairs(values: &[f64]) -> String {
    values.chunks(2).map(list).collect::<Vec<_>>().join(";")
}

/// A number as briefly as it can be written to four places.
fn number(value: f64) -> String {
    let written = format!("{value:.4}");
    let trimmed = written.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" { "0".to_owned() } else { trimmed.to_owned() }
}

/// Text for an attribute or an element, escaped as 5.8.2 says: the XML delimiters as
/// entities, line breaks and tabs as character references, and any other control
/// character as a backslash and three octal digits.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            '\t' => out.push_str("&#x9;"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\{:03o}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}
