//! Inline images found in a content stream, and made image XObjects (8.9.7, ROADMAP Y-10).
//!
//! **An inline image is a picture in the middle of an operator stream**: a dictionary of
//! abbreviated keys between `BI` and `ID`, then the samples, then `EI`. A walk over the
//! stream's tokens reads those samples as tokens, so nothing that walks one — the run
//! reader, the image redaction — could reach the picture. A redaction lifts each inline
//! image on the page into an image XObject first, with its keys and names spelled out
//! (Tables 91 and 92), and the page draws it with `Do`: what it draws is the same, and
//! the picture is then where the rest of the redaction can blank it.
//!
//! **Where the samples end is counted where it can be**: an unfiltered image is
//! `H × ⌈W × components × bits ÷ 8⌉` bytes long, and the bytes `EI` can occur inside it.
//! A filtered one is read to the first `EI` standing alone, as 8.9.7 leaves a reader to.

use super::target::Target;
use fepdf_model::arena::PdfArena;
use fepdf_model::inline_image::{self, device_components, entries};
use fepdf_model::lexer::{Lexer, Token};
use fepdf_model::{Document, Handle, Object, PdfResult};
use std::ops::Range;
use std::sync::Arc;

/// One inline image in a content stream, by byte ranges.
pub struct Found {
    /// From `B` of `BI` to past `I` of `EI`.
    pub whole: Range<usize>,
    /// Between `BI` and `ID`: the abbreviated dictionary.
    pub header: Range<usize>,
    /// The samples, as written.
    pub data: Range<usize>,
}

/// Every inline image in `content`. `components` answers how many colour components a
/// colour space named from the page's resources has, where it can.
pub fn locate(content: &[u8], components: &dyn Fn(&str) -> Option<usize>) -> Vec<Found> {
    let mut lexer = Lexer::new(bytes::Bytes::copy_from_slice(content));
    let mut found = Vec::new();
    while let Ok(token) = lexer.next_token() {
        match token {
            Token::EOF => break,
            Token::Keyword(ref k) if k == "BI" => {}
            _ => continue,
        }
        let start = lexer.pos().saturating_sub(2);
        let Some(one) = image_at(content, &mut lexer, start, components) else { break };
        lexer.set_pos(one.whole.end);
        found.push(one);
    }
    found
}

/// The inline image whose `BI` ends where `lexer` stands.
fn image_at(
    content: &[u8],
    lexer: &mut Lexer,
    start: usize,
    components: &dyn Fn(&str) -> Option<usize>,
) -> Option<Found> {
    let header_start = lexer.pos();
    let header_end = loop {
        let before = lexer.pos();
        match lexer.next_token().ok()? {
            Token::Keyword(ref k) if k == "ID" => break before,
            Token::EOF => return None,
            _ => {}
        }
    };
    // One white-space character follows `ID` (8.9.7).
    let data_start = lexer.pos() + 1;
    let header = &content[header_start..header_end];
    let counted = unfiltered_length(header, components)
        .map(|n| data_start + n)
        .filter(|end| ei_after(content, *end).is_some());
    let (data_end, ei) = match counted {
        Some(end) => (end, ei_after(content, end)?),
        None => standalone_ei(content, data_start)?,
    };
    Some(Found {
        whole: start..ei + 2,
        header: header_start..header_end,
        data: data_start..data_end,
    })
}

/// Where `EI` stands after `at` with only white space between, if it does.
fn ei_after(content: &[u8], at: usize) -> Option<usize> {
    let ei = at + content.get(at..)?.iter().take_while(|b| b.is_ascii_whitespace()).count();
    (content.get(ei..ei + 2) == Some(b"EI") && ends_token(content, ei + 2)).then_some(ei)
}

/// The first `EI` after `from` with white space before it and nothing of a token after
/// it: where the samples end, and where it stands.
fn standalone_ei(content: &[u8], from: usize) -> Option<(usize, usize)> {
    (from..content.len().saturating_sub(2)).find_map(|at| {
        let before = content.get(at).is_some_and(u8::is_ascii_whitespace);
        (before && content.get(at + 1..at + 3) == Some(b"EI") && ends_token(content, at + 3))
            .then_some((at, at + 1))
    })
}

/// Whether nothing at `at` continues a token: the end, white space or a delimiter.
fn ends_token(content: &[u8], at: usize) -> bool {
    content.get(at).is_none_or(|b| b.is_ascii_whitespace() || b"()<>[]{}/%".contains(b))
}

