//! Icons: Text (Table 175), FileAttachment (Table 187), Sound (Table 188), and the caret of
//! Table 183.
//!
//! **The standard names these icons and does not draw them.** A reader "shall provide
//! predefined icon appearances" for the names it lists. Each is drawn here as a simple
//! line drawing on a grid twenty units square, scaled into the rectangle and centred in
//! it. An unknown name gets the subtype's default, as the tables say: Note, PushPin and
//! Speaker.

use super::{Area, Drawing, Entries};
use std::fmt::Write as _;

/// A path in grid units, and whether it is filled rather than stroked.
struct Stroke {
    path: &'static str,
    filled: bool,
}

const fn line(path: &'static str) -> Stroke {
    Stroke { path, filled: false }
}

const fn solid(path: &'static str) -> Stroke {
    Stroke { path, filled: true }
}

/// The drawing of an icon subtype, or of a caret.
pub(super) fn icon(entries: &Entries<'_>, area: Area, subtype: &str) -> Drawing {
    if subtype == "Caret" {
        return caret(entries, area);
    }
    let name = entries.name("Name");
    let strokes = match subtype {
        "FileAttachment" => attachment(name.as_deref().unwrap_or("PushPin")),
        "Sound" => sound(name.as_deref().unwrap_or("Speaker")),
        _ => note(name.as_deref().unwrap_or("Note")),
    };
    // The background is `/C`, which Table 166 makes the colour of a closed icon; a pale
    // yellow where there is none, as readers draw a note.
    let background = entries.colour(false).unwrap_or_else(|| "1 0.95 0.6 rg\n".to_owned());
    let side = area.width().min(area.height());
    let scale = side / 20.0;
    let (dx, dy) = ((area.width() - side) / 2.0, (area.height() - side) / 2.0);
    let mut content = format!(
        "q\n{scale:.4} 0 0 {scale:.4} {dx:.2} {dy:.2} cm\n{background}1 1 18 18 re f\n0 G 0 g 1.2 w 1 J 1 j\n"
    );
    for s in strokes {
        content.push_str(s.path);
        content.push_str(if s.filled { " f\n" } else { " S\n" });
    }
    content.push_str("Q\n");
    Drawing::of(content)
}

/// Table 175's names: Comment, Key, Note, Help, NewParagraph, Paragraph, Insert; and the
/// eight ISO 19444-1 Table 8 adds: Check, Circle, Cross, RightArrow, RightPointer, Star,
/// UpArrow, UpLeftArrow.
fn note(name: &str) -> Vec<Stroke> {
    match name {
        "Check" => vec![line("4 10 m 8 5 l 16 15 l")],
        "Circle" => vec![line(
            "10 16 m 13.3 16 16 13.3 16 10 c 16 6.7 13.3 4 10 4 c 6.7 4 4 6.7 4 10 c 4 13.3 6.7 16 10 16 c h",
        )],
        "Cross" => vec![line("4 4 m 16 16 l"), line("4 16 m 16 4 l")],
        "RightArrow" => vec![line("3 10 m 16 10 l"), line("11 15 m 16 10 l 11 5 l")],
        "RightPointer" => vec![solid("4 4 m 17 10 l 4 16 l 7 10 l h")],
        "Star" => vec![solid(
            "10 17 m 12 12 l 17 12 l 13 9 l 14.5 3.5 l 10 7 l 5.5 3.5 l 7 9 l 3 12 l 8 12 l h",
        )],
        "UpArrow" => vec![line("10 3 m 10 16 l"), line("5 11 m 10 16 l 15 11 l")],
        "UpLeftArrow" => vec![line("16 4 m 5 15 l"), line("5 9 m 5 15 l 11 15 l")],
        "Comment" => vec![line("3 7 m 3 16 l 17 16 l 17 7 l 9 7 l 5 3 l 6 7 l h")],
        "Key" => vec![
            line(
                "7 13 m 7 15.2 8.8 17 11 17 c 13.2 17 15 15.2 15 13 c 15 10.8 13.2 9 11 9 c 8.8 9 7 10.8 7 13 c h",
            ),
            line("9 10 m 3 4 l 5 2 l"),
            line("5 6 m 7 4 l"),
        ],
        "Help" => vec![line("6 13 m 6 16 14 16 14 13 c 14 10 10 11 10 8 c"), solid("9 3 2 2 re")],
        "NewParagraph" => {
            vec![line("6 9 m 10 16 l 14 9 l"), line("10 3 m 10 8 l"), line("8 3 m 8 6 l")]
        }
        "Paragraph" => pilcrow(),
        "Insert" => vec![line("4 4 m 10 16 l 16 4 l")],
        _ => vec![
            line("5 3 m 5 17 l 12 17 l 15 14 l 15 3 l h"),
            line("12 17 m 12 14 l 15 14 l"),
            line("7 11 m 13 11 l"),
            line("7 8 m 13 8 l"),
            line("7 5 m 11 5 l"),
        ],
    }
}

