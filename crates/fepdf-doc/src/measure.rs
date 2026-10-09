//! The scale a drawing declares for measuring on it (ISO 32000-2 12.9), read and
//! written (ROADMAP W-16).
//!
//! **A scale belongs to a viewport, not to a page.** A page's `/VP` holds viewports, each a
//! rectangle with its own `/Measure`; a point is measured in the last viewport whose
//! `/BBox` holds it, and a distance in the viewport of its first point (12.9.1).
//!
//! **A measurement is shown as the standard says to show it**, by the number format
//! arrays the measure dictionary carries (12.9.2): `/D` for a distance, `/A` for an area,
//! each converting from the units of `/X`'s first element.

use bytes::Bytes;
use fepdf_model::arena::PdfArena;
use fepdf_model::{Document, Handle, Object, PdfName, PdfResult};
use std::collections::BTreeMap;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// How the fractional part of the last unit is shown (Table 268, `/F`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fraction {
    /// As a decimal, to the precision `/D` names.
    Decimal,
    /// As a fraction whose denominator `/D` names.
    Fraction,
    /// Rounded to a whole unit.
    Round,
    /// Truncated to a whole unit.
    Truncate,
}

/// One unit of a number format array (Table 268).
#[derive(Debug, Clone, PartialEq)]
pub struct NumberFormat {
    /// `/U`: the label.
    pub unit: String,
    /// `/C`: what the value in the previous unit is multiplied by to be in this one.
    pub factor: f64,
    /// `/F`.
    pub fraction: Fraction,
    /// `/D`: the decimal precision, or the fraction's denominator.
    pub precision: u32,
    /// `/FD`: keep the precision as written, rather than dropping zeros or reducing.
    pub exact: bool,
    /// `/RT`: between orders of thousands.
    pub thousands: String,
    /// `/RD`: the decimal point.
    pub decimal: String,
    /// `/PS` and `/SS`: before and after the label.
    pub spacing: (String, String),
    /// `/O`: whether the label comes before the value.
    pub prefix: bool,
}

impl NumberFormat {
    /// A unit with every optional entry at its default.
    #[must_use]
    pub fn plain(unit: &str, factor: f64) -> Self {
        Self {
            unit: unit.to_owned(),
            factor,
            fraction: Fraction::Decimal,
            precision: 100,
            exact: false,
            thousands: ",".to_owned(),
            decimal: ".".to_owned(),
            spacing: (" ".to_owned(), " ".to_owned()),
            prefix: false,
        }
    }
}

/// A rectilinear scale (Table 267), and the viewport it holds for.
#[derive(Debug, Clone, PartialEq)]
pub struct Scale {
    /// `/R`: the ratio, as the drawing states it.
    pub ratio: String,
    /// `/X`: user space units into the measuring units along x.
    pub x: Vec<NumberFormat>,
    /// `/Y` with `/CYX`: along y, when it differs, and what makes y units x units.
    pub y: Option<(Vec<NumberFormat>, f64)>,
    /// `/D`: distances.
    pub distance: Vec<NumberFormat>,
    /// `/A`: areas.
    pub area: Vec<NumberFormat>,
    /// The viewport's `/BBox`, in default user space.
    pub bbox: [f64; 4],
}

impl Scale {
    /// Whether `point` is inside the viewport this scale holds for.
    #[must_use]
    pub fn holds(&self, point: (f64, f64)) -> bool {
        let [x0, y0, x1, y1] = self.bbox;
        (x0.min(x1)..=x0.max(x1)).contains(&point.0) && (y0.min(y1)..=y0.max(y1)).contains(&point.1)
    }

    /// The first units of `/X` and `/Y` a user space change of `(dx, dy)` is, in x units.
    fn in_x_units(&self, dx: f64, dy: f64) -> (f64, f64) {
        let x = self.x.first().map_or(1.0, |f| f.factor);
        let y = self.y.as_ref().map_or(x, |(y, cyx)| y.first().map_or(1.0, |f| f.factor) * cyx);
        (dx * x, dy * y)
    }

    /// How far it is from one point to the other, formatted by `/D`.
    #[must_use]
    pub fn distance(&self, from: (f64, f64), to: (f64, f64)) -> String {
        let (dx, dy) = self.in_x_units(to.0 - from.0, to.1 - from.1);
        format(dx.hypot(dy), &self.distance)
    }

