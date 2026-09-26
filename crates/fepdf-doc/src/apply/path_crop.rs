//! The paths a crop puts outside the sheet, cut to the part that remains (ADR-0088).
//!
//! **Hidden is not removed.** A crop that only clipped would leave every line and fill of
//! a drawing in the file, the half on the other sheet included; this rebuilds each painted
//! path from what lies on the kept side.
//!
//! **What is cut, and what is left whole.** A path made of straight lines is cut exactly:
//! a filled one as a polygon clipped to the kept rectangle, a stroked one segment by
//! segment against the rectangle grown by half the pen, so a line along the edge keeps the
//! half of it that shows. A path wholly outside is taken out. Left whole, because cutting
//! would change what shows:
//!
//! - a path with a curve in it — a curve cut at the edge is a different curve, and
//!   flattening it into lines would draw a different shape;
//! - a path both filled and stroked, where the cut would stroke the new edge along the
//!   sheet's border;
//! - a path used to clip (`W`), which decides what everything after it shows — taking one
//!   out that lies outside would uncover what it was hiding.
//!
//! **Reached: the page's own content.** A path inside a form XObject is not reached.

use fepdf_model::lexer::{Lexer, Token};
use fepdf_model::{Document, PdfResult};
use kurbo::{Affine, Point, Rect};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One piece of a path, in the space it was written in.
#[derive(Clone, Copy)]
enum Piece {
    Move(Point),
    Line(Point),
    Curve,
    Close,
}

/// A path under construction: where its operators start, and what it holds.
#[derive(Default)]
struct Building {
    start: Option<usize>,
    pieces: Vec<Piece>,
    /// Every point it names, control points included, on the page.
    extent: Option<Rect>,
    clips: bool,
    current: Point,
}

/// The graphics state this follows: where things are, and how wide the pen is.
#[derive(Clone, Copy)]
struct Pen {
    ctm: Affine,
    width: f64,
}

/// Cuts each path painted on `page` to what `keep` leaves of it.
///
/// # Errors
/// Fails when the page is not there or its content cannot be read.
pub fn cut_paths_outside(doc: &Document, page: usize, keep: (f64, f64, f64, f64)) -> PdfResult<()> {
    let Some(data) = crate::apply::text::page_content(doc, page)? else { return Ok(()) };
    let tokens = tokens_of(&data);
    let keep = Rect::new(keep.0, keep.1, keep.2, keep.3);
    let mut replaced: BTreeMap<usize, (usize, Vec<u8>)> = BTreeMap::new();
    let (mut pen, mut saved) = (Pen { ctm: Affine::IDENTITY, width: 1.0 }, Vec::new());
    let mut path = Building::default();
    let mut operands_from = 0;
    for (index, token) in tokens.iter().enumerate() {
        let Token::Keyword(op) = token else { continue };
        let numbers = numbers_of(&tokens[operands_from..index]);
        let first = operands_from;
        operands_from = index + 1;
        match op.as_str() {
            "q" => saved.push(pen),
            "Q" => pen = saved.pop().unwrap_or(pen),
            "cm" if numbers.len() >= 6 => {
                pen.ctm *= Affine::new([
                    numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5],
                ]);
            }
            "w" => pen.width = numbers.first().copied().unwrap_or(pen.width),
            "W" | "W*" => path.clips = true,
            "m" | "l" | "c" | "v" | "y" | "re" | "h" => {
                path.start.get_or_insert(first);
                path.add(op, &numbers, pen.ctm);
            }
            "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                if let Some(start) = path.start
                    && let Some(rebuilt) = rebuild(&path, op, pen, keep)
                {
                    replaced.insert(start, (index, rebuilt.into_bytes()));
                }
                path = Building::default();
            }
            _ => {}
        }
    }
    if replaced.is_empty() {
        return Ok(());
    }
    crate::apply::text::write_page_content(doc, page, rewritten(&tokens, &replaced))
}

/// The tokens written out again, each run from a key of `replaced` to the index it holds
/// written as the bytes beside it instead.
pub fn rewritten(tokens: &[Token], replaced: &BTreeMap<usize, (usize, Vec<u8>)>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut index = 0;
    while let Some(token) = tokens.get(index) {
        if let Some((end, bytes)) = replaced.get(&index) {
            out.extend_from_slice(bytes);
            index = end + 1;
            continue;
        }
        token.write_to(&mut out);
        index += 1;
    }
    out
}

