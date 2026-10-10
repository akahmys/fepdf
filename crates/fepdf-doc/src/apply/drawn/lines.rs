//! Lines and shapes: Line (Table 178), Square and Circle (Table 180), Polygon and PolyLine
//! (Table 181), Ink (Table 185), and the line endings of Table 179.

use super::{Area, Drawing, Entries, local};
use std::fmt::Write as _;

/// A point in the annotation's own space.
type Point = (f64, f64);

/// A line annotation: `/L`, its leader lines (`/LL`, `/LLE`, `/LLO`) and its endings.
///
/// The leader lines are drawn as Table 178 places them: perpendicular to the line, on the
/// clockwise side of the direction from the first point to the second for a positive
/// `/LL`, and the line itself is drawn at their far ends. A caption (`/Cap`) is not set.
pub(super) fn line(entries: &Entries<'_>, area: Area) -> Drawing {
    let [x1, y1, x2, y2] = match entries.numbers("L")[..] {
        [a, b, c, d] => [a, b, c, d],
        _ => [area.left, area.bottom, area.right, area.top],
    };
    let (start, end) = (local(area, x1, y1), local(area, x2, y2));
    let Some(along) = unit(start, end) else { return Drawing::of(String::new()) };
    // Clockwise of the direction of travel.
    let side = (along.1, -along.0);
    let reach = entries.number("LL").unwrap_or(0.0);
    let extension = entries.number("LLE").unwrap_or(0.0).max(0.0) * reach.signum();
    let offset = entries.number("LLO").unwrap_or(0.0).max(0.0) * reach.signum();
    let shift = |p: Point, by: f64| (side.0.mul_add(by, p.0), side.1.mul_add(by, p.1));
    let (from, to) = (shift(start, reach), shift(end, reach));

    let pen = entries.pen();
    let mut content = stroke_setup(entries, &pen);
    if reach != 0.0 {
        for p in [start, end] {
            let (a, b) = (shift(p, offset), shift(p, reach + extension));
            let _ = writeln!(content, "{:.2} {:.2} m {:.2} {:.2} l S", a.0, a.1, b.0, b.1);
        }
    }
    let _ = writeln!(content, "{:.2} {:.2} m {:.2} {:.2} l S", from.0, from.1, to.0, to.1);
    let styles = entries.names("LE");
    let fill = entries.interior();
    for (tip, back, style) in
        [(from, along, styles.first()), (to, (-along.0, -along.1), styles.get(1))]
    {
        content.push_str(&ending(
            style.map_or("None", String::as_str),
            tip,
            back,
            pen.width,
            fill.as_deref(),
        ));
    }
    Drawing::of(content)
}

/// A square or a circle, inscribed in `/Rect` less `/RD`, stroked in `/C` and filled in
/// `/IC` (Table 180).
pub(super) fn shape(entries: &Entries<'_>, area: Area, round: bool) -> Drawing {
    let [l, t, r, b] = entries.inset();
    let pen = entries.pen();
    let half = pen.width / 2.0;
    let (x0, y0) = (l + half, b + half);
    let (x1, y1) = (area.width() - r - half, area.height() - t - half);
    let path = if round {
        ellipse(f64::midpoint(x0, x1), f64::midpoint(y0, y1), (x1 - x0) / 2.0, (y1 - y0) / 2.0)
    } else {
        format!("{x0:.2} {y0:.2} {:.2} {:.2} re\n", x1 - x0, y1 - y0)
    };
    Drawing::of(paint(entries, &pen, &path, true))
}

/// A polygon, closed and filled in `/IC`, or a polyline, open with its endings
/// (Table 181). `/Path`, where present, is drawn in place of `/Vertices`.
pub(super) fn polygon(entries: &Entries<'_>, area: Area, closed: bool) -> Drawing {
    let pen = entries.pen();
    let path = entries.arrays("Path");
    if !path.is_empty() {
        let built = path_operators(&path, area) + if closed { "h\n" } else { "" };
        return Drawing::of(paint(entries, &pen, &built, closed));
    }
    let points = pairs(&entries.numbers("Vertices"), area);
    let mut built = polyline(&points);
    if closed {
        built.push_str("h\n");
    }
    let mut content = paint(entries, &pen, &built, closed);
    if !closed {
        let styles = entries.names("LE");
        let fill = entries.interior();
        if let (Some(first), Some(second)) = (points.first(), points.get(1)) {
            let back = unit(*first, *second).unwrap_or((1.0, 0.0));
            content.push_str(&ending(
                styles.first().map_or("None", String::as_str),
                *first,
                back,
                pen.width,
                fill.as_deref(),
            ));
        }
        if let (Some(last), Some(before)) =
            (points.last(), points.len().checked_sub(2).and_then(|i| points.get(i)))
        {
            let back = unit(*last, *before).unwrap_or((-1.0, 0.0));
            content.push_str(&ending(
                styles.get(1).map_or("None", String::as_str),
                *last,
                back,
                pen.width,
                fill.as_deref(),
            ));
        }
    }
    Drawing::of(content)
}

/// Freehand strokes: each of `/InkList`'s paths, or `/Path` (Table 185).
pub(super) fn ink(entries: &Entries<'_>, area: Area) -> Drawing {
    let pen = entries.pen();
    let path = entries.arrays("Path");
    let built = if path.is_empty() {
        entries.arrays("InkList").iter().map(|stroke| polyline(&pairs(stroke, area))).collect()
    } else {
        path_operators(&path, area)
    };
    Drawing::of(paint(entries, &pen, &built, false))
}

