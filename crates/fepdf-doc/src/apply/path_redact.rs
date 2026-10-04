//! The paths a redaction region lies over, cut to what lies outside it (ROADMAP Y-10,
//! decided by the owner 2026-10-03).
//!
//! **Cut, not removed whole and not left.** MuPDF removes a path the region touches, which
//! takes a page's background or a table's rules with it; leaving it leaves a signature
//! drawn in vectors. Here what a region covers goes and the rest is drawn as it was.
//!
//! **Curves are cut where they cross**, split at the parameter the crossing falls at, so
//! the part outside is the same curve. A crop leaves a curved path whole for that reason
//! ([`super::path_crop`]); a redaction cannot, since what it leaves is inside.
//!
//! - **A filled path, or one used to clip, loses the region**: it is clipped to each of
//!   the four strips round it, which do not overlap, so the fill rule reads the pieces as
//!   it read the whole.
//! - **A stroked one loses every stretch the pen would draw inside**: the region grown by
//!   half the pen. A mitred corner whose point could reach the region is broken there,
//!   so it is drawn as two ends and no point. A dashed stroke's pieces each start their
//!   dash where the original was at that point.
//!
//! **Reached: the page's own content**, after its inline images are lifted. A path a form
//! XObject draws is a later part of Y-10.

use super::target::Target;
use super::text::GlyphBox;
use fepdf_model::lexer::Token;
use fepdf_model::{Document, PdfResult};
use kurbo::{
    Affine, BezPath, Line, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point, Rect, Shape,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Far enough to cross any page: the length of the lines regions are cut along.
const FAR: f64 = 1.0e7;

/// The graphics state a path is painted in, as far as cutting it needs.
#[derive(Clone)]
struct Pen {
    ctm: Affine,
    width: f64,
    miter: f64,
    /// Whether `j` says corners are mitred, which is 0 (Table 55) and the default.
    mitred: bool,
    /// `d`: the dash array as written, and its phase; `None` for a solid line.
    dash: Option<(String, f64)>,
}

impl Default for Pen {
    fn default() -> Self {
        Self { ctm: Affine::IDENTITY, width: 1.0, miter: 10.0, mitred: true, dash: None }
    }
}

/// A path under construction: the index of its first operand, and its points in the user
/// space it is written in.
#[derive(Default)]
struct Building {
    start: Option<usize>,
    path: BezPath,
    clip: Option<&'static str>,
}

/// Cuts every path the page's content paints to what lies outside `regions`.
///
/// # Errors
/// Fails when the page is not there or its content cannot be read.
pub fn cut_paths(doc: &Document, target: Target, regions: &[GlyphBox]) -> PdfResult<()> {
    let Some(data) = target.content(doc)? else { return Ok(()) };
    let tokens = super::image_crop::tokens_of(&data);
    let regions: Vec<Rect> = regions.iter().map(|r| Rect::new(r.0, r.1, r.2, r.3)).collect();
    let mut replaced: BTreeMap<usize, (usize, Vec<u8>)> = BTreeMap::new();
    walk(&tokens, &mut |start, end, building, op, pen| {
        if let Some(rebuilt) = rebuild(&building.path, op, building.clip, pen, &regions) {
            replaced.insert(start, (end, rebuilt.into_bytes()));
        }
    });
    if replaced.is_empty() {
        return Ok(());
    }
    let out = super::path_crop::rewritten(&tokens, &replaced);
    target.write(doc, out)
}

/// Where on the page a path the regions meet is cut: each region cut to the path's box,
/// grown by the pen for a stroke.
///
/// # Errors
/// Fails when the page is not there or its content cannot be read.
pub fn cut_areas(doc: &Document, target: Target, regions: &[GlyphBox]) -> PdfResult<Vec<GlyphBox>> {
    let Some(data) = target.content(doc)? else { return Ok(Vec::new()) };
    let tokens = super::image_crop::tokens_of(&data);
    let mut areas = Vec::new();
    walk(&tokens, &mut |_, _, building, op, pen| {
        if op == "n" && building.clip.is_none() {
            return;
        }
        let reach = if strokes(op) { corner_reach(pen) } else { 0.0 };
        let extent = (pen.ctm * building.path.clone()).bounding_box().inflate(reach, reach);
        for region in regions {
            let cut = extent.intersect(Rect::new(region.0, region.1, region.2, region.3));
            if cut.width() > 0.0 && cut.height() > 0.0 {
                areas.push((cut.x0, cut.y0, cut.x1, cut.y1));
            }
        }
    });
    Ok(areas)
}

/// Follows the graphics state through `tokens`, and hands each painted path to `painted`
/// with where it starts and ends among them.
fn walk(tokens: &[Token], painted: &mut dyn FnMut(usize, usize, &Building, &str, &Pen)) {
    let (mut pen, mut saved, mut building) = (Pen::default(), Vec::new(), Building::default());
    let mut operands_from = 0;
    for (index, token) in tokens.iter().enumerate() {
        let Token::Keyword(op) = token else { continue };
        let operands = &tokens[operands_from..index];
        let first = operands_from;
        operands_from = index + 1;
        let numbers = numbers_of(operands);
        match op.as_str() {
            "q" => saved.push(pen.clone()),
            "Q" => pen = saved.pop().unwrap_or_default(),
            "cm" if numbers.len() >= 6 => pen.ctm *= six(&numbers),
            "w" => pen.width = numbers.first().copied().unwrap_or(pen.width),
            "M" => pen.miter = numbers.first().copied().unwrap_or(pen.miter),
            "j" => pen.mitred = numbers.first().map_or(pen.mitred, |n| n.abs() < 0.5),
            "d" => pen.dash = dash_of(operands),
            "W" => building.clip = Some("W"),
            "W*" => building.clip = Some("W*"),
            "m" | "l" | "c" | "v" | "y" | "re" | "h" => {
                building.start.get_or_insert(first);
                add(&mut building.path, op, &numbers);
            }
            "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                if let Some(start) = building.start {
                    painted(start, index, &building, op, &pen);
                }
                building = Building::default();
            }
            _ => {}
        }
    }
}