    /// The length of the path through `points`, formatted by `/D`.
    #[must_use]
    pub fn path_length(&self, points: &[(f64, f64)]) -> String {
        let length = points
            .windows(2)
            .map(|pair| match *pair {
                [(x0, y0), (x1, y1)] => {
                    let (dx, dy) = self.in_x_units(x1 - x0, y1 - y0);
                    dx.hypot(dy)
                }
                _ => 0.0,
            })
            .sum();
        format(length, &self.distance)
    }

    /// The area the polygon through `points` encloses, formatted by `/A`.
    #[must_use]
    pub fn area(&self, points: &[(f64, f64)]) -> String {
        let scaled: Vec<(f64, f64)> = points.iter().map(|&(x, y)| self.in_x_units(x, y)).collect();
        format(polygon_area(&scaled), &self.area)
    }
}

/// The area of the polygon through `points`, closed back to the first (the shoelace).
#[must_use]
pub fn polygon_area(points: &[(f64, f64)]) -> f64 {
    let twice: f64 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a.0.mul_add(b.1, -(b.0 * a.1)))
        .sum();
    (twice / 2.0).abs()
}

/// `value` written out by the number format array `formats` (12.9.2).
///
/// Each unit but the last takes the whole part and hands on the fraction, multiplied into
/// the next; the last shows what remains as its `/F` says. A value with no fraction left
/// stops at the unit that took the last of it.
#[must_use]
pub fn format(value: f64, formats: &[NumberFormat]) -> String {
    let mut out = String::new();
    let mut value = value;
    for (index, unit) in formats.iter().enumerate() {
        value *= unit.factor;
        let last = index + 1 == formats.len();
        let whole = value.trunc();
        let part = value - whole;
        if last || part.abs() < 1e-9 {
            let written = last_value(value, unit);
            out.push_str(&labelled(&written, unit));
            break;
        }
        out.push_str(&labelled(&grouped(whole, &unit.thousands), unit));
        value = part;
    }
    out.trim_end().to_owned()
}

/// The value with its label on the side `/O` says, spaced by `/PS` and `/SS`.
fn labelled(value: &str, unit: &NumberFormat) -> String {
    let label = format!("{}{}{}", unit.spacing.0, unit.unit, unit.spacing.1);
    if unit.prefix { format!("{label}{value}") } else { format!("{value}{label}") }
}

/// The last unit's value, with its fraction shown as `/F`, `/D` and `/FD` say.
fn last_value(value: f64, unit: &NumberFormat) -> String {
    let denominator = f64::from(unit.precision.max(1));
    match unit.fraction {
        Fraction::Round => grouped(value.round(), &unit.thousands),
        Fraction::Truncate => grouped(value.trunc(), &unit.thousands),
        Fraction::Decimal => {
            let digits = decimal_digits(unit.precision);
            let rounded = (value * denominator).round() / denominator;
            let whole = rounded.trunc();
            let mut fraction = format!("{:.*}", digits, rounded - whole);
            // `format!` writes "0.25"; the digits are what follow its point.
            fraction = fraction.split_once('.').map_or_else(String::new, |(_, f)| f.to_owned());
            if !unit.exact {
                fraction = fraction.trim_end_matches('0').to_owned();
            }
            let whole = grouped(whole, &unit.thousands);
            if fraction.is_empty() { whole } else { format!("{whole}{}{fraction}", unit.decimal) }
        }
        Fraction::Fraction => {
            let whole = value.trunc();
            let mut numerator = ((value - whole) * denominator).round();
            let (mut whole, mut over) = (whole, denominator);
            if numerator >= over {
                whole += 1.0;
                numerator = 0.0;
            }
            if numerator == 0.0 {
                return grouped(whole, &unit.thousands);
            }
            if !unit.exact {
                let common = gcd(numerator, over);
                numerator /= common;
                over /= common;
            }
            format!("{} {numerator}/{over}", grouped(whole, &unit.thousands))
        }
    }
}

/// How many decimal digits a precision of `precision` is: 100 is two.
fn decimal_digits(precision: u32) -> usize {
    let mut digits = 0;
    let mut left = precision;
    while left >= 10 {
        left /= 10;
        digits += 1;
    }
    digits
}