/// The pen and colour a stroke is drawn with: `/C`, or black where there is none.
fn stroke_setup(entries: &Entries<'_>, pen: &super::Pen) -> String {
    let colour = entries.colour(true).unwrap_or_else(|| "0 G\n".to_owned());
    format!("{colour}{}", pen.operators())
}

/// `path` stroked in `/C` and, where `fill` and `/IC` say so, filled in `/IC`. A zero
/// width with no interior draws nothing, as Table 168 says a zero width means.
fn paint(entries: &Entries<'_>, pen: &super::Pen, path: &str, fillable: bool) -> String {
    let interior = entries.interior().filter(|_| fillable);
    let stroked = pen.width > 0.0;
    let operator = match (stroked, interior.is_some()) {
        (true, true) => "B",
        (true, false) => "S",
        (false, true) => "f",
        (false, false) => return String::new(),
    };
    format!("{}{}{path}{operator}\n", stroke_setup(entries, pen), interior.unwrap_or_default())
}

/// Alternating coordinates as points in the annotation's space.
fn pairs(values: &[f64], area: Area) -> Vec<Point> {
    values.as_chunks::<2>().0.iter().map(|[x, y]| local(area, *x, *y)).collect()
}

/// `m` to the first point and `l` to the rest.
fn polyline(points: &[Point]) -> String {
    let mut path = String::new();
    for (nth, (x, y)) in points.iter().enumerate() {
        let _ = writeln!(path, "{x:.2} {y:.2} {}", if nth == 0 { "m" } else { "l" });
    }
    path
}

/// `/Path`'s arrays as `m`, `l` and `c` (Tables 181 and 185).
fn path_operators(path: &[Vec<f64>], area: Area) -> String {
    let mut built = String::new();
    for (nth, operands) in path.iter().enumerate() {
        let points = pairs(operands, area);
        let numbers: Vec<String> = points.iter().map(|(x, y)| format!("{x:.2} {y:.2}")).collect();
        let operator = match (nth, points.len()) {
            (0, _) => "m",
            (_, 3) => "c",
            _ => "l",
        };
        let _ = writeln!(built, "{} {operator}", numbers.join(" "));
    }
    built
}

/// The direction from `a` to `b`, of length one; none where they are the same point.
pub(super) fn unit(a: Point, b: Point) -> Option<Point> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx.hypot(dy);
    (length > f64::EPSILON).then(|| (dx / length, dy / length))
}

/// An ellipse centred on `(cx, cy)`, as four Bézier quarters.
pub(super) fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> String {
    const K: f64 = 0.552_284_75;
    let (kx, ky) = (rx * K, ry * K);
    format!(
        "{:.2} {cy:.2} m\n{:.2} {:.2} {:.2} {:.2} {cx:.2} {:.2} c\n{:.2} {:.2} {:.2} {:.2} {:.2} {cy:.2} c\n\
         {:.2} {:.2} {:.2} {:.2} {cx:.2} {:.2} c\n{:.2} {:.2} {:.2} {:.2} {:.2} {cy:.2} c\nh\n",
        cx + rx,
        cx + rx,
        cy + ky,
        cx + kx,
        cy + ry,
        cy + ry,
        cx - kx,
        cy + ry,
        cx - rx,
        cy + ky,
        cx - rx,
        cx - rx,
        cy - ky,
        cx - kx,
        cy - ry,
        cy - ry,
        cx + kx,
        cy - ry,
        cx + rx,
        cy - ky,
        cx + rx,
    )
}

/// One of Table 179's endings at `tip`, where `back` points from the tip into the line.
///
/// The size is the one most readers draw, three times the width plus three, so a hairline
/// still shows an arrowhead. A closed ending is filled in `/IC` where there is one, and
/// stroked either way; an unknown name is `None`, as Table 179 leaves it.
pub(super) fn ending(
    style: &str,
    tip: Point,
    back: Point,
    width: f64,
    fill: Option<&str>,
) -> String {
    let size = width.mul_add(3.0, 3.0);
    let at = |along: f64, across: f64| {
        (
            back.1.mul_add(-across, back.0.mul_add(along, tip.0)),
            back.0.mul_add(across, back.1.mul_add(along, tip.1)),
        )
    };
    let segment =
        |a: Point, b: Point| format!("{:.2} {:.2} m {:.2} {:.2} l S\n", a.0, a.1, b.0, b.1);
    let closed = |points: &[Point]| {
        let operator = if fill.is_some() { "b" } else { "s" };
        format!("{}{}{operator}\n", fill.unwrap_or_default(), polyline(points))
    };
    let (spread, half) = (size * 0.866, size / 2.0);
    match style {
        "Square" => closed(&[at(-half, -half), at(half, -half), at(half, half), at(-half, half)]),
        "Circle" => {
            let operator = if fill.is_some() { "b" } else { "s" };
            format!("{}{}{operator}\n", fill.unwrap_or_default(), ellipse(tip.0, tip.1, half, half))
        }
        "Diamond" => closed(&[at(-half, 0.0), at(0.0, -half), at(half, 0.0), at(0.0, half)]),
        "OpenArrow" => segment(at(spread, half), tip) + &segment(tip, at(spread, -half)),
        "ClosedArrow" => closed(&[tip, at(spread, half), at(spread, -half)]),
        "ROpenArrow" => segment(at(-spread, half), tip) + &segment(tip, at(-spread, -half)),
        "RClosedArrow" => closed(&[tip, at(-spread, half), at(-spread, -half)]),
        "Butt" => segment(at(0.0, half), at(0.0, -half)),
        // Thirty degrees clockwise of the perpendicular.
        "Slash" => segment(at(-half * 0.5, half * 0.866), at(half * 0.5, -half * 0.866)),
        _ => String::new(),
    }
}
