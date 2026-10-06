//! An inline image's dictionary, spelled out as an image XObject's (8.9.7, Tables 91 and
//! 92).
//!
//! **One home for the abbreviations.** A redaction lifts an inline image into an image
//! XObject, and the interpreter draws one the way it draws an image XObject (ROADMAP
//! Y-F33). Both need the same dictionary, and both read it from here: what `BI` wrote,
//! with its keys and names spelled out and a colour space the resources name replaced by
//! that space.

use crate::arena::PdfArena;
use crate::handle::Handle;
use crate::lexer::Token;
use crate::object::{Object, PdfName};
use crate::parser::Parser;
use std::collections::BTreeMap;

/// How deep a colour space is followed: an `/Indexed` base, and what a copied one holds
/// (Rule 6).
const DEPTH: usize = 8;

/// An inline image's key spelled out (Table 91); a key already whole stays.
#[must_use]
pub fn spelled_key(key: &str) -> &str {
    match key {
        "BPC" => "BitsPerComponent",
        "CS" => "ColorSpace",
        "D" => "Decode",
        "DP" => "DecodeParms",
        "F" => "Filter",
        "H" => "Height",
        "IM" => "ImageMask",
        "I" => "Interpolate",
        "L" => "Length",
        "W" => "Width",
        other => other,
    }
}

/// A filter's name spelled out (Table 92).
#[must_use]
pub fn spelled_filter(name: &str) -> &str {
    match name {
        "AHx" => "ASCIIHexDecode",
        "A85" => "ASCII85Decode",
        "LZW" => "LZWDecode",
        "Fl" => "FlateDecode",
        "RL" => "RunLengthDecode",
        "CCF" => "CCITTFaxDecode",
        "DCT" => "DCTDecode",
        other => other,
    }
}

/// A colour space's name spelled out (Table 92).
#[must_use]
pub fn spelled_space(name: &str) -> &str {
    match name {
        "G" => "DeviceGray",
        "RGB" => "DeviceRGB",
        "CMYK" => "DeviceCMYK",
        "I" => "Indexed",
        other => other,
    }
}

/// The components of a device colour space, by its name or its inline abbreviation;
/// `Indexed` is one, an index.
#[must_use]
pub fn device_components(name: &str) -> Option<usize> {
    match name {
        "G" | "DeviceGray" | "I" | "Indexed" => Some(1),
        "RGB" | "DeviceRGB" => Some(3),
        "CMYK" | "DeviceCMYK" => Some(4),
        _ => None,
    }
}

/// The abbreviated dictionary's entries between `BI` and `ID`, keys as written, values
/// made in `arena`.
#[must_use]
pub fn entries(header: &[u8], arena: &PdfArena) -> Vec<(String, Object)> {
    let mut parser = Parser::new(bytes::Bytes::copy_from_slice(header), arena);
    let mut out = Vec::new();
    while let Ok(Token::Name(key)) = parser.next_token() {
        let Ok(value) = parser.parse_object() else { break };
        out.push((String::from_utf8_lossy(&key).to_string(), value));
    }
    out
}

/// The image XObject dictionary `header` makes, in `arena`: `/Type` and `/Subtype`, every
/// key and name spelled out, and a colour space named from the resources replaced by what
/// `named_space` answers for it, since an image XObject names a space and not a resource.
///
/// **Where a key is written both ways, the abbreviation is taken**, and the second half
/// of the answer names each such key for the caller to record. 8.9.7 lets the
/// abbreviations stand in for the full names and says nothing of a header giving both;
/// the abbreviation is the form Table 91 gives inline images, and the PDF Association's
/// `InlineAbbreviations.pdf` marks it the one meant.
#[must_use]
pub fn dictionary(
    header: &[u8],
    arena: &PdfArena,
    named_space: &dyn Fn(&str) -> Option<Object>,
) -> (BTreeMap<Handle<PdfName>, Object>, Vec<String>) {
    let mut dict = BTreeMap::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("XObject")));
    dict.insert(arena.name("Subtype"), Object::Name(arena.name("Image")));
    let (mut abbreviated, mut both) = (std::collections::BTreeSet::new(), Vec::new());
    for (written, value) in entries(header, arena) {
        let key = spelled_key(&written);
        let short = key != written;
        let name = arena.name(key);
        if dict.contains_key(&name) && short != abbreviated.contains(key) {
            both.push(key.to_owned());
            if !short {
                continue;
            }
        }
        if short {
            abbreviated.insert(key.to_owned());
        }
        let value = match key {
            "ColorSpace" => colour_space(arena, value, named_space, 0),
            "Filter" => spelled_names(arena, value, spelled_filter),
            _ => value,
        };
        dict.insert(name, value);
    }
    (dict, both)
}

