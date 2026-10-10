//! What marks text, by `/QuadPoints`: Highlight, Underline, StrikeOut and Squiggly
//! (Table 182), and Redact before it is applied (Table 195).
//!
//! **Which edge of a quadrilateral is its foot is worked out, not assumed.** Table 182
//! gives the four vertices "in counterclockwise order" with the text along the edge from
//! the first to the second. Writers in the field put the first two along the top instead,
//! the order this engine's own `AddAnnotation` writes. Both agree that the text runs from
//! the first point to the second. So the other two points say which side is up: if they
//! lie to the left of that direction, the first edge is the foot; if to the right, it is
//! the top.

use super::{Area, Drawing, Entries, local, merge_state};
use fepdf_model::Object;
use std::fmt::Write as _;

type Point = (f64, f64);

/// One quadrilateral, as its foot and its head, each from where the text starts to where
/// it ends.
struct Quad {
    foot: (Point, Point),
    head: (Point, Point),
}

impl Quad {
    /// How tall it is, across the text.
    fn height(&self) -> f64 {
        let (a, b) = (self.foot.0, self.head.0);
        (b.0 - a.0).hypot(b.1 - a.1)
    }

    /// A line parallel to the foot, `share` of the way up.
    fn across(&self, share: f64) -> (Point, Point) {
        let mix =
            |f: Point, h: Point| ((h.0 - f.0).mul_add(share, f.0), (h.1 - f.1).mul_add(share, f.1));
        (mix(self.foot.0, self.head.0), mix(self.foot.1, self.head.1))
    }
}

/// A text markup's drawing.
pub(super) fn marking(entries: &Entries<'_>, area: Area, subtype: &str) -> Drawing {
    let quads = quads_of(entries, area);
    let colour =
        |stroking| entries.colour(stroking).unwrap_or_else(|| default_colour(subtype, stroking));
    let mut content = String::new();
    if subtype == "Highlight" {
        // Multiplied, so the words it marks stay dark under it (11.3.5).
        let mut drawing = Drawing::of(String::new());
        let mut state = super::Dict::new();
        state.insert(entries.arena.name("BM"), Object::Name(entries.arena.name("Multiply")));
        merge_state(entries.arena, &mut drawing.resources, "GM", state);
        content.push_str("/GM gs\n");
        content.push_str(&colour(false));
        for q in &quads {
            let _ = writeln!(content, "{}f", outline(q));
        }
        drawing.content = content;
        return drawing;
    }
    content.push_str(&colour(true));
    for q in &quads {
        let width = (q.height() / 14.0).max(0.5);
        let _ = writeln!(content, "{width:.2} w 1 J");
        match subtype {
            "StrikeOut" => content.push_str(&segment(q.across(0.5))),
            "Squiggly" => content.push_str(&squiggle(q)),
            _ => content.push_str(&segment(q.across(0.08))),
        }
    }
    Drawing::of(content)
}

/// A redaction not yet applied: the outline of what it will remove, in `/C`, or red where
/// it has none. Table 195's `/IC`, `/RO` and `/OverlayText` describe the page *after* it
/// is applied, so they are not drawn here.
pub(super) fn redaction(entries: &Entries<'_>, area: Area) -> Drawing {
    let mut content = entries.colour(true).unwrap_or_else(|| "1 0 0 RG\n".to_owned());
    content.push_str("1 w\n");
    for q in &quads_of(entries, area) {
        let _ = writeln!(content, "{}s", outline(q));
    }
    Drawing::of(content)
}

/// What a markup is drawn in when it states no `/C`: yellow for a highlight, as readers
/// draw one, and red for the rest.
fn default_colour(subtype: &str, stroking: bool) -> String {
    let rgb = if subtype == "Highlight" { "1 1 0" } else { "1 0 0" };
    format!("{rgb} {}\n", if stroking { "RG" } else { "rg" })
}