/// Table 187's names: Graph, PushPin, Paperclip, Tag.
fn attachment(name: &str) -> Vec<Stroke> {
    match name {
        "Graph" => vec![
            line("3 3 m 3 17 l"),
            line("3 3 m 17 3 l"),
            solid("5 3 3 6 re"),
            solid("10 3 3 10 re"),
            solid("15 3 2 13 re"),
        ],
        "Paperclip" => {
            vec![line("8 5 m 8 15 l 8 17.5 13 17.5 13 15 c 13 6 l 13 3.5 10 3.5 10 6 c 10 13 l")]
        }
        "Tag" => vec![line("3 10 m 9 16 l 17 16 l 17 4 l 9 4 l h"), solid("12.5 9.5 2 2 re")],
        _ => vec![
            line("10 2 m 10 9 l"),
            line("6 9 m 14 9 l"),
            line("7 9 m 8 15 l 12 15 l 13 9 l"),
            line("7 15 m 13 15 l 13 17 l 7 17 l h"),
        ],
    }
}

/// Table 188's names: Speaker, Mic; and Ear, which ISO 19444-1 Table 16 adds.
fn sound(name: &str) -> Vec<Stroke> {
    match name {
        "Ear" => vec![
            line(
                "7 12 m 7 15.5 9.5 18 12.5 18 c 15.5 18 17 15.5 17 13 c 17 9 12.5 9 12.5 5.5 c 12.5 3 10 2 8.5 3 c",
            ),
            line("10 12 m 10 14 12.5 14.5 13.5 13 c"),
        ],
        "Mic" => vec![
            line("8 9 m 8 16 l 8 18 12 18 12 16 c 12 9 l 12 7 8 7 8 9 c h"),
            line("5 11 m 5 4 15 4 15 11 c"),
            line("10 5 m 10 2 l"),
            line("7 2 m 13 2 l"),
        ],
        _ => vec![
            solid("3 7 m 7 7 l 12 3 l 12 17 l 7 13 l 3 13 l h"),
            line("14 7 m 16 9 16 11 14 13 c"),
            line("15.5 5 m 19 8 19 12 15.5 15 c"),
        ],
    }
}

/// A stand-in for media this engine does not play (ADR-0120): a frame, and a play mark,
/// or a cube for 3D. The real appearance of a Movie, Screen, 3D or RichMedia annotation is
/// a still of its content.
pub(super) fn media(area: Area, cube: bool) -> Drawing {
    let side = area.width().min(area.height());
    let scale = side / 20.0;
    let (dx, dy) = ((area.width() - side) / 2.0, (area.height() - side) / 2.0);
    let sign = if cube {
        "5 5 m 13 5 l 13 13 l 5 13 l h S\n5 13 m 8 16 l 16 16 l 13 13 l S\n13 5 m 16 8 l 16 16 l S\n"
    } else {
        "7 5 m 15 10 l 7 15 l h f\n"
    };
    let frame = format!(
        "0.85 g 0 0 {:.2} {:.2} re f\n0.4 G 1 w 0.5 0.5 {:.2} {:.2} re S\n",
        area.width(),
        area.height(),
        area.width() - 1.0,
        area.height() - 1.0
    );
    Drawing::of(format!(
        "{frame}q\n{scale:.4} 0 0 {scale:.4} {dx:.2} {dy:.2} cm\n0.3 G 0.3 g 1.2 w\n{sign}Q\n"
    ))
}

/// A pilcrow, ¶, drawn rather than set, so no face is needed for it.
fn pilcrow() -> Vec<Stroke> {
    vec![
        solid("8 17 m 5 17 3 15 3 13 c 3 11 5 9 8 9 c h"),
        line("8 17 m 8 3 l"),
        line("12 17 m 12 3 l"),
        line("8 17 m 15 17 l"),
    ]
}

/// A caret: an inverted V filled in `/C`, inside `/Rect` less `/RD`, with a pilcrow to its
/// right when `/Sy` is `P` (Table 183).
fn caret(entries: &Entries<'_>, area: Area) -> Drawing {
    let [left, top, right, bottom] = entries.inset();
    let (wide, high) = (area.width() - left - right, area.height() - top - bottom);
    let paragraph = entries.name("Sy").as_deref() == Some("P");
    let caret_wide = if paragraph { wide * 0.6 } else { wide };
    let colour = entries.colour(false).unwrap_or_else(|| "0 0 1 rg\n".to_owned());
    let apex = left + caret_wide / 2.0;
    let mut content = format!(
        "{colour}{left:.2} {bottom:.2} m {apex:.2} {:.2} l {:.2} {bottom:.2} l {apex:.2} {:.2} l h f\n",
        bottom + high,
        left + caret_wide,
        high.mul_add(0.35, bottom),
    );
    if paragraph {
        let side = (wide - caret_wide).min(high);
        let scale = side / 20.0;
        let _ = writeln!(
            content,
            "q\n{scale:.4} 0 0 {scale:.4} {:.2} {bottom:.2} cm\n0 G 0 g 1.2 w",
            left + caret_wide
        );
        for s in pilcrow() {
            content.push_str(s.path);
            content.push_str(if s.filled { " f\n" } else { " S\n" });
        }
        content.push_str("Q\n");
    }
    Drawing::of(content)
}
