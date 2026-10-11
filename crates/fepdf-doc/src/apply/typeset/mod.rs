//! Pages made from plain text (ROADMAP AA-6, [ADR-0124]).
//!
//! **Paragraphs are what the text declares, and nothing is inferred** ([ADR-0091]): a
//! blank line ends one. A line break inside a paragraph is kept — a plain text file's
//! lines are its writer's — and a form feed starts a page. Lines break where UAX #14
//! allows, as far as the face's advances let them reach (`lines`); pages break where
//! they fill, a paragraph running on to the next where it must; each paragraph is a `/P`
//! element (`tagging`).
//!
//! **One face sets the whole text**, the one the ladder every text this engine sets goes
//! through finds for all its characters ([ADR-0089]), embedded once for every page.
//!
//! [ADR-0124]: ../../../../../docs/adr/0124-plain-text-is-set-by-its-own-lines-and-paragraphs.md
//! [ADR-0091]: ../../../../../docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md
//! [ADR-0089]: ../../../../../docs/adr/0089-a-face-is-embedded-only-where-it-permits-it.md

mod lines;
mod tagging;

use crate::operation::TextSetting;
use bytes::Bytes;
use fepdf_model::object::SublimatedData;
use fepdf_model::{Document, Handle, Object, PdfArena, PdfError, PdfName, PdfResult};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Arc;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// One page's part of a paragraph: which paragraph, and its lines with their baselines.
struct Piece {
    paragraph: usize,
    lines: Vec<(String, f64)>,
}

/// The text of a plain text file: UTF-8, its byte order mark dropped, or UTF-16 that
/// begins with one.
///
/// # Errors
/// Refused when it is neither, which is what a file in a legacy encoding — Shift_JIS,
/// Latin-1 — is: guessing would set text nobody wrote.
pub fn plain_text(bytes: &[u8]) -> PdfResult<String> {
    let refuse = || {
        PdfError::refused(
            "InsertText",
            "the text is not UTF-8, nor UTF-16 with a byte order mark".to_owned(),
        )
    };
    let utf16 = |rest: &[u8], big: bool| {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| if big { u16::from_be_bytes(pair) } else { u16::from_le_bytes(pair) })
            .collect();
        String::from_utf16(&units).map_err(|_| refuse())
    };
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8(rest.to_vec()).map_err(|_| refuse()),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, true),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, false),
        _ => String::from_utf8(bytes.to_vec()).map_err(|_| refuse()),
    }
}

/// Puts pages of `text` in at `at`, set as `setting` says; how many.
///
/// # Errors
/// Refused when there is no text, the setting leaves no room for a line, or no face here
/// draws every character.
pub fn apply_insert_text(
    doc: &mut Document,
    text: &str,
    at: usize,
    setting: &TextSetting,
) -> PdfResult<usize> {
    let refuse = |why: String| PdfError::refused("InsertText", why);
    let room = room(setting).map_err(refuse)?;
    let text = normalised(text);
    let visible: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if visible.is_empty() {
        return Err(refuse("there is no text to set".to_owned()));
    }
    let (base, program) = crate::apply::font::face_for(&visible)
        .map_err(|why| refuse(format!("no face here draws the text: {why}")))?;
    let measure = lines::Measure::new(&program, f64::from(setting.font_size));
    let pages = laid_out(&text, &measure, setting, room);
    let every: Vec<&str> =
        pages.iter().flatten().flat_map(|p| p.lines.iter().map(|(l, _)| l.as_str())).collect();
    let embedded = crate::apply::font::embed_for(doc, &program, &base, &every)?;
    let arena = doc.arena();
    let mut parts: Vec<tagging::Tagged> = Vec::new();
    let mut handles = Vec::with_capacity(pages.len());
    for pieces in &pages {
        let page = page(arena, (pieces, &embedded, &program), setting)?;
        for (mcid, piece) in pieces.iter().enumerate() {
            if parts.len() <= piece.paragraph {
                parts.resize_with(piece.paragraph + 1, || tagging::Tagged { parts: Vec::new() });
            }
            if let Some(paragraph) = parts.get_mut(piece.paragraph) {
                paragraph.parts.push((page, mcid));
            }
        }
        handles.push(page);
    }
    tagging::tag(doc, &parts, &handles, setting.lang.as_deref())?;
    let clamped = at.min(doc.pages.len());
    for (nth, page) in handles.iter().enumerate() {
        doc.pages.insert(clamped + nth, *page);
    }
    doc.rebuild_page_tree_in_arena()?;
    Ok(handles.len())
}

