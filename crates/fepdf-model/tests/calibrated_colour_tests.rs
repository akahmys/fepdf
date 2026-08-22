//! `/CalRGB` (ISO 32000-2, 8.6.5.3) and the default colour spaces (8.6.5.6).
//!
//! The permuting matrix here is the one `DefaultRGBColourSpaces.pdf` uses — a file from
//! the `pdf-differences` corpus built for this clause, whose own comment labels it
//! `CalRGB --> G R B`. Working the clause through by hand: red in becomes XYZ `(0, 1, 0)`,
//! which is pure green out. That is what PDFKit paints, and it is what makes this
//! testable without a renderer.

use fepdf_model::color::ResolvedColorSpace;
use fepdf_model::graphics::Color;
use fepdf_model::{Handle, Object, PdfArena, PdfName};
use std::collections::BTreeMap;

fn dict(
    arena: &PdfArena,
    entries: Vec<(&str, Object)>,
) -> Handle<BTreeMap<Handle<PdfName>, Object>> {
    let mut map = BTreeMap::new();
    for (key, value) in entries {
        map.insert(arena.intern_name(PdfName::new(key)), value);
    }
    arena.alloc_dict(map)
}

fn nums(arena: &PdfArena, values: &[f64]) -> Object {
    Object::Array(arena.alloc_array(values.iter().map(|v| Object::Real(*v)).collect()))
}

/// `[/CalRGB << … >>]`.
fn cal_rgb(arena: &PdfArena, entries: Vec<(&str, Object)>) -> Object {
    let params = Object::Dictionary(dict(arena, entries));
    Object::Array(
        arena.alloc_array(vec![Object::Name(arena.intern_name(PdfName::new("CalRGB"))), params]),
    )
}

fn rgb(color: Color) -> (f64, f64, f64) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Gray(..) | Color::Cmyk(..) | Color::Lab(..) => panic!("expected Rgb"),
    }
}

fn close(actual: (f64, f64, f64), expected: (f64, f64, f64)) -> bool {
    (actual.0 - expected.0).abs() < 1e-6_f64
        && (actual.1 - expected.1).abs() < 1e-6_f64
        && (actual.2 - expected.2).abs() < 1e-6_f64
}

#[test]
fn a_permuting_matrix_moves_the_colour() {
    let arena = PdfArena::new();
    // `/Matrix [0 1 0  1 0 0  0 0 1]`, exactly as the corpus file writes it.
    let obj = cal_rgb(
        &arena,
        vec![
            ("WhitePoint", nums(&arena, &[1.0, 1.0, 1.0])),
            ("Matrix", nums(&arena, &[0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0])),
        ],
    );
    let space = ResolvedColorSpace::parse(&obj, &arena).expect("CalRGB parses");
    assert_eq!(space.components, 3);

    // Red in: A=1 contributes to Y alone, so XYZ is (0, 1, 0) and sRGB is pure green.
    assert!(close(rgb(space.to_color(&[1.0, 0.0, 0.0]).expect("red")), (0.0, 1.0, 0.0)));

    // And the swap goes the other way: B=1 contributes to X alone, so green in comes out
    // red. Asserted as "mostly red" rather than exactly, because XYZ (1, 0, 0) carries a
    // little blue into sRGB — the primaries do not line up, which is the point of going
    // through XYZ at all.
    let from_green = rgb(space.to_color(&[0.0, 1.0, 0.0]).expect("green"));
    assert!(from_green.0 > 0.9, "green in comes out red: {from_green:?}");
    assert!(from_green.1 < 0.1, "and not green: {from_green:?}");
}

#[test]
fn the_identity_matrix_is_not_the_identity_transform() {
    let arena = PdfArena::new();
    // `/Matrix` defaults to the identity, which maps A, B, C straight onto X, Y, Z — and
    // XYZ is *not* sRGB, so even this "does nothing" case moves the colour. Reading a
    // CalRGB as though it were DeviceRGB, which is what this engine did for every one of
    // them, is wrong even here.
    let obj = cal_rgb(&arena, vec![("WhitePoint", nums(&arena, &[1.0, 1.0, 1.0]))]);
    let space = ResolvedColorSpace::parse(&obj, &arena).expect("CalRGB parses");
    // A mid grey shows it: XYZ (0.5, 0.5, 0.5) is not sRGB (0.5, 0.5, 0.5). Saturated
    // primaries are a poor probe here, because clamping hides the difference — XYZ
    // (0, 1, 0) *does* land on sRGB (0, 1, 0) once the negative channels are clipped,
    // which is why this test asks a grey instead.
    let out = rgb(space.to_color(&[0.5, 0.5, 0.5]).expect("grey"));
    assert!(!close(out, (0.5, 0.5, 0.5)), "XYZ is not sRGB: {out:?}");
}

#[test]
fn gamma_decodes_each_component_before_the_matrix() {
    let arena = PdfArena::new();
    let obj = cal_rgb(
        &arena,
        vec![
            ("WhitePoint", nums(&arena, &[1.0, 1.0, 1.0])),
            ("Gamma", nums(&arena, &[2.0, 2.0, 2.0])),
            ("Matrix", nums(&arena, &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0])),
        ],
    );
    let space = ResolvedColorSpace::parse(&obj, &arena).expect("CalRGB parses");
    // 0.5 with gamma 2 decodes to 0.25 before the matrix sees it, so Y is 0.25 rather
    // than 0.5 and the result is darker than the same input with gamma 1.
    let dark = rgb(space.to_color(&[0.0, 0.5, 0.0]).expect("half green"));
    let plain = cal_rgb(&arena, vec![("WhitePoint", nums(&arena, &[1.0, 1.0, 1.0]))]);
    let plain = ResolvedColorSpace::parse(&plain, &arena).expect("parses");
    let bright = rgb(plain.to_color(&[0.0, 0.5, 0.0]).expect("half green"));
    assert!(dark.1 < bright.1, "gamma 2 must darken: {dark:?} vs {bright:?}");
}

#[test]
fn components_outside_the_range_are_clamped_not_refused() {
    let arena = PdfArena::new();
    // "component values falling outside that range shall be adjusted to the nearest
    // valid value without error indication".
    let obj = cal_rgb(&arena, vec![("WhitePoint", nums(&arena, &[1.0, 1.0, 1.0]))]);
    let space = ResolvedColorSpace::parse(&obj, &arena).expect("CalRGB parses");
    assert!(space.to_color(&[-2.0, 3.0, 0.5]).is_some());
}