/// The operands' numbers, in order.
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

/// The matrix six numbers write.
fn six(n: &[f64]) -> Affine {
    Affine::new([n[0], n[1], n[2], n[3], n[4], n[5]])
}

/// A `d`'s array as written, and its phase; `None` for an empty array, which is solid.
fn dash_of(operands: &[Token]) -> Option<(String, f64)> {
    let mut array = Vec::new();
    Token::LeftArray.write_to(&mut array);
    let mut lengths = 0;
    for token in operands.iter().take_while(|t| **t != Token::RightArray).skip(1) {
        token.write_to(&mut array);
        lengths += 1;
    }
    Token::RightArray.write_to(&mut array);
    let phase = numbers_of(operands.last().map(std::slice::from_ref).unwrap_or_default());
    (lengths > 0).then(|| {
        (String::from_utf8_lossy(&array).to_string(), phase.first().copied().unwrap_or(0.0))
    })
}

/// Adds what a path-construction operator says to `path` (8.5.2).
fn add(path: &mut BezPath, op: &str, n: &[f64]) {
    let at = |i: usize| {
        Point::new(n.get(i).copied().unwrap_or(0.0), n.get(i + 1).copied().unwrap_or(0.0))
    };
    let current = path.elements().last().and_then(PathEl::end_point).unwrap_or_default();
    match op {
        "m" => path.move_to(at(0)),
        "l" => path.line_to(at(0)),
        "c" => path.curve_to(at(0), at(2), at(4)),
        "v" => path.curve_to(current, at(0), at(2)),
        "y" => path.curve_to(at(0), at(2), at(2)),
        "re" => {
            let (corner, size) = (at(0), at(2));
            path.move_to(corner);
            path.line_to((corner.x + size.x, corner.y));
            path.line_to((corner.x + size.x, corner.y + size.y));
            path.line_to((corner.x, corner.y + size.y));
            path.close_path();
        }
        _ => path.close_path(),
    }
}

fn strokes(op: &str) -> bool {
    matches!(op, "S" | "s" | "B" | "B*" | "b" | "b*")
}

/// How far past its points a stroke's ink reaches, on the page, away from its corners:
/// half the pen, scaled by the most the transform stretches a length — its largest
/// singular value.
fn reach_of(pen: &Pen) -> f64 {
    let [a, b, c, d, _, _] = pen.ctm.as_coeffs();
    let (across, down) = (a.mul_add(a, b * b), c.mul_add(c, d * d));
    let spread = ((across - down) / 2.0).hypot(a.mul_add(c, b * d));
    pen.width.max(0.0) / 2.0 * (f64::midpoint(across, down) + spread).sqrt()
}