/// The quadrilaterals of `/QuadPoints`, or `/Rect` as one where there are none.
fn quads_of(entries: &Entries<'_>, area: Area) -> Vec<Quad> {
    let given: Vec<Quad> = entries
        .numbers("QuadPoints")
        .as_chunks::<8>()
        .0
        .iter()
        .map(|[x1, y1, x2, y2, x3, y3, x4, y4]| {
            quad([
                local(area, *x1, *y1),
                local(area, *x2, *y2),
                local(area, *x3, *y3),
                local(area, *x4, *y4),
            ])
        })
        .collect();
    if !given.is_empty() {
        return given;
    }
    let (w, h) = (area.width(), area.height());
    vec![Quad { foot: ((0.0, 0.0), (w, 0.0)), head: ((0.0, h), (w, h)) }]
}

/// A quadrilateral from its four points in either order the field writes.
fn quad([p1, p2, p3, p4]: [Point; 4]) -> Quad {
    let along = (p2.0 - p1.0, p2.1 - p1.1);
    // Left of the direction of the text is up.
    let up = (-along.1, along.0);
    let middle = (f64::midpoint(p3.0, p4.0) - p1.0, f64::midpoint(p3.1, p4.1) - p1.1);
    // Whichever of the other two is nearer the first point starts the other edge.
    let (start, end) = if (p3.0 - p1.0).hypot(p3.1 - p1.1) <= (p4.0 - p1.0).hypot(p4.1 - p1.1) {
        (p3, p4)
    } else {
        (p4, p3)
    };
    if middle.0.mul_add(up.0, middle.1 * up.1) >= 0.0 {
        Quad { foot: (p1, p2), head: (start, end) }
    } else {
        Quad { foot: (start, end), head: (p1, p2) }
    }
}

fn outline(quad: &Quad) -> String {
    let ((foot_start, foot_end), (head_start, head_end)) = (quad.foot, quad.head);
    let at = |(x, y): Point| format!("{x:.2} {y:.2}");
    format!("{} m {} l {} l {} l h\n", at(foot_start), at(foot_end), at(head_end), at(head_start))
}

fn segment((a, b): (Point, Point)) -> String {
    format!("{:.2} {:.2} m {:.2} {:.2} l S\n", a.0, a.1, b.0, b.1)
}

/// A wavy line along the foot, its amplitude a twelfth of the height.
fn squiggle(q: &Quad) -> String {
    let (a, b) = q.foot;
    let length = (b.0 - a.0).hypot(b.1 - a.1);
    let height = q.height();
    if length <= f64::EPSILON || height <= f64::EPSILON {
        return String::new();
    }
    let along = ((b.0 - a.0) / length, (b.1 - a.1) / length);
    let up = ((q.head.0.0 - a.0) / height, (q.head.0.1 - a.1) / height);
    let amplitude = (height / 12.0).max(0.5);
    let step = amplitude * 2.0;
    let mut path = format!("{:.2} {:.2} m\n", a.0, a.1);
    let mut high = true;
    // Counted rather than compared, so the last peak lands on the end exactly.
    let steps = super::super::markup::whole(length / step);
    for nth in 1..=steps {
        let done = (step * f64::from(nth)).min(length);
        let lift = if high { amplitude * 2.0 } else { 0.0 };
        let _ = writeln!(
            path,
            "{:.2} {:.2} l",
            up.0.mul_add(lift, along.0.mul_add(done, a.0)),
            up.1.mul_add(lift, along.1.mul_add(done, a.1))
        );
        high = !high;
    }
    path.push_str("S\n");
    path
}

#[cfg(test)]
mod tests {
    use super::quad;

    /// **Either order the field writes gives the same foot**: Table 182's counterclockwise
    /// one, which starts along the foot, and the one that starts along the top.
    #[test]
    fn the_foot_is_found_in_either_order() {
        let counterclockwise = quad([(0.0, 0.0), (100.0, 0.0), (100.0, 20.0), (0.0, 20.0)]);
        let top_first = quad([(0.0, 20.0), (100.0, 20.0), (0.0, 0.0), (100.0, 0.0)]);
        for q in [counterclockwise, top_first] {
            let ((a, b), (c, d)) = (q.foot, q.head);
            assert!(a.1.abs() < 1e-9 && b.1.abs() < 1e-9, "the foot is at the bottom: {a:?} {b:?}");
            assert!((c.1 - 20.0).abs() < 1e-9 && (d.1 - 20.0).abs() < 1e-9, "the head on top");
            assert!(a.0 < b.0 && c.0 < d.0, "both run the way the text does");
        }
    }
}