/// `value`, a name or an array of them, with each name spelled out by `spell`.
fn spelled_names(arena: &PdfArena, value: Object, spell: fn(&str) -> &str) -> Object {
    let one = |o: &Object| match o.as_name().and_then(|n| arena.get_name_str(n)) {
        Some(name) => Object::Name(arena.name(spell(&name))),
        None => o.clone(),
    };
    match value {
        Object::Array(a) => {
            let items = arena.get_array(a).unwrap_or_default().iter().map(one).collect();
            Object::Array(arena.alloc_array(items))
        }
        other => one(&other),
    }
}

/// The colour space an inline image names: a device space or `Indexed` spelled out, and a
/// name the resources give a space to replaced by that space.
fn colour_space(
    arena: &PdfArena,
    value: Object,
    named_space: &dyn Fn(&str) -> Option<Object>,
    depth: usize,
) -> Object {
    if depth >= DEPTH {
        return value;
    }
    if let Some(name) = value.as_name().and_then(|n| arena.get_name_str(n)) {
        let spelled = spelled_space(&name);
        if spelled != name || device_components(&name).is_some() {
            return Object::Name(arena.name(spelled));
        }
        return named_space(&name).unwrap_or(value);
    }
    let Some(mut items) = value.as_array().and_then(|a| arena.get_array(a)) else { return value };
    if let Some(first) = items.first_mut() {
        *first = spelled_names(arena, first.clone(), spelled_space);
    }
    if let Some(base) = items.get_mut(1) {
        *base = colour_space(arena, base.clone(), named_space, depth + 1);
    }
    Object::Array(arena.alloc_array(items))
}

/// `object` of `from`, made again in `to`: references followed, names interned again,
/// arrays and dictionaries copied, a stream's data shared.
///
/// **For drawing an inline image without writing to the document.** A document's arena is
/// sealed outside `apply`, and the dictionary an inline image makes is built in a scratch
/// arena; the colour space the page's resources name comes across with this.
#[must_use]
pub fn copy_between(from: &PdfArena, to: &PdfArena, object: &Object, depth: usize) -> Object {
    if depth >= DEPTH {
        return Object::Null;
    }
    let copy_dict = |dict: Handle<BTreeMap<Handle<PdfName>, Object>>| {
        let entries = from.get_dict(dict).unwrap_or_default();
        let copied = entries
            .iter()
            .filter_map(|(k, v)| {
                let key = to.name(&from.get_name_str(*k)?);
                Some((key, copy_between(from, to, v, depth + 1)))
            })
            .collect();
        to.alloc_dict(copied)
    };
    match object {
        Object::Reference(h) => {
            from.get_object(*h).map_or(Object::Null, |o| copy_between(from, to, &o, depth + 1))
        }
        Object::Name(n) => {
            from.get_name_str(*n).map_or(Object::Null, |name| Object::Name(to.name(&name)))
        }
        Object::Array(a) => Object::Array(
            to.alloc_array(
                from.get_array(*a)
                    .unwrap_or_default()
                    .iter()
                    .map(|item| copy_between(from, to, item, depth + 1))
                    .collect(),
            ),
        ),
        Object::Dictionary(d) => Object::Dictionary(copy_dict(*d)),
        Object::Stream(d, data) => Object::Stream(copy_dict(*d), std::sync::Arc::clone(data)),
        Object::Boolean(_)
        | Object::Integer(_)
        | Object::Real(_)
        | Object::String(_)
        | Object::Hex(_)
        | Object::Text(_)
        | Object::Null => object.clone(),
    }
}