/// How far a mitred corner's point can reach past the corner, on the page (8.4.3.5).
fn corner_reach(pen: &Pen) -> f64 {
    if pen.mitred { reach_of(pen) * pen.miter.max(1.0) } else { reach_of(pen) }
}

/// How far `point` is from `rect`, 0 inside it.
fn distance(point: Point, rect: Rect) -> f64 {
    let dx = (rect.x0 - point.x).max(point.x - rect.x1).max(0.0);
    let dy = (rect.y0 - point.y).max(point.y - rect.y1).max(0.0);
    dx.hypot(dy)
}

/// What replaces a painted path, or `None` to leave it as it is.
fn rebuild(
    path: &BezPath,
    op: &str,
    clip: Option<&str>,
    pen: &Pen,
    regions: &[Rect],
) -> Option<String> {
    let fills = matches!(op, "f" | "F" | "f*" | "B" | "B*" | "b" | "b*");
    let reach = if strokes(op) { reach_of(pen) } else { 0.0 };
    // A mitred corner reaches further than the pen, and is what is broken when it does.
    let further = if strokes(op) { corner_reach(pen) } else { 0.0 };
    let on_page = pen.ctm * path.clone();
    let extent = on_page.bounding_box().inflate(further, further);
    let painted = fills || strokes(op) || clip.is_some();
    if !painted || !regions.iter().any(|r| r.overlaps(extent)) {
        return None;
    }
    let inverse = pen.ctm.inverse();
    let area = || regions.iter().fold(subpaths(&on_page, true), |kept, r| cut_area(&kept, *r));
    let mut out = String::new();
    if fills {
        write_closed(&mut out, &area(), inverse);
        out.push_str(if op.ends_with('*') { "f*\n" } else { "f\n" });
    }
    if strokes(op) {
        let closing = matches!(op, "s" | "b" | "b*");
        let grown: Vec<Rect> = regions.iter().map(|r| r.inflate(reach, reach)).collect();
        let near = |p: Point| regions.iter().any(|r| distance(p, *r) < corner_reach(pen));
        write_stroke(&mut out, &on_page, closing, (&grown, &near), pen, inverse);
    }
    if let Some(rule) = clip {
        write_closed(&mut out, &area(), inverse);
        let _ = writeln!(out, "{rule} n");
    }
    Some(out)
}

/// The subpaths of `path` as segments; each closed by a line back to its start where
/// `close` says or the path does.
fn subpaths(path: &BezPath, close: bool) -> Vec<(Vec<PathSeg>, bool)> {
    let mut out: Vec<(Vec<PathSeg>, bool)> = Vec::new();
    let (mut start, mut at) = (Point::ZERO, Point::ZERO);
    for element in path.elements() {
        let seg = match *element {
            PathEl::MoveTo(p) => {
                finish(&mut out, start, at, close);
                out.push((Vec::new(), false));
                (start, at) = (p, p);
                continue;
            }
            PathEl::LineTo(p) => PathSeg::Line(Line::new(at, p)),
            PathEl::QuadTo(c, p) => PathSeg::Quad(kurbo::QuadBez::new(at, c, p)),
            PathEl::CurveTo(c1, c2, p) => PathSeg::Cubic(kurbo::CubicBez::new(at, c1, c2, p)),
            PathEl::ClosePath => {
                finish(&mut out, start, at, true);
                at = start;
                continue;
            }
        };
        at = seg.end();
        if let Some((segs, _)) = out.last_mut() {
            segs.push(seg);
        }
    }
    finish(&mut out, start, at, close);
    out.retain(|(segs, _)| !segs.is_empty());
    out
}

/// Closes the last subpath with a line back to `start` if `close` says, once.
fn finish(out: &mut [(Vec<PathSeg>, bool)], start: Point, at: Point, close: bool) {
    let Some((segs, closed)) = out.last_mut() else { return };
    if !close || *closed || segs.is_empty() {
        return;
    }
    if (at - start).hypot() > 1e-9 {
        segs.push(PathSeg::Line(Line::new(at, start)));
    }
    *closed = true;
}