impl Building {
    /// Adds what `op` says, its points taken onto the page through `ctm` for the extent.
    fn add(&mut self, op: &str, numbers: &[f64], ctm: Affine) {
        let at = |i: usize| {
            Point::new(
                numbers.get(i).copied().unwrap_or(0.0),
                numbers.get(i + 1).copied().unwrap_or(0.0),
            )
        };
        match op {
            "m" | "l" => {
                self.current = at(0);
                self.pieces.push(if op == "m" { Piece::Move(at(0)) } else { Piece::Line(at(0)) });
                self.touch(at(0), ctm);
            }
            "c" | "v" | "y" => {
                let count = if op == "c" { 3 } else { 2 };
                self.touch(self.current, ctm);
                for nth in 0..count {
                    self.touch(at(nth * 2), ctm);
                }
                self.current = at((count - 1) * 2);
                self.pieces.push(Piece::Curve);
            }
            "re" => self.rectangle(at(0), at(2), ctm),
            _ => self.pieces.push(Piece::Close),
        }
    }

    /// `re`: a closed subpath round the rectangle at `corner`, `size` across (8.5.2.1).
    fn rectangle(&mut self, corner: Point, size: Point, ctm: Affine) {
        let (x, y, w, h) = (corner.x, corner.y, size.x, size.y);
        let corners = [
            Point::new(x, y),
            Point::new(x + w, y),
            Point::new(x + w, y + h),
            Point::new(x, y + h),
        ];
        self.pieces.push(Piece::Move(corners[0]));
        for corner in &corners[1..] {
            self.pieces.push(Piece::Line(*corner));
        }
        self.pieces.push(Piece::Close);
        for corner in corners {
            self.touch(corner, ctm);
        }
        self.current = corners[0];
    }

    /// Takes `point` into the path's extent, on the page.
    fn touch(&mut self, point: Point, ctm: Affine) {
        let on_page = ctm * point;
        self.extent = Some(
            self.extent
                .map_or_else(|| Rect::from_points(on_page, on_page), |r| r.union_pt(on_page)),
        );
    }

    /// The path's subpaths as lists of points, each with whether it was closed — or
    /// `None` when there is a curve in it.
    fn polygons(&self) -> Option<Vec<(Vec<Point>, bool)>> {
        let mut out: Vec<(Vec<Point>, bool)> = Vec::new();
        for piece in &self.pieces {
            match piece {
                Piece::Move(point) => out.push((vec![*point], false)),
                Piece::Line(point) => match out.last_mut() {
                    Some((points, _)) => points.push(*point),
                    None => out.push((vec![*point], false)),
                },
                Piece::Close => {
                    if let Some((_, closed)) = out.last_mut() {
                        *closed = true;
                    }
                }
                Piece::Curve => return None,
            }
        }
        Some(out)
    }
}

/// What replaces a painted path, or `None` to leave it as it is.
fn rebuild(path: &Building, op: &str, pen: Pen, keep: Rect) -> Option<String> {
    let extent = path.extent?;
    let strokes = matches!(op, "S" | "s" | "B" | "B*" | "b" | "b*");
    let fills = matches!(op, "f" | "F" | "f*" | "B" | "B*" | "b" | "b*");
    // How far the pen reaches past the points, on the page — generously, so a mitred
    // corner is not taken for outside.
    let reach = if strokes { pen.width.max(1.0) * pen.ctm.determinant().abs().sqrt() } else { 0.0 };
    let reached = extent.inflate(reach, reach);
    if path.clips || op == "n" {
        return None;
    }
    if reached.intersect(keep).is_zero_area() && !overlaps(reached, keep) {
        return Some(String::new());
    }
    if keep.contains_rect(reached) || (strokes && fills) {
        return None;
    }
    let polygons = path.polygons()?;
    let inverse = pen.ctm.inverse();
    let on_page = |points: &[Point]| points.iter().map(|p| pen.ctm * *p).collect::<Vec<_>>();
    let mut out = String::new();
    if fills {
        for (points, _) in &polygons {
            let cut = clip_polygon(&on_page(points), keep);
            if cut.len() >= 3 {
                write_polyline(&mut out, &cut, inverse, true);
            }
        }
        let _ = writeln!(out, "{op}");
    } else {
        let grown = keep.inflate(reach / 2.0, reach / 2.0);
        for (points, closed) in &polygons {
            let mut points = on_page(points);
            if (*closed || op == "s")
                && let Some(first) = points.first().copied()
            {
                points.push(first);
            }
            for pair in points.windows(2) {
                if let Some(kept) = clip_segment(pair[0], pair[1], grown) {
                    write_polyline(&mut out, &kept, inverse, false);
                }
            }
        }
        out.push_str("S\n");
    }
    Some(out)
}

