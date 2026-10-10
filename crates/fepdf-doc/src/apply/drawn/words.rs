//! What sets text: FreeText (Table 177) and a stamp's name (Table 184).
//!
//! **Set in a face that draws every character, embedded**, through the ladder every text
//! this engine sets goes through ([ADR-0089]). Where no face here draws the words, the
//! frame is drawn without them and a `Decision` says so: an import should not fail for one
//! note's script.
//!
//! [ADR-0089]: ../../../../../docs/adr/0089-a-face-is-embedded-only-where-it-permits-it.md

use super::{Area, Drawing, Entries, local};
use fepdf_model::interpretation::Decision;
use fepdf_model::{Document, Object, PdfResult};
use std::fmt::Write as _;

/// Table 184's default, and what a stamp with no `/Name` says.
const STAMP_DEFAULT: &str = "Draft";

/// A free text annotation: its box (`/Rect` less `/RD`), border (`/BS`), callout line
/// (`/CL`, with `/LE` at its first point) and words (`/Contents` in `/DA`'s size and
/// colour, justified by `/Q`). A typewriter (`/IT /FreeTextTypeWriter`) has no border.
pub(super) fn free_text(doc: &Document, entries: &Entries<'_>, area: Area) -> PdfResult<Drawing> {
    let [l, t, r, b] = entries.inset();
    let text_box = Area { left: l, bottom: b, right: area.width() - r, top: area.height() - t };
    let pen = entries.pen();
    let mut content = String::new();
    let typewriter = entries.name("IT").as_deref() == Some("FreeTextTypeWriter");
    if !typewriter && pen.width > 0.0 {
        let half = pen.width / 2.0;
        let _ = writeln!(
            content,
            "0 G {}{:.2} {:.2} {:.2} {:.2} re S",
            pen.operators(),
            text_box.left + half,
            text_box.bottom + half,
            text_box.width() - pen.width,
            text_box.height() - pen.width
        );
    }
    content.push_str(&callout(entries, area));
    let da = entries.text("DA").unwrap_or_default();
    let size = super::super::appearance::parse_default_appearance(&da)
        .map(|d| d.size)
        .filter(|s| *s > 0.0)
        .unwrap_or(12.0);
    let colour = da_colour(&da);
    let words = entries.text("Contents").unwrap_or_default();
    let quadding = entries.number("Q").unwrap_or(0.0);
    let mut drawing = Drawing::of(content);
    set(
        doc,
        &mut drawing,
        &words,
        &Setting { inside: text_box, size, colour, quadding, top: true, fit: false },
    )?;
    Ok(drawing)
}

/// A stamp with no picture: its name in a rounded frame, in `/C` or red (Table 184).
pub(super) fn stamp(doc: &Document, entries: &Entries<'_>, area: Area) -> PdfResult<Drawing> {
    let name = entries.name("Name").unwrap_or_else(|| STAMP_DEFAULT.to_owned());
    let label = spaced(&name).to_uppercase();
    let colour = entries.colour(false).unwrap_or_else(|| "0.8 0 0 rg\n".to_owned());
    let stroke = colour.replace(" rg", " RG").replace(" g\n", " G\n").replace(" k\n", " K\n");
    let (w, h) = (area.width(), area.height());
    let inset = (h * 0.08).max(1.0);
    let content = format!(
        "{stroke}{:.2} w\n{inset:.2} {inset:.2} {:.2} {:.2} re S\n",
        inset,
        2.0f64.mul_add(-inset, w),
        2.0f64.mul_add(-inset, h)
    );
    // As large as half the height allows; `set` shrinks it to the width the face needs.
    let size = (h * 0.5).max(1.0);
    let inside = Area { left: inset * 2.0, bottom: inset, right: w - inset * 2.0, top: h - inset };
    let mut drawing = Drawing::of(content);
    let setting = Setting { inside, size, colour, quadding: 1.0, top: false, fit: true };
    set(doc, &mut drawing, &label, &setting)?;
    Ok(drawing)
}

/// A watermark's words (12.5.6.22), centred in grey, no wider than the rectangle.
///
/// # Errors
/// When the face that draws them will not embed.
pub(super) fn watermark(
    doc: &Document,
    drawing: &mut Drawing,
    text: &str,
    size: f64,
    area: Area,
) -> PdfResult<()> {
    let inside = Area { left: 0.0, bottom: 0.0, right: area.width(), top: area.height() };
    let top = text.lines().count() > 1;
    let setting =
        Setting { inside, size, colour: "0.5 g\n".to_owned(), quadding: 1.0, top, fit: true };
    set(doc, drawing, text, &setting)
}

