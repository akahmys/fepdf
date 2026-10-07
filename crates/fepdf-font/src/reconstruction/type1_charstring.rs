//! One Type 1 charstring (Adobe Type 1 Font Format, chapter 6) run, and its outline written
//! back as a Type 2 charstring (Adobe Technical Note 5177).
//!
//! **Run, not translated operator by operator.** Type 1 places the first point at the
//! left sidebearing `hsbw` states, draws flex through `callothersubr`, and composes an
//! accented glyph with `seac`; none of those has a Type 2 operator to become. So the
//! charstring is interpreted into an outline in absolute coordinates, and the outline is
//! written out with `rmoveto`, `rlineto` and `rrcurveto` alone. Hints are dropped: Type 2
//! allows a stem only before the first drawing operator, and Type 1 replaces them in the
//! middle of a path.

use crate::latin_names::STANDARD_ENCODING;
use std::collections::BTreeMap;

/// How many subroutine bytes one glyph may decrypt and run, summed over every `callsubr`
/// it makes, however deep, and over both components of a `seac`.
///
/// The depth cap bounds how deep calls go and not how many there are, so a subroutine
/// calling itself k times cost k^10: at k = 7, 79 s in a debug build (ROADMAP Z-2). A real
/// glyph's calls are hint replacement and flex, a few hundred bytes in all; 1 MiB is three
/// orders of magnitude above that and runs in milliseconds. A glyph that reaches it stops
/// where it is, as one past the depth cap does.
pub(super) const SUBROUTINE_BUDGET: usize = 1 << 20;

/// How deep `callsubr` may nest. The Type 1 specification sets no limit; ten is the
/// figure this converter has always used, and real fonts nest two or three deep.
const MAX_SUBROUTINE_DEPTH: usize = 10;

/// The Type 1 operand stack holds 24 entries (6.1); a charstring pushing past twice that
/// is not one, and its extra operands are dropped rather than kept.
const MAX_STACK: usize = 48;

/// What a glyph's charstring can reach besides itself: the subroutines it calls, and the
/// other glyphs a `seac` composes it from.
pub(super) struct Type1Program<'a> {
    pub charstrings: &'a BTreeMap<String, Vec<u8>>,
    pub subrs: &'a [Vec<u8>],
    /// `/lenIV`: the random bytes each charstring starts with, or `None` where it is
    /// negative and the charstrings are not encrypted at all.
    pub len_iv: Option<usize>,
}

impl Type1Program<'_> {
    /// A charstring as plain bytes, its encryption (7.2) undone and its `lenIV` bytes
    /// taken off.
    pub(super) fn plain(&self, charstring: &[u8]) -> Vec<u8> {
        match self.len_iv {
            Some(n) => super::FontReconstructor::decrypt_charstring(charstring, n),
            None => charstring.to_vec(),
        }
    }
}

/// A piece of an outline, at absolute coordinates in character space.
#[derive(Clone, Copy)]
enum Segment {
    Move(f64, f64),
    Line(f64, f64),
    Curve([f64; 6]),
}

/// Why running a charstring stopped before its last byte.
#[derive(PartialEq, Eq)]
enum Stop {
    /// `return`: the caller goes on.
    Return,
    /// `endchar`, `seac`, a truncated operand, or the budget spent: the glyph is done.
    End,
}

struct Interpreter<'a> {
    program: &'a Type1Program<'a>,
    stack: Vec<f64>,
    /// What `callothersubr` leaves for `pop` to take back.
    returned: Vec<f64>,
    origin: (f64, f64),
    point: (f64, f64),
    width: Option<f64>,
    /// The points a flex has collected, between `1 callothersubr` and `0 callothersubr`.
    flex: Option<Vec<(f64, f64)>>,
    path: Vec<Segment>,
    budget: usize,
    /// `seac`'s operands: `asb adx ady bchar achar`.
    seac: Option<[f64; 5]>,
}

/// A glyph's charstring as a Type 2 charstring: its outline, and its advance width as the
/// first operand.
pub(super) fn convert_glyph(charstring: &[u8], program: &Type1Program<'_>) -> Vec<u8> {
    let mut it = Interpreter {
        program,
        stack: Vec::new(),
        returned: Vec::new(),
        origin: (0.0, 0.0),
        point: (0.0, 0.0),
        width: None,
        flex: None,
        path: Vec::new(),
        budget: SUBROUTINE_BUDGET,
        seac: None,
    };
    it.run(&program.plain(charstring), 0);

    // A `seac` glyph is drawn as its base and its accent (6.4): the base at the origin, the
    // accent with its origin at `adx - asb, ady`, from which its own `hsbw` steps on by
    // its sidebearing. Both are standard-encoding codes. A component that is itself a
    // `seac` is not composed again.
    if let Some([asb, adx, ady, bchar, achar]) = it.seac.take() {
        it.path.clear();
        for (code, origin) in [(bchar, (0.0, 0.0)), (achar, (adx - asb, ady))] {
            let component =
                standard_encoding_name(code).and_then(|name| program.charstrings.get(name));
            if let Some(component) = component {
                it.origin = origin;
                it.point = origin;
                it.stack.clear();
                it.flex = None;
                it.run(&program.plain(component), 0);
                it.seac = None;
            }
        }
    }
    it.to_type2()
}