/// Whether two rectangles share any point, edges included.
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

/// `points`, on the page, written back into the space the path was written in.
fn write_polyline(out: &mut String, points: &[Point], inverse: Affine, closed: bool) {
    for (nth, point) in points.iter().enumerate() {
        let written = inverse * *point;
        let _ =
            writeln!(out, "{:.4} {:.4} {}", written.x, written.y, if nth == 0 { "m" } else { "l" });
    }
    if closed {
        out.push_str("h\n");
    }
}

/// A polygon cut to a rectangle, one edge of the rectangle at a time (Sutherland–Hodgman).
fn clip_polygon(points: &[Point], keep: Rect) -> Vec<Point> {
    let edges: [(fn(Point, Rect) -> bool, fn(Point, Point, Rect) -> Point); 4] = [
        (|p, r| p.x >= r.x0, |a, b, r| crossing_x(a, b, r.x0)),
        (|p, r| p.x <= r.x1, |a, b, r| crossing_x(a, b, r.x1)),
        (|p, r| p.y >= r.y0, |a, b, r| crossing_y(a, b, r.y0)),
        (|p, r| p.y <= r.y1, |a, b, r| crossing_y(a, b, r.y1)),
    ];
    let mut current = points.to_vec();
    for (inside, cross) in edges {
        let Some(&last) = current.last() else { break };
        let mut next = Vec::with_capacity(current.len() + 4);
        let mut previous = last;
        for &point in &current {
            match (inside(point, keep), inside(previous, keep)) {
                (true, true) => next.push(point),
                (true, false) => {
                    next.push(cross(previous, point, keep));
                    next.push(point);
                }
                (false, true) => next.push(cross(previous, point, keep)),
                (false, false) => {}
            }
            previous = point;
        }
        current = next;
    }
    current
}

fn crossing_x(a: Point, b: Point, x: f64) -> Point {
    let t = (x - a.x) / (b.x - a.x);
    Point::new(x, (b.y - a.y).mul_add(t, a.y))
}

fn crossing_y(a: Point, b: Point, y: f64) -> Point {
    let t = (y - a.y) / (b.y - a.y);
    Point::new((b.x - a.x).mul_add(t, a.x), y)
}

/// The part of the segment from `from` to `to` inside `keep`, if any (Liang–Barsky).
fn clip_segment(from: Point, to: Point, keep: Rect) -> Option<[Point; 2]> {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let (mut enter, mut leave) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, from.x - keep.x0),
        (dx, keep.x1 - from.x),
        (-dy, from.y - keep.y0),
        (dy, keep.y1 - from.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            enter = enter.max(t);
        } else {
            leave = leave.min(t);
        }
    }
    if enter > leave {
        return None;
    }
    let at = |t: f64| Point::new(dx.mul_add(t, from.x), dy.mul_add(t, from.y));
    Some([at(enter), at(leave)])
}

/// The tokens of a content stream, in order.
fn tokens_of(data: &[u8]) -> Vec<Token> {
    let mut lexer = Lexer::new(bytes::Bytes::copy_from_slice(data));
    let mut tokens = Vec::new();
    while let Ok(token) = lexer.next_token() {
        if token == Token::EOF {
            break;
        }
        tokens.push(token);
    }
    tokens
}

/// The numeric operands among `operands`, in order.
fn numbers_of(operands: &[Token]) -> Vec<f64> {
    operands
        .iter()
        .filter_map(|token| match token {
            Token::Integer(n) => i32::try_from(*n).ok().map(f64::from),
            Token::Real(n) => Some(*n),
            _ => None,
        })
        .collect()
}