/// "NotForPublicRelease" as "Not For Public Release", and ISO 19444-1 Table 14's
/// "SBNotApproved" and "SHSignHere" without the prefix that says which set they are from.
fn spaced(name: &str) -> String {
    let bare = ["SB", "SH"]
        .iter()
        .find_map(|p| name.strip_prefix(p).filter(|rest| rest.starts_with(char::is_uppercase)))
        .unwrap_or(name);
    let mut out = String::new();
    for (nth, c) in bare.chars().enumerate() {
        if nth > 0 && c.is_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// The colour operator of a `/DA` string, or black.
fn da_colour(da: &str) -> String {
    let tokens: Vec<&str> = da.split_whitespace().collect();
    for (at, operator) in tokens.iter().enumerate() {
        let count = match *operator {
            "g" => 1,
            "rg" => 3,
            "k" => 4,
            _ => continue,
        };
        let operands = at.checked_sub(count).and_then(|from| tokens.get(from..at));
        if let Some(operands) = operands {
            return format!("{} {operator}\n", operands.join(" "));
        }
    }
    "0 g\n".to_owned()
}

/// Where and how a block of words is set.
struct Setting {
    inside: Area,
    size: f64,
    colour: String,
    /// Table 177's `/Q`: 0 left, 1 centred, 2 right.
    quadding: f64,
    /// From the top of the box down, or centred on it as one line.
    top: bool,
    /// Whether the size shrinks until the widest line fits the box, as a stamp's name
    /// does; a free text's words keep `/DA`'s size and run past it, as Table 177 sets them.
    fit: bool,
}

/// Sets `words` into `drawing`, a line to each line, embedding the face that draws them.
/// Where none does, the words are left out and a `Decision` says so.
fn set(doc: &Document, drawing: &mut Drawing, words: &str, setting: &Setting) -> PdfResult<()> {
    let lines: Vec<&str> = words.lines().collect();
    if lines.concat().trim().is_empty() {
        return Ok(());
    }
    let Ok((base, program)) = crate::apply::font::face_for(&lines.concat()) else {
        doc.record(Decision::ambiguity(
            "12.5.5",
            format!("no face here draws the words of an imported annotation, {words:?}"),
            "drew its frame without them",
        ));
        return Ok(());
    };
    let embedded = crate::apply::font::embed_for(doc, &program, &base, &lines)?;
    let units =
        fepdf_font::metrics::read_metrics(&program).map_or(1000.0, |m| f64::from(m.units_per_em));
    let widest = lines.iter().map(|l| advance(&program, l, units)).fold(0.0, f64::max);
    let room = setting.inside.width() - 4.0;
    let size =
        if setting.fit && widest > 0.0 { setting.size.min(room / widest) } else { setting.size };
    let mut text = format!("BT\n/FW {size:.2} Tf {}", setting.colour);
    let mut baseline = if setting.top {
        size.mul_add(-0.88, setting.inside.top - 2.0)
    } else {
        // One line centred in the box: its capitals, about seven tenths of an em tall,
        // about the middle.
        size.mul_add(-0.35, f64::midpoint(setting.inside.bottom, setting.inside.top))
    };
    for line in &lines {
        if !line.is_empty() {
            let codes = super::super::markup::codes_of(&program, &embedded, line)?;
            let width = advance(&program, line, units) * size;
            let room = setting.inside.width() - 4.0 - width;
            let x = room
                .max(0.0)
                .mul_add(setting.quadding.clamp(0.0, 2.0) / 2.0, setting.inside.left + 2.0);
            let _ = writeln!(text, "1 0 0 1 {x:.2} {baseline:.2} Tm\n<{codes}> Tj");
        }
        baseline = size.mul_add(-1.2, baseline);
    }
    text.push_str("ET\n");
    drawing.content.push_str(&text);
    let arena = doc.arena();
    let mut fonts = super::Dict::new();
    fonts.insert(arena.name("FW"), Object::Reference(embedded.font));
    drawing.resources.insert(arena.name("Font"), Object::Dictionary(arena.alloc_dict(fonts)));
    Ok(())
}

/// How far `line` advances, in ems.
fn advance(program: &[u8], line: &str, units: f64) -> f64 {
    let glyphs = fepdf_font::subset::glyphs_for(program, line).unwrap_or_default();
    let total: f64 = glyphs
        .iter()
        .map(|g| f64::from(fepdf_font::metrics::advance_width(program, *g).unwrap_or(0)))
        .sum();
    total / units.max(1.0)
}

/// `/CL`, the callout line of Table 177, from its first point through any knee to its
/// last, with `/LE`'s ending at the first.
fn callout(entries: &Entries<'_>, area: Area) -> String {
    let points: Vec<(f64, f64)> =
        entries.numbers("CL").as_chunks::<2>().0.iter().map(|[x, y]| local(area, *x, *y)).collect();
    let (Some(first), Some(rest)) = (points.first(), points.get(1..)) else { return String::new() };
    if rest.is_empty() {
        return String::new();
    }
    let mut path = format!("0 G 1 w\n{:.2} {:.2} m\n", first.0, first.1);
    for p in rest {
        let _ = writeln!(path, "{:.2} {:.2} l", p.0, p.1);
    }
    path.push_str("S\n");
    if let (Some(style), Some(next)) = (entries.names("LE").first(), rest.first()) {
        let back = super::lines::unit(*first, *next).unwrap_or((1.0, 0.0));
        path.push_str(&super::lines::ending(
            style,
            *first,
            back,
            1.0,
            entries.interior().as_deref(),
        ));
    }
    path
}
