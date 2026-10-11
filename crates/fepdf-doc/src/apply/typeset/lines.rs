//! Where a paragraph's lines break: at the opportunities UAX #14 gives, as far along as
//! the face's advances let a line reach.
//!
//! **Greedy, and no further.** Each line takes as much as fits, as a typewriter does; a
//! paragraph is not balanced against its next lines. A break the text itself makes — a
//! line feed — is mandatory (UAX #14, BK and LF). A run with no opportunity in it wider
//! than the line, a long address or a word in a narrow measure, is broken between
//! characters, which UAX #14 leaves to the implementation as a last resort.

use std::cell::RefCell;
use std::collections::BTreeMap;
use unicode_linebreak::{BreakOpportunity, linebreaks};

/// How wide text is in one face at one size.
pub(super) struct Measure<'a> {
    program: &'a [u8],
    /// Points to a font unit: the size over the face's units per em.
    scale: f64,
    widths: RefCell<BTreeMap<char, f64>>,
}

impl<'a> Measure<'a> {
    pub(super) fn new(program: &'a [u8], size: f64) -> Self {
        let units = fepdf_font::metrics::read_metrics(program)
            .map_or(1000.0, |m| f64::from(m.units_per_em))
            .max(1.0);
        Self { program, scale: size / units, widths: RefCell::new(BTreeMap::new()) }
    }

    /// How wide `text` is, in points.
    pub(super) fn width(&self, text: &str) -> f64 {
        text.chars().map(|c| self.advance(c)).sum::<f64>() * self.scale
    }

    fn advance(&self, c: char) -> f64 {
        if let Some(width) = self.widths.borrow().get(&c) {
            return *width;
        }
        let width = fepdf_font::subset::glyphs_for(self.program, &c.to_string())
            .ok()
            .and_then(|glyphs| glyphs.first().copied())
            .and_then(|glyph| fepdf_font::metrics::advance_width(self.program, glyph))
            .map_or(0.0, f64::from);
        self.widths.borrow_mut().insert(c, width);
        width
    }
}

/// The lines `paragraph` is set in, none wider than `room` points unless one character
/// is.
pub(super) fn broken(paragraph: &str, measure: &Measure<'_>, room: f64) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0;
    // The last opportunity at which what has been taken so far still fits.
    let mut fits: Option<usize> = None;
    for (at, opportunity) in linebreaks(paragraph) {
        let taken = paragraph.get(start..at).unwrap_or_default();
        if measure.width(taken.trim_end()) > room {
            if let Some(last) = fits.filter(|last| *last > start) {
                lines.push(trimmed(paragraph.get(start..last)));
                start = last;
            }
            start = by_characters(paragraph, (start, at), measure, room, &mut lines);
        }
        if opportunity == BreakOpportunity::Mandatory {
            lines.push(trimmed(paragraph.get(start..at)));
            start = at;
            fits = None;
        } else {
            fits = Some(at);
        }
    }
    lines
}

/// Lines cut between characters from `start` while what is left before `end` is too
/// wide, each as many characters as fit and at least one; where what is left begins.
fn by_characters(
    text: &str,
    (mut start, end): (usize, usize),
    measure: &Measure<'_>,
    room: f64,
    lines: &mut Vec<String>,
) -> usize {
    loop {
        let rest = text.get(start..end).unwrap_or_default();
        if measure.width(rest.trim_end()) <= room {
            break;
        }
        let mut cut = 0;
        for (at, c) in rest.char_indices() {
            let next = at + c.len_utf8();
            if cut > 0 && measure.width(rest.get(..next).unwrap_or_default()) > room {
                break;
            }
            cut = next;
        }
        if cut == 0 {
            break;
        }
        lines.push(trimmed(rest.get(..cut)));
        start += cut;
    }
    start
}

/// A line without the spaces and the line feed it ends with.
fn trimmed(line: Option<&str>) -> String {
    line.unwrap_or_default().trim_end().to_owned()
}