/// How many bytes an unfiltered image's samples take, where the header says enough.
fn unfiltered_length(header: &[u8], components: &dyn Fn(&str) -> Option<usize>) -> Option<usize> {
    let scratch = PdfArena::new();
    let entries = entries(header, &scratch);
    let get = |short: &str, long: &str| {
        entries.iter().find(|(k, _)| k == short || k == long).map(|(_, v)| v.clone())
    };
    if get("F", "Filter").is_some() {
        return None;
    }
    let number = |short, long| {
        get(short, long).and_then(|v| v.as_integer()).and_then(|n| usize::try_from(n).ok())
    };
    let (width, height) = (number("W", "Width")?, number("H", "Height")?);
    let stencil = get("IM", "ImageMask").and_then(|v| v.as_bool()) == Some(true);
    let (count, bits) = if stencil {
        (1, 1)
    } else {
        let name = |o: &Object| o.as_name().and_then(|n| scratch.get_name_str(n));
        let count = match get("CS", "ColorSpace")? {
            Object::Array(a) => {
                let first = scratch.get_array(a)?.first().and_then(name)?;
                matches!(first.as_str(), "I" | "Indexed").then_some(1)?
            }
            other => device_components(&name(&other)?).or_else(|| components(&name(&other)?))?,
        };
        (count, number("BPC", "BitsPerComponent")?)
    };
    Some(height * (width * count * bits).div_ceil(8))
}

/// Lifts every inline image in `target`'s content into an image XObject, drawn with `Do`
/// where the image was.
///
/// # Errors
/// Fails when the page is not there or the content cannot be read.
pub fn lift(doc: &Document, target: Target) -> PdfResult<()> {
    let Some(content) = target.content(doc)? else { return Ok(()) };
    let found = locate(&content, &|name| resource_components(doc, target, name));
    if found.is_empty() {
        return Ok(());
    }
    let mut out = Vec::with_capacity(content.len());
    let mut at = 0;
    for image in found {
        out.extend_from_slice(&content[at..image.whole.start]);
        let header = &content[image.header.clone()];
        let xobject = xobject_of(doc, target, header, &content[image.data.clone()]);
        let name = super::image_crop::name_in(doc, target.resources(doc)?, xobject);
        out.extend_from_slice(format!("/{name} Do").as_bytes());
        at = image.whole.end;
    }
    out.extend_from_slice(&content[at..]);
    target.write(doc, out)
}

/// The image XObject an inline image's header and samples make: keys and names spelled
/// out, a colour space named from the resources taken from them, the samples as written.
fn xobject_of(doc: &Document, target: Target, header: &[u8], data: &[u8]) -> Handle<Object> {
    let arena = doc.arena();
    let (dict, _) = inline_image::dictionary(header, arena, &|name| named_space(doc, target, name));
    let stream = Object::Stream(
        arena.alloc_dict(dict),
        Arc::new(fepdf_model::object::SublimatedData::Raw(bytes::Bytes::copy_from_slice(data))),
    );
    arena.alloc_object(stream)
}

/// The colour space `target`'s resources name `name`.
fn named_space(doc: &Document, target: Target, name: &str) -> Option<Object> {
    let arena = doc.arena();
    let resources = target.resources_read(doc).ok()?;
    let spaces =
        arena.dict_entry(resources, arena.name("ColorSpace"))?.resolve(arena).as_dict_handle()?;
    arena.dict_entry(spaces, arena.name(name))
}

/// How many components the colour space `target`'s resources name `name` has.
pub fn resource_components(doc: &Document, target: Target, name: &str) -> Option<usize> {
    let arena = doc.arena();
    let space = named_space(doc, target, name)?.resolve(arena);
    if let Some(device) = space.as_name().and_then(|n| arena.get_name_str(n)) {
        return device_components(&device);
    }
    let items = arena.get_array(space.as_array()?)?;
    let family = items.first()?.as_name().and_then(|n| arena.get_name_str(n))?;
    match family.as_str() {
        "Indexed" | "Separation" | "CalGray" => Some(1),
        "CalRGB" | "Lab" => Some(3),
        "DeviceN" => items
            .get(1)?
            .resolve(arena)
            .as_array()
            .and_then(|a| arena.get_array(a))
            .map(|n| n.len()),
        "ICCBased" => {
            let stream = items.get(1)?.as_reference().and_then(|h| arena.get_object(h))?;
            let dict = stream.as_dict_handle()?;
            arena
                .dict_entry(dict, arena.name("N"))?
                .as_integer()
                .and_then(|n| usize::try_from(n).ok())
        }
        _ => None,
    }
}