/// The greatest common divisor of two whole numbers held as floats.
fn gcd(a: f64, b: f64) -> f64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    for _ in 0..64 {
        if b < 0.5 {
            break;
        }
        (a, b) = (b, a % b);
    }
    a.max(1.0)
}

/// A whole number with `separator` between its orders of thousands.
fn grouped(whole: f64, separator: &str) -> String {
    let digits = format!("{:.0}", whole.abs());
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push_str(separator);
        }
        out.push(digit);
    }
    if whole < 0.0 { format!("-{out}") } else { out }
}

/// The scale that holds at `point` on `page`: the last viewport whose `/BBox` holds it,
/// when that viewport's measure is rectilinear (12.9.1).
#[must_use]
pub fn scale_at(doc: &Document, page: usize, point: (f64, f64)) -> Option<Scale> {
    holding(&scales_on(doc, page), point).cloned()
}

/// Of `scales`, in the page's viewport order, the one that holds at `point`: the last
/// whose viewport holds it (12.9.1).
#[must_use]
pub fn holding(scales: &[Scale], point: (f64, f64)) -> Option<&Scale> {
    scales.iter().rev().find(|scale| scale.holds(point))
}

/// Every rectilinear scale `page` declares, in its viewport order.
#[must_use]
pub fn scales_on(doc: &Document, page: usize) -> Vec<Scale> {
    let arena = doc.arena();
    let Some(page) = doc.get_page_handle(page).and_then(|p| doc.resolve_to_dict(p).ok()) else {
        return Vec::new();
    };
    let viewports = match arena.dict_entry(page, arena.name("VP")).map(|v| v.resolve(arena)) {
        Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
        _ => return Vec::new(),
    };
    viewports
        .iter()
        .filter_map(|viewport| {
            let viewport = arena.get_dict(viewport.resolve(arena).as_dict_handle()?)?;
            let bbox = rectangle(arena, viewport.get(&arena.name("BBox"))?)?;
            let measure = viewport.get(&arena.name("Measure"))?.resolve(arena).as_dict_handle()?;
            rectilinear(arena, &arena.get_dict(measure)?, bbox)
        })
        .collect()
}

/// A measure dictionary, when it is rectilinear and carries what Table 267 requires.
fn rectilinear(arena: &PdfArena, measure: &Dict, bbox: [f64; 4]) -> Option<Scale> {
    let name = |key: &str| {
        measure
            .get(&arena.name(key))
            .and_then(|v| v.resolve(arena).as_name())
            .and_then(|n| arena.get_name(n))
            .map(|n| n.as_str().to_string())
    };
    if name("Subtype").is_some_and(|s| s != "RL") {
        return None;
    }
    let formats = |key: &str| number_formats(arena, measure.get(&arena.name(key))?);
    let y = formats("Y").map(|y| {
        let cyx = measure.get(&arena.name("CYX")).and_then(|c| c.resolve(arena).as_f64());
        (y, cyx.unwrap_or(1.0))
    });
    Some(Scale {
        ratio: text(arena, measure.get(&arena.name("R"))?)?,
        x: formats("X")?,
        y,
        distance: formats("D")?,
        area: formats("A")?,
        bbox,
    })
}

/// A number format array (Table 268), or nothing when one element lacks `/U` or `/C`.
fn number_formats(arena: &PdfArena, value: &Object) -> Option<Vec<NumberFormat>> {
    let Object::Array(array) = value.resolve(arena) else { return None };
    let formats: Option<Vec<NumberFormat>> = arena
        .get_array(array)?
        .iter()
        .map(|element| {
            number_format(arena, &arena.get_dict(element.resolve(arena).as_dict_handle()?)?)
        })
        .collect();
    formats.filter(|f| !f.is_empty())
}

