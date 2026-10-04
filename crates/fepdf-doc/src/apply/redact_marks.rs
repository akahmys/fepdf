//! What a redaction does to marked content: the replacement text a sequence carries, and
//! which sequences lose their content (14.6, 14.9.4, ROADMAP Y-10).
//!
//! **A sequence's properties can say what its content reads.** `/Span <</ActualText
//! (…)>> BDC … EMC` carries the words the glyphs inside show, and removing the glyphs
//! leaves the words — MuPDF's redaction is reported doing exactly that. A sequence whose
//! content a region meets has its `/ActualText`, `/Alt` and `/E` replaced by
//! [`MARKER`], as iText pdfSweep 5.0.8 does (decided by the owner 2026-10-03).
//!
//! **And the structure tree reads the same sequences by MCID.** Each one's content is
//! classed against the regions — untouched, touched in part, or gone whole — before any
//! of it is removed, so the tree can be told which marks went and which lost something.

use super::target::Target;
use super::text::GlyphBox;
use fepdf_model::lexer::Token;
use fepdf_model::{Document, Object, PdfResult};
use kurbo::Rect;
use std::collections::BTreeMap;

/// What replaces replacement text that read what a region covered.
pub const MARKER: &str = "[REDACTED]";

/// The keys that carry what content reads, or stands for (14.9.3, 14.9.4, 14.9.5).
const SAYING: [&str; 3] = ["ActualText", "Alt", "E"];

/// What became of a marked-content sequence's content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fate {
    /// A region met some of it.
    Touched,
    /// Regions covered all of it.
    Gone,
}

/// One marked-content sequence: where it opens and closes among the tokens, and where its
/// properties are.
struct Sequence {
    /// The index of the first operand of `BDC` or `BMC`.
    from: usize,
    /// The index of `BDC` or `BMC`.
    open: usize,
    /// The index of the `EMC` that closes it.
    close: usize,
}

/// The MCIDs of `target`'s sequences whose content the regions met, each with its fate;
/// and the content rewritten so that every such sequence's replacement text is the marker.
///
/// # Errors
/// Fails when the content or a font cannot be read.
pub fn mark(
    doc: &Document,
    target: Target,
    regions: &[GlyphBox],
) -> PdfResult<BTreeMap<i64, Fate>> {
    let Some(data) = target.content(doc)? else { return Ok(BTreeMap::new()) };
    let tokens = super::image_crop::tokens_of(&data);
    let items = items(doc, target, &tokens)?;
    let regions: Vec<Rect> = regions.iter().map(|r| Rect::new(r.0, r.1, r.2, r.3)).collect();
    let mut fates = BTreeMap::new();
    let mut replaced: BTreeMap<usize, (usize, Vec<u8>)> = BTreeMap::new();
    for sequence in sequences(&tokens) {
        let Some(fate) = fate_of(&items, &sequence, &regions) else { continue };
        let operands = &tokens[sequence.from..sequence.open];
        if let Some(mcid) = mcid_of(doc, target, operands) {
            fates
                .entry(mcid)
                .and_modify(|f| {
                    if fate == Fate::Touched {
                        *f = fate;
                    }
                })
                .or_insert(fate);
        }
        if let Some(rewritten) = with_marker(doc, target, operands, &tokens[sequence.open])? {
            replaced.insert(sequence.from, (sequence.open, rewritten));
        }
    }
    if !replaced.is_empty() {
        target.write(doc, super::path_crop::rewritten(&tokens, &replaced))?;
    }
    Ok(fates)
}

/// Every drawn thing in the content, by the token index of the operator that draws it,
/// with where it lands.
fn items(doc: &Document, target: Target, tokens: &[Token]) -> PdfResult<Vec<(usize, Rect)>> {
    let glyphs = super::text::glyphs_by_operator(doc, target, super::redact::DESCENT)?;
    let mut items: Vec<(usize, Rect)> =
        glyphs.into_iter().map(|(at, g)| (at, Rect::new(g.0, g.1, g.2, g.3))).collect();
    items.extend(super::redact_images::image_extents(doc, target, tokens)?);
    items.extend(super::path_redact::painted_extents(tokens));
    for drawn in super::redact_forms::forms_drawn(doc, target, tokens)? {
        items.push((drawn.do_at, drawn.placed.transform_rect_bbox(drawn.bbox)));
    }
    Ok(items)
}