/// The width a line may take and how many lines a page holds, or why there is no room.
fn room(setting: &TextSetting) -> Result<(f64, usize), String> {
    let [w, h] = setting.sheet.map(f64::from);
    let (margin, size) = (f64::from(setting.margin), f64::from(setting.font_size));
    let leading = size * f64::from(setting.leading);
    if !(size > 0.0 && leading > 0.0 && margin >= 0.0) {
        return Err(format!("a size of {size}, leading {leading} and margin {margin} set nothing"));
    }
    let (across, down) = ((-2.0f64).mul_add(margin, w), (-2.0f64).mul_add(margin, h));
    if across < size || down < size {
        return Err(format!("a sheet {w} by {h} with margins of {margin} has no room for a line"));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a page holds few lines
    let count = ((down - size) / leading).floor() as usize + 1;
    Ok((across, count))
}

/// The text with its line ends made line feeds, its tabs four spaces, and every other
/// control character but the form feed taken out.
fn normalised(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\u{c}'))
        .collect()
}

/// The pages' pieces: each section a form feed ends starts a page, each paragraph a
/// blank line ends is broken into lines, and a blank line's height is left between
/// paragraphs, though not at the top of a page.
fn laid_out(
    text: &str,
    measure: &lines::Measure<'_>,
    setting: &TextSetting,
    (across, per_page): (f64, usize),
) -> Vec<Vec<Piece>> {
    let mut pages: Vec<Vec<Piece>> = Vec::new();
    let mut paragraph = 0;
    for section in text.split('\u{c}') {
        let mut used = per_page; // a section starts a page
        for block in paragraphs(section) {
            let mut piece: Option<Piece> = None;
            if used > 0 && used < per_page {
                used += 1;
            }
            for line in lines::broken(&block, measure, across) {
                if used >= per_page {
                    if let (Some(done), Some(last)) = (piece.take(), pages.last_mut()) {
                        last.push(done);
                    }
                    pages.push(Vec::new());
                    used = 0;
                }
                let y = baseline(setting, used);
                piece
                    .get_or_insert_with(|| Piece { paragraph, lines: Vec::new() })
                    .lines
                    .push((line, y));
                used += 1;
            }
            if let (Some(done), Some(last)) = (piece, pages.last_mut()) {
                last.push(done);
            }
            paragraph += 1;
        }
    }
    pages
}

/// A section's paragraphs: its runs of lines that are not blank, each with its line
/// breaks.
fn paragraphs(section: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in section.split('\n') {
        if line.trim().is_empty() {
            if !current.is_empty() {
                found.push(current.join("\n"));
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        found.push(current.join("\n"));
    }
    found
}

/// The baseline of line `nth` on a page: the first a size's ascent below the top margin.
fn baseline(setting: &TextSetting, nth: usize) -> f64 {
    let size = f64::from(setting.font_size);
    let [_, high] = setting.sheet;
    let top = size.mul_add(-0.88, f64::from(high) - f64::from(setting.margin));
    #[allow(clippy::cast_precision_loss)] // a page holds few lines
    let down = nth as f64 * size * f64::from(setting.leading);
    top - down
}

/// A page holding `pieces`, each marked as its paragraph's content with its own `/MCID`.
///
/// # Errors
/// When a line has a character the embedded face does not.
fn page(
    arena: &PdfArena,
    (pieces, embedded, program): (&[Piece], &crate::apply::font::Embedded, &[u8]),
    setting: &TextSetting,
) -> PdfResult<Handle<Object>> {
    let size = setting.font_size;
    let left = setting.margin;
    let mut content = String::new();
    for (mcid, piece) in pieces.iter().enumerate() {
        let _ = writeln!(content, "/P <</MCID {mcid}>> BDC\nBT\n/F1 {size:.2} Tf");
        for (line, y) in &piece.lines {
            if line.is_empty() {
                continue;
            }
            let codes = crate::apply::markup::codes_of(program, embedded, line)?;
            let _ = writeln!(content, "1 0 0 1 {left:.2} {y:.2} Tm <{codes}> Tj");
        }
        content.push_str("ET\nEMC\n");
    }
    let stream = arena.alloc_object(Object::Stream(
        arena.alloc_dict(Dict::new()),
        Arc::new(SublimatedData::Raw(Bytes::from(content))),
    ));
    let mut fonts = Dict::new();
    fonts.insert(arena.name("F1"), Object::Reference(embedded.font));
    let mut resources = Dict::new();
    resources.insert(arena.name("Font"), Object::Dictionary(arena.alloc_dict(fonts)));
    let [w, h] = setting.sheet.map(f64::from);
    let media = vec![Object::Real(0.0), Object::Real(0.0), Object::Real(w), Object::Real(h)];
    let mut dict = Dict::new();
    dict.insert(arena.name("Type"), Object::Name(arena.name("Page")));
    dict.insert(arena.name("MediaBox"), Object::Array(arena.alloc_array(media)));
    dict.insert(arena.name("Resources"), Object::Dictionary(arena.alloc_dict(resources)));
    dict.insert(arena.name("Contents"), Object::Reference(stream));
    Ok(arena.alloc_object(Object::Dictionary(arena.alloc_dict(dict))))
}