/// One number format dictionary.
fn number_format(arena: &PdfArena, dict: &Dict) -> Option<NumberFormat> {
    let entry = |key: &str| dict.get(&arena.name(key)).map(|v| v.resolve(arena));
    let words = |key: &str, default: &str| {
        entry(key).and_then(|v| text(arena, &v)).unwrap_or_else(|| default.to_owned())
    };
    let named = |key: &str| {
        entry(key)
            .and_then(|v| v.as_name())
            .and_then(|n| arena.get_name(n))
            .map(|n| n.as_str().to_string())
    };
    let mut format = NumberFormat::plain(&text(arena, &entry("U")?)?, entry("C")?.as_f64()?);
    format.fraction = match named("F").as_deref() {
        Some("F") => Fraction::Fraction,
        Some("R") => Fraction::Round,
        Some("T") => Fraction::Truncate,
        _ => Fraction::Decimal,
    };
    let default = if format.fraction == Fraction::Fraction { 16 } else { 100 };
    format.precision = entry("D")
        .and_then(|d| d.as_f64())
        .filter(|d| *d >= 1.0 && *d <= f64::from(u32::MAX))
        .map_or(default, whole_u32);
    format.exact = matches!(entry("FD"), Some(Object::Boolean(true)));
    format.thousands = words("RT", ",");
    format.decimal = words("RD", ".");
    if format.decimal.is_empty() {
        ".".clone_into(&mut format.decimal);
    }
    format.spacing = (words("PS", " "), words("SS", " "));
    format.prefix = named("O").as_deref() == Some("P");
    Some(format)
}

/// A number already known to be a whole number in `u32`'s range.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // checked by the caller
fn whole_u32(value: f64) -> u32 {
    value.round() as u32
}

/// A text string's text, in either of the shapes it is stored in.
fn text(arena: &PdfArena, value: &Object) -> Option<String> {
    match value.resolve(arena) {
        Object::Text(text) => Some(text),
        Object::String(bytes) | Object::Hex(bytes) => {
            Some(fepdf_model::refine::text::recover_string(&bytes))
        }
        _ => None,
    }
}

/// Four numbers.
fn rectangle(arena: &PdfArena, value: &Object) -> Option<[f64; 4]> {
    let Object::Array(array) = value.resolve(arena) else { return None };
    let numbers: Vec<f64> =
        arena.get_array(array)?.iter().filter_map(|n| n.resolve(arena).as_f64()).collect();
    numbers.try_into().ok()
}

/// A rectilinear measure dictionary saying one unit of user space is `per_unit` `unit`s
/// (Table 267), on a page whose unit is `user_unit` points (Table 31).
///
/// Distances are in `unit`, and areas in its square. `/R` states the ratio per inch of the
/// sheet, the way a drawing's title block does: an inch is `72 / user_unit` units, so on a
/// page of `/UserUnit 10` it said ten times the drawing's scale until 2026-09-30.
pub fn write_rectilinear(
    arena: &PdfArena,
    per_unit: f64,
    user_unit: f64,
    unit: &str,
) -> Handle<Object> {
    let format_array = |label: &str, factor: f64| {
        let mut dict = BTreeMap::new();
        dict.insert(arena.name("Type"), Object::Name(arena.name("NumberFormat")));
        dict.insert(arena.name("U"), Object::Text(label.to_owned()));
        dict.insert(arena.name("C"), Object::Real(factor));
        Object::Array(arena.alloc_array(vec![Object::Dictionary(arena.alloc_dict(dict))]))
    };
    let mut measure = BTreeMap::new();
    measure.insert(arena.name("Type"), Object::Name(arena.name("Measure")));
    measure.insert(arena.name("Subtype"), Object::Name(arena.name("RL")));
    let per_inch = per_unit * 72.0 / user_unit;
    // Written by the number formatting the distances use, to six places: the ratio came
    // in as an `f32`, and its last digits are noise rather than the drawing's scale.
    let mut stated = NumberFormat::plain(unit, 1.0);
    stated.precision = 1_000_000;
    let ratio = format!("1 in = {}", format(per_inch, &[stated]));
    measure.insert(arena.name("R"), Object::Text(ratio));
    measure.insert(arena.name("X"), format_array(unit, per_unit));
    measure.insert(arena.name("D"), format_array(unit, 1.0));
    measure.insert(arena.name("A"), format_array(&format!("{unit}²"), 1.0));
    arena.alloc_object(Object::Dictionary(arena.alloc_dict(measure)))
}