/// Closed subpaths with `region` taken out: each clipped to the four strips round it.
fn cut_area(kept: &[(Vec<PathSeg>, bool)], region: Rect) -> Vec<(Vec<PathSeg>, bool)> {
    let strips: [&[(bool, f64, bool)]; 4] = [
        &[(true, region.x0, true)],
        &[(true, region.x1, false)],
        &[(true, region.x0, false), (true, region.x1, true), (false, region.y0, true)],
        &[(true, region.x0, false), (true, region.x1, true), (false, region.y1, false)],
    ];
    let mut out = Vec::new();
    for (segs, _) in kept {
        let extent = segs.iter().fold(Rect::ZERO, |r, s| {
            if r == Rect::ZERO { s.bounding_box() } else { r.union(s.bounding_box()) }
        });
        if !extent.overlaps(region) {
            out.push((segs.clone(), true));
            continue;
        }
        for strip in strips {
            let clipped =
                strip.iter().fold(segs.clone(), |s, (x, at, less)| clip_half(&s, *x, *at, *less));
            if !clipped.is_empty() {
                out.push((clipped, true));
            }
        }
    }
    out
}

/// A closed subpath cut to the side of a line it keeps: `x = at` across when `across`,
/// else `y = at`, keeping what is less when `less`. The pieces on that side are joined
/// along the line, as Sutherland and Hodgman join them, with each curve split where it
/// crosses.
fn clip_half(segs: &[PathSeg], across: bool, at: f64, less: bool) -> Vec<PathSeg> {
    let line =
        if across { Line::new((at, -FAR), (at, FAR)) } else { Line::new((-FAR, at), (FAR, at)) };
    let inside = |p: Point| {
        let v = if across { p.x } else { p.y };
        if less { v <= at } else { v >= at }
    };
    let pieces: Vec<PathSeg> =
        segs.iter().flat_map(|s| split_at(s, line)).filter(|p| inside(p.eval(0.5))).collect();
    join_closed(&pieces)
}

/// `seg` split where it crosses `line`.
fn split_at(seg: &PathSeg, line: Line) -> Vec<PathSeg> {
    let mut ts: Vec<f64> = seg
        .intersect_line(line)
        .iter()
        .map(|i| i.segment_t)
        .filter(|t| *t > 1e-9 && *t < 1.0 - 1e-9)
        .collect();
    ts.sort_by(f64::total_cmp);
    let bounds: Vec<f64> = std::iter::once(0.0).chain(ts).chain(std::iter::once(1.0)).collect();
    bounds.windows(2).filter(|w| w[1] - w[0] > 1e-12).map(|w| seg.subsegment(w[0]..w[1])).collect()
}

/// `pieces` in order as one closed outline, a straight line wherever one ends short of
/// where the next begins, and back to the first.
fn join_closed(pieces: &[PathSeg]) -> Vec<PathSeg> {
    let mut out: Vec<PathSeg> = Vec::new();
    for piece in pieces {
        if let Some(last) = out.last()
            && (last.end() - piece.start()).hypot() > 1e-9
        {
            out.push(PathSeg::Line(Line::new(last.end(), piece.start())));
        }
        out.push(*piece);
    }
    if let (Some(first), Some(last)) = (out.first(), out.last())
        && (last.end() - first.start()).hypot() > 1e-9
    {
        out.push(PathSeg::Line(Line::new(last.end(), first.start())));
    }
    out
}

/// Writes closed subpaths, taken back through `inverse` into the space they are drawn in.
fn write_closed(out: &mut String, subpaths: &[(Vec<PathSeg>, bool)], inverse: Affine) {
    for (segs, _) in subpaths {
        write_run(out, segs, inverse);
        out.push_str("h\n");
    }
}

/// Writes one run of segments as a subpath, through `inverse`.
fn write_run(out: &mut String, segs: &[PathSeg], inverse: Affine) {
    let Some(first) = segs.first() else { return };
    let p = inverse * first.start();
    let _ = writeln!(out, "{:.6} {:.6} m", p.x, p.y);
    for seg in segs {
        let cubic = match *seg {
            PathSeg::Line(l) => {
                let p = inverse * l.p1;
                let _ = writeln!(out, "{:.6} {:.6} l", p.x, p.y);
                continue;
            }
            PathSeg::Quad(q) => q.raise(),
            PathSeg::Cubic(c) => c,
        };
        let [c1, c2, end] = [cubic.p1, cubic.p2, cubic.p3].map(|p| inverse * p);
        let _ = writeln!(
            out,
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c",
            c1.x, c1.y, c2.x, c2.y, end.x, end.y
        );
    }
}