/// Every marked-content sequence, nested ones included.
fn sequences(tokens: &[Token]) -> Vec<Sequence> {
    let (mut open, mut out, mut operands_from) = (Vec::new(), Vec::new(), 0);
    for (index, token) in tokens.iter().enumerate() {
        let Token::Keyword(op) = token else { continue };
        match op.as_str() {
            "BDC" | "BMC" => open.push((operands_from, index)),
            "EMC" => {
                if let Some((from, at)) = open.pop() {
                    out.push(Sequence { from, open: at, close: index });
                }
            }
            _ => {}
        }
        operands_from = index + 1;
    }
    out
}

/// What became of `sequence`'s content: gone where every item in it lies inside a region,
/// touched where a region meets any; `None` where none does, or it draws nothing.
fn fate_of(items: &[(usize, Rect)], sequence: &Sequence, regions: &[Rect]) -> Option<Fate> {
    let inside: Vec<&Rect> = items
        .iter()
        .filter(|(at, _)| *at > sequence.open && *at < sequence.close)
        .map(|(_, r)| r)
        .collect();
    let meets = |r: &Rect| regions.iter().any(|g| g.overlaps(*r));
    let covered =
        |r: &Rect| regions.iter().any(|g| g.contains_rect(*r) || r.area() == 0.0 && g.overlaps(*r));
    if !inside.iter().any(|r| meets(r)) {
        return None;
    }
    Some(if inside.iter().all(|r| covered(r)) { Fate::Gone } else { Fate::Touched })
}

/// The property list `/Properties` in `target`'s resources names `name`.
fn named_list(doc: &Document, target: Target, name: &[u8]) -> Option<fepdf_model::DictHandle> {
    let arena = doc.arena();
    let resources = target.resources_read(doc).ok()?;
    let lists =
        arena.dict_entry(resources, arena.name("Properties"))?.resolve(arena).as_dict_handle()?;
    arena
        .dict_entry(lists, arena.name(&String::from_utf8_lossy(name)))?
        .resolve(arena)
        .as_dict_handle()
}

/// The `/MCID` a `BDC`'s properties carry: written in place, read in a scratch arena so
/// that reading writes nothing, or named from the resources.
fn mcid_of(doc: &Document, target: Target, operands: &[Token]) -> Option<i64> {
    let arena = doc.arena();
    match operands.get(1)? {
        Token::LeftDict => {
            let mut bytes = Vec::new();
            for token in &operands[1..] {
                token.write_to(&mut bytes);
            }
            let scratch = fepdf_model::PdfArena::new();
            let parsed =
                fepdf_model::parser::Parser::new(bytes.into(), &scratch).parse_object().ok()?;
            scratch.dict_entry(parsed.as_dict_handle()?, scratch.name("MCID"))?.as_integer()
        }
        Token::Name(name) => {
            arena.dict_entry(named_list(doc, target, name)?, arena.name("MCID"))?.as_integer()
        }
        _ => None,
    }
}

/// A `BDC`'s operands and operator with its replacement text the marker, or `None` where
/// it carries none.
fn with_marker(
    doc: &Document,
    target: Target,
    operands: &[Token],
    op: &Token,
) -> PdfResult<Option<Vec<u8>>> {
    let says = |name: &[u8]| SAYING.iter().any(|k| k.as_bytes() == name);
    let mut out = Vec::new();
    match operands.get(1) {
        Some(Token::LeftDict) => {
            let mut changed = false;
            let mut after_key = false;
            for token in operands {
                if after_key && matches!(token, Token::String(_) | Token::Hex(_)) {
                    Token::String(bytes::Bytes::from(MARKER)).write_to(&mut out);
                    changed = true;
                } else {
                    token.write_to(&mut out);
                }
                after_key = matches!(token, Token::Name(n) if says(n));
            }
            if !changed {
                return Ok(None);
            }
        }
        Some(Token::Name(name)) => {
            mark_list(doc, target, name);
            return Ok(None);
        }
        _ => return Ok(None),
    }
    op.write_to(&mut out);
    Ok(Some(out))
}

/// Makes the marker the replacement text of the property list `/Properties` names
/// `name`, in place.
///
/// **In place, not a copy**: a copy under a new name left the original in the resources,
/// and the writer wrote it with what it said. Another sequence naming the same list loses
/// its replacement text too, which errs towards removing.
fn mark_list(doc: &Document, target: Target, name: &[u8]) {
    let arena = doc.arena();
    let Some(list) = named_list(doc, target, name) else { return };
    let mut dict = arena.get_dict(list).unwrap_or_default();
    let mut changed = false;
    for key in SAYING {
        if let Some(value) = dict.get_mut(&arena.name(key)) {
            *value = Object::String(bytes::Bytes::from(MARKER));
            changed = true;
        }
    }
    if changed {
        arena.set_dict(list, dict);
    }
}