/// Puts a viewport over `bbox` on `page` with `measure` as its scale, in place of any
/// viewport whose measure has the same `/Subtype`.
///
/// # Errors
/// Fails when the page is not there.
pub fn set_viewport(
    doc: &Document,
    page: usize,
    bbox: [f64; 4],
    name: &str,
    measure: Handle<Object>,
) -> PdfResult<()> {
    let arena = doc.arena();
    let page_h = doc.page_handle(page)?;
    let subtype_of = |measure: &Object| {
        let dict = arena.get_dict(measure.resolve(arena).as_dict_handle()?)?;
        let subtype = dict.get(&arena.name("Subtype")).and_then(|s| s.resolve(arena).as_name());
        Some(
            subtype
                .and_then(|n| arena.get_name(n))
                .map_or("RL".to_owned(), |n| n.as_str().to_string()),
        )
    };
    let ours = arena.get_object(measure).and_then(|m| subtype_of(&m));
    let page_dh = doc.resolve_to_dict(page_h)?;
    let mut page_dict = arena.get_dict(page_dh).unwrap_or_default();
    let mut kept: Vec<Object> = match page_dict.get(&arena.name("VP")).map(|v| v.resolve(arena)) {
        Some(Object::Array(array)) => arena.get_array(array).unwrap_or_default(),
        _ => Vec::new(),
    };
    kept.retain(|viewport| {
        let theirs = viewport
            .resolve(arena)
            .as_dict_handle()
            .and_then(|dh| arena.get_dict(dh))
            .and_then(|dict| dict.get(&arena.name("Measure")).and_then(&subtype_of));
        theirs != ours
    });
    let mut viewport = BTreeMap::new();
    viewport.insert(arena.name("Type"), Object::Name(arena.name("Viewport")));
    let corners = bbox.iter().map(|v| Object::Real(*v)).collect();
    viewport.insert(arena.name("BBox"), Object::Array(arena.alloc_array(corners)));
    viewport.insert(arena.name("Name"), Object::String(Bytes::from(name.to_owned())));
    viewport.insert(arena.name("Measure"), Object::Reference(measure));
    kept.push(Object::Dictionary(arena.alloc_dict(viewport)));
    page_dict.insert(arena.name("VP"), Object::Array(arena.alloc_array(kept)));
    arena.set_dict(page_dh, page_dict);
    Ok(())
}

/// The standard's own example, and what a scale does with a distance and an area.
#[cfg(test)]
mod formatted {
    use super::{Fraction, NumberFormat, format, polygon_area};

    /// **12.9.2's example**: 1.4505 miles is "1 mi 2,378 ft 7 5/8 in".
    #[test]
    fn the_standards_example() {
        let mut inches = NumberFormat::plain("in", 12.0);
        inches.fraction = Fraction::Fraction;
        inches.precision = 8;
        let distance = [NumberFormat::plain("mi", 1.0), NumberFormat::plain("ft", 5280.0), inches];
        assert_eq!(format(1.4505, &distance), "1 mi 2,378 ft 7 5/8 in");
    }

    /// A decimal drops the zeros it does not need, unless `/FD` keeps them.
    #[test]
    fn a_decimal_to_its_precision() {
        let mut metres = NumberFormat::plain("m", 1.0);
        assert_eq!(format(12.5, &[metres.clone()]), "12.5 m");
        assert_eq!(format(1234.0, &[metres.clone()]), "1,234 m");
        metres.exact = true;
        assert_eq!(format(12.5, &[metres]), "12.50 m");
    }

    /// A fraction is reduced unless `/FD` says not to, and one that rounds up carries.
    #[test]
    fn a_fraction_reduced_and_carried() {
        let mut inches = NumberFormat::plain("in", 1.0);
        inches.fraction = Fraction::Fraction;
        inches.precision = 16;
        assert_eq!(format(2.5, &[inches.clone()]), "2 1/2 in");
        assert_eq!(format(2.999, &[inches.clone()]), "3 in");
        inches.exact = true;
        assert_eq!(format(2.5, &[inches]), "2 8/16 in");
    }

    /// The shoelace, either way round.
    #[test]
    fn an_area_is_the_same_either_way_round() {
        let square = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!((polygon_area(&square) - 100.0).abs() < 1e-9);
        let mut backwards = square;
        backwards.reverse();
        assert!((polygon_area(&backwards) - 100.0).abs() < 1e-9);
    }
}