/// Writes what a stroke keeps outside `grown`: each subpath as it was where nothing of it
/// is inside and no corner is `near` enough for its point to reach in, else the runs
/// between the stretches that are and the corners that are, each dashed from where the
/// original was at its start.
fn write_stroke(
    out: &mut String,
    on_page: &BezPath,
    closing: bool,
    (grown, near): (&[Rect], &dyn Fn(Point) -> bool),
    pen: &Pen,
    inverse: Affine,
) {
    let mut solid = String::new();
    for (segs, closed) in subpaths(on_page, closing) {
        let pieces = stroke_pieces(&segs, closed, grown, near);
        if pieces.iter().all(|(_, kept, breaks)| *kept && !*breaks) {
            write_run(&mut solid, &segs, inverse);
            if closed {
                solid.push_str("h\n");
            }
            continue;
        }
        let (mut along, mut run_from, mut run) = (0.0, 0.0, Vec::new());
        for (piece, kept, breaks) in pieces {
            if kept {
                if run.is_empty() {
                    run_from = along;
                }
                run.push(piece);
            }
            if !kept || breaks {
                dashed_run(out, &mut solid, &run, run_from, pen, inverse);
                run.clear();
            }
            along += user_length(&piece, inverse);
        }
        dashed_run(out, &mut solid, &run, run_from, pen, inverse);
    }
    if !solid.is_empty() {
        out.push_str(&solid);
        out.push_str("S\n");
    }
}

/// A subpath's segments split at the edges of `grown`, each piece with whether it is
/// kept — outside every region — and whether a corner it ends at is `near` enough to break
/// the stroke there.
fn stroke_pieces(
    segs: &[PathSeg],
    closed: bool,
    grown: &[Rect],
    near: &dyn Fn(Point) -> bool,
) -> Vec<(PathSeg, bool, bool)> {
    let mut out = Vec::new();
    for (nth, seg) in segs.iter().enumerate() {
        let parts = grown
            .iter()
            .fold(vec![*seg], |parts, r| parts.iter().flat_map(|p| split_by_rect(p, *r)).collect());
        let corner = nth + 1 < segs.len() || closed;
        let count = parts.len();
        for (index, part) in parts.into_iter().enumerate() {
            let mid = part.eval(0.5);
            let kept = !grown.iter().any(|r| r.contains(mid));
            let breaks = corner && index + 1 == count && near(part.end());
            out.push((part, kept, breaks));
        }
    }
    out
}

/// Writes a run that kept part of a subpath: into `solid`, to be stroked with the rest,
/// or with its own dash phase, `from` user-space units along, stroked on its own.
fn dashed_run(
    out: &mut String,
    solid: &mut String,
    run: &[PathSeg],
    from: f64,
    pen: &Pen,
    inverse: Affine,
) {
    if run.is_empty() {
        return;
    }
    match &pen.dash {
        None => write_run(solid, run, inverse),
        Some((array, phase)) => {
            let _ = writeln!(out, "q {array} {:.6} d", phase + from);
            write_run(out, run, inverse);
            out.push_str("S Q\n");
        }
    }
}

/// `seg` split where it crosses each edge of `rect`.
fn split_by_rect(seg: &PathSeg, rect: Rect) -> Vec<PathSeg> {
    let edges = [
        Line::new((rect.x0, -FAR), (rect.x0, FAR)),
        Line::new((rect.x1, -FAR), (rect.x1, FAR)),
        Line::new((-FAR, rect.y0), (FAR, rect.y0)),
        Line::new((-FAR, rect.y1), (FAR, rect.y1)),
    ];
    edges
        .iter()
        .fold(vec![*seg], |parts, edge| parts.iter().flat_map(|p| split_at(p, *edge)).collect())
}

/// How long `seg` is in the space it is drawn in, which a dash is measured in.
fn user_length(seg: &PathSeg, inverse: Affine) -> f64 {
    let back = match *seg {
        PathSeg::Line(l) => PathSeg::Line(Line::new(inverse * l.p0, inverse * l.p1)),
        PathSeg::Quad(q) => PathSeg::Quad(inverse * q),
        PathSeg::Cubic(c) => PathSeg::Cubic(inverse * c),
    };
    back.arclen(1e-6)
}