fn standard_encoding_name(code: f64) -> Option<&'static str> {
    let code = u8::try_from(code as i64).ok()?;
    STANDARD_ENCODING.iter().find(|(c, _)| *c == code).map(|(_, name)| *name)
}

impl Interpreter<'_> {
    fn run(&mut self, bytes: &[u8], depth: usize) -> Option<Stop> {
        let mut at = 0;
        while let Some(&b) = bytes.get(at) {
            let operand = match b {
                32..=246 => Some((f64::from(b) - 139.0, 1)),
                247..=250 => bytes
                    .get(at + 1)
                    .map(|&w| (f64::from((i32::from(b) - 247) * 256 + i32::from(w) + 108), 2)),
                251..=254 => bytes
                    .get(at + 1)
                    .map(|&w| (f64::from(-(i32::from(b) - 251) * 256 - i32::from(w) - 108), 2)),
                255 => bytes
                    .get(at + 1..at + 5)
                    .map(|v| (f64::from(i32::from_be_bytes([v[0], v[1], v[2], v[3]])), 5)),
                _ => None,
            };
            if b >= 32 {
                let (value, width) = operand?;
                if self.stack.len() < MAX_STACK {
                    self.stack.push(value);
                }
                at += width;
                continue;
            }
            let stop = if b == 12 {
                let b2 = *bytes.get(at + 1)?;
                at += 2;
                self.escape(b2)
            } else {
                at += 1;
                self.operator(b, depth)
            };
            if stop.is_some() {
                return stop;
            }
        }
        None
    }

    fn arg(&self, k: usize) -> f64 {
        self.stack.get(k).copied().unwrap_or(0.0)
    }

    fn operator(&mut self, b: u8, depth: usize) -> Option<Stop> {
        let [a0, a1, a2, a3, a4, a5]: [f64; 6] = std::array::from_fn(|k| self.arg(k));
        match b {
            // rmoveto, hmoveto, vmoveto
            21 => self.move_by(a0, a1),
            22 => self.move_by(a0, 0.0),
            4 => self.move_by(0.0, a0),
            // rlineto, hlineto, vlineto
            5 => self.line_by(a0, a1),
            6 => self.line_by(a0, 0.0),
            7 => self.line_by(0.0, a0),
            // rrcurveto, vhcurveto, hvcurveto
            8 => self.curve_by([a0, a1, a2, a3, a4, a5]),
            30 => self.curve_by([0.0, a0, a1, a2, a3, 0.0]),
            31 => self.curve_by([a0, 0.0, a1, a2, 0.0, a3]),
            // hsbw
            13 => self.side_bearing(a0, 0.0, a1),
            // callsubr
            10 => {
                let index = self.stack.pop().unwrap_or(-1.0);
                if depth >= MAX_SUBROUTINE_DEPTH || index < 0.0 {
                    return None;
                }
                let program = self.program;
                let subr = program.subrs.get(index as usize)?;
                let Some(left) = self.budget.checked_sub(subr.len()) else {
                    return Some(Stop::End);
                };
                self.budget = left;
                return match self.run(&program.plain(subr), depth + 1) {
                    Some(Stop::End) => Some(Stop::End),
                    Some(Stop::Return) | None => None,
                };
            }
            11 => return Some(Stop::Return),
            14 => return Some(Stop::End),
            // hstem, vstem, closepath, and anything this does not know
            _ => {}
        }
        self.stack.clear();
        None
    }

    fn escape(&mut self, b2: u8) -> Option<Stop> {
        let [a0, a1, a2, a3, a4]: [f64; 5] = std::array::from_fn(|k| self.arg(k));
        match b2 {
            // seac
            6 => {
                self.seac = Some([a0, a1, a2, a3, a4]);
                return Some(Stop::End);
            }
            // sbw
            7 => self.side_bearing(a0, a1, a2),
            // div
            12 => {
                let divisor = self.stack.pop().unwrap_or(0.0);
                let dividend = self.stack.pop().unwrap_or(0.0);
                self.stack.push(if divisor == 0.0 { 0.0 } else { dividend / divisor });
                return None;
            }
            // callothersubr: othersubr# and n on top, n arguments under them
            16 => {
                let which = self.stack.pop().unwrap_or(-1.0);
                let n = (self.stack.pop().unwrap_or(0.0).max(0.0) as usize).min(self.stack.len());
                let args = self.stack.split_off(self.stack.len() - n);
                self.other_subroutine(which, &args);
                self.returned = args.into_iter().rev().collect();
                return None;
            }
            // pop
            17 => {
                let value = self.returned.pop().unwrap_or(0.0);
                if self.stack.len() < MAX_STACK {
                    self.stack.push(value);
                }
                return None;
            }
            // dotsection, vstem3, hstem3, setcurrentpoint (flex has set the point already)
            _ => {}
        }
        self.stack.clear();
        None
    }

    /// The three OtherSubrs a flex is made of (8.3). 3, hint replacement, has nothing to
    /// draw: its argument comes back through `pop` for the `callsubr` after it.
    fn other_subroutine(&mut self, which: f64, _args: &[f64]) {
        if which == 1.0 {
            self.flex = Some(Vec::new());
        } else if which == 2.0 {
            let point = self.point;
            if let Some(points) = self.flex.as_mut() {
                points.push(point);
            }
        } else if which == 0.0
            && let Some(points) = self.flex.take()
        {
            // The first point is the reference point, and the six after it are two
            // curves' control and end points.
            if let [_, p1, p2, p3, p4, p5, p6, ..] = points[..] {
                self.path.push(Segment::Curve([p1.0, p1.1, p2.0, p2.1, p3.0, p3.1]));
                self.path.push(Segment::Curve([p4.0, p4.1, p5.0, p5.1, p6.0, p6.1]));
                self.point = p6;
            }
        }
    }

    fn side_bearing(&mut self, sbx: f64, sby: f64, wx: f64) {
        self.width.get_or_insert(wx);
        self.point = (self.origin.0 + sbx, self.origin.1 + sby);
    }

    fn move_by(&mut self, dx: f64, dy: f64) {
        self.point = (self.point.0 + dx, self.point.1 + dy);
        // In a flex, `rmoveto` places the next point and draws nothing.
        if self.flex.is_none() {
            self.path.push(Segment::Move(self.point.0, self.point.1));
        }
    }

    fn line_by(&mut self, dx: f64, dy: f64) {
        self.point = (self.point.0 + dx, self.point.1 + dy);
        self.path.push(Segment::Line(self.point.0, self.point.1));
    }

    fn curve_by(&mut self, d: [f64; 6]) {
        let (x0, y0) = self.point;
        let (x1, y1) = (x0 + d[0], y0 + d[1]);
        let (x2, y2) = (x1 + d[2], y1 + d[3]);
        let (x3, y3) = (x2 + d[4], y2 + d[5]);
        self.path.push(Segment::Curve([x1, y1, x2, y2, x3, y3]));
        self.point = (x3, y3);
    }

    /// The outline as Type 2: the width first, before the first operator that clears
    /// the stack (TN 5177, 3.1, with `nominalWidthX` 0), then every segment relative to
    /// the one before it, then `endchar`.
    fn to_type2(&self) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(width) = self.width {
            put(&mut out, width);
        }
        let mut at = (0.0, 0.0);
        let mut started = false;
        for &segment in &self.path {
            if !started && !matches!(segment, Segment::Move(..)) {
                out.extend([139, 139, 21]);
            }
            started = true;
            match segment {
                Segment::Move(x, y) => {
                    put(&mut out, x - at.0);
                    put(&mut out, y - at.1);
                    out.push(21);
                    at = (x, y);
                }
                Segment::Line(x, y) => {
                    put(&mut out, x - at.0);
                    put(&mut out, y - at.1);
                    out.push(5);
                    at = (x, y);
                }
                Segment::Curve([x1, y1, x2, y2, x3, y3]) => {
                    for (v, from) in
                        [(x1, at.0), (y1, at.1), (x2, x1), (y2, y1), (x3, x2), (y3, y2)]
                    {
                        put(&mut out, v - from);
                    }
                    out.push(8);
                    at = (x3, y3);
                }
            }
        }
        out.push(14);
        out
    }
}

/// One Type 2 operand: an integer in the smallest form that holds it, anything else as
/// 16.16 fixed point (255), which is the only fractional form Type 2 has.
fn put(out: &mut Vec<u8>, value: f64) {
    if value.fract() == 0.0 && value.abs() <= f64::from(i16::MAX) {
        super::FontReconstructor::push_t2_number(out, value as i32);
    } else {
        let fixed = (value * 65536.0).round().clamp(f64::from(i32::MIN), f64::from(i32::MAX));
        out.push(255);
        out.extend_from_slice(&(fixed as i32).to_be_bytes());
    }
}
