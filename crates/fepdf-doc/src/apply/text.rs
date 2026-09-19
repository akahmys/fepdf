//! Changing the text a page already draws.
//!
//! **The unit is a run**, which is one show-text operator, and the rewrite happens on the
//! token stream — the same ground redaction works on
//! ([ADR-0064](../../../../docs/adr/0064-redaction-removed-the-second-run-of-a-page-and-no-other.md)),
//! and for the same reason: it is where the bytes are, whatever form the stream is held in
//! elsewhere.
//!
//! **A run is found by what it reads, not by where it is.** `TextSpan.op_index` is the
//! obvious way to name one and it carries nothing on the default path — 1007 spans of
//! `samples/constitution.pdf` all report 0 with refinement on, and 1007 distinct indices
//! with it off. So this walks the stream, tracks the font each run is set in, decodes each
//! run through that font, and compares.

use fepdf_model::font::FontResource;
use fepdf_model::lexer::{Lexer, Token};
use fepdf_model::{Document, Object, PdfError, PdfResult};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Replaces every run on `page` that reads exactly `find`.
///
/// # Errors
/// Fails when the page is not there, or when the font a matching run is set in cannot
/// draw a character of `replace` — naming the character.
pub fn apply_edit_text_run(
    doc: &Document,
    page: usize,
    find: &str,
    replace: &str,
) -> PdfResult<()> {
    let fonts = fonts_of_page(doc, page)?;
    let arena = doc.arena();
    let page_h =
        doc.get_page_handle(page).ok_or_else(|| PdfError::Other("the page is not there".into()))?;
    let page_dh = doc.resolve_to_dict(page_h)?;
    let mut page_dict = arena.get_dict(page_dh).unwrap_or_default();

    let Some(contents) = page_dict.get(&arena.name("Contents")).cloned() else { return Ok(()) };
    let data = crate::remediation::decode_page_contents(doc, &contents)?;
    let rewritten = rewrite_runs(&data, &fonts, find, replace)?;
    if rewritten == data.as_ref() {
        return Ok(());
    }

    // **One stream in place of however many there were**, which is what redaction does
    // with the same bytes: the page's content is the concatenation of its streams, and a
    // rewrite of the whole is one stream.
    let stream = arena.alloc_object(Object::Stream(
        arena.alloc_dict(BTreeMap::new()),
        Arc::new(fepdf_model::object::SublimatedData::Raw(bytes::Bytes::from(rewritten))),
    ));
    page_dict.insert(arena.name("Contents"), Object::Reference(stream));
    arena.set_dict(page_dh, page_dict);
    Ok(())
}

/// The fonts a page names, by the resource name its content stream uses.
fn fonts_of_page(doc: &Document, page: usize) -> PdfResult<BTreeMap<String, Arc<FontResource>>> {
    let arena = doc.arena();
    let page_h =
        doc.get_page_handle(page).ok_or_else(|| PdfError::Other("the page is not there".into()))?;
    let chain = doc.get_parent_chain(page_h);
    let view = fepdf_model::Page::new(arena, page_h, chain);
    let resources = arena.get_dict(view.resources_handle()).unwrap_or_default();

    let mut out = BTreeMap::new();
    let Some(fonts_dh) =
        resources.get(&arena.name("Font")).and_then(|o| o.resolve(arena).as_dict_handle())
    else {
        return Ok(out);
    };
    for (key, value) in arena.get_dict(fonts_dh).unwrap_or_default() {
        let Some(name) = arena.get_name(key) else { continue };
        if let Some(handle) = value.as_reference()
            && let Ok(font) = doc.get_font(handle)
        {
            out.insert(name.as_str().to_string(), font);
        }
    }
    Ok(out)
}

/// The stream, with every matching run's string replaced.
fn rewrite_runs(
    data: &[u8],
    fonts: &BTreeMap<String, Arc<FontResource>>,
    find: &str,
    replace: &str,
) -> PdfResult<Vec<u8>> {
    let mut lexer = Lexer::new(bytes::Bytes::copy_from_slice(data));
    let mut out = Vec::with_capacity(data.len());
    let mut operands: Vec<Token> = Vec::new();
    let mut current: Option<Arc<FontResource>> = None;

    while let Ok(token) = lexer.next_token() {
        if token == Token::EOF {
            break;
        }
        let Token::Keyword(ref op) = token else {
            operands.push(token);
            continue;
        };
        if op == "Tf" {
            current = font_named(&operands, fonts);
        }
        let shows_text = matches!(op.as_str(), "Tj" | "TJ" | "'" | "\"");
        for operand in std::mem::take(&mut operands) {
            match (&operand, shows_text, current.as_ref()) {
                (Token::String(s) | Token::Hex(s), true, Some(font)) if decode(font, s) == find => {
                    Token::String(encode(font, replace)?).write_to(&mut out);
                }
                _ => operand.write_to(&mut out),
            }
        }
        token.write_to(&mut out);
    }
    for operand in operands {
        operand.write_to(&mut out);
    }
    Ok(out)
}

/// The font a `Tf` names, out of the operands before it.
fn font_named(
    operands: &[Token],
    fonts: &BTreeMap<String, Arc<FontResource>>,
) -> Option<Arc<FontResource>> {
    // `/F1 12 Tf`: the name is the operand before the size.
    operands.iter().rev().find_map(|token| match token {
        Token::Name(name) => fonts.get(String::from_utf8_lossy(name).as_ref()).cloned(),
        _ => None,
    })
}

/// What a run's bytes read, through the font it is set in.
fn decode(font: &FontResource, bytes: &[u8]) -> String {
    let width = if font.is_cid_keyed { 2 } else { 1 };
    let by_code: BTreeMap<u32, &str> =
        font.unified_map.iter().map(|(text, code)| (*code, text.as_str())).collect();
    bytes
        .chunks(width)
        .filter_map(|chunk| {
            let code = chunk.iter().fold(0u32, |acc, byte| (acc << 8) | u32::from(*byte));
            by_code.get(&code).copied()
        })
        .collect()
}

/// `text`, in the codes this font draws it by.
///
/// **A character the font does not draw is refused.** Substituting one draws a different
/// letter, and writing the character's own bytes draws whatever glyph happens to sit at
/// that code — which is how 図面 became six Latin glyphs before this phase.
fn encode(font: &FontResource, text: &str) -> PdfResult<bytes::Bytes> {
    let width = if font.is_cid_keyed { 2 } else { 1 };
    let mut out = Vec::with_capacity(text.chars().count() * width);
    for character in text.chars() {
        let Some(code) = font.unified_map.get(&character.to_string()) else {
            return Err(PdfError::Other(
                format!("the font this run is set in does not draw {character:?}").into(),
            ));
        };
        if width == 2 {
            out.extend_from_slice(&u16::try_from(*code).unwrap_or(0).to_be_bytes());
        } else {
            out.push(u8::try_from(*code).unwrap_or(0));
        }
    }
    Ok(bytes::Bytes::from(out))
}
