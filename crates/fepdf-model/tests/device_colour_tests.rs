//! Conversions between the device colour spaces (ISO 32000-2, 10.4.2).
//!
//! 10.4.2.1 offers these algorithms to a PDF processor that is not ICC-enabled, which
//! this one is not, so they are the conformant answer here rather than a stopgap — and
//! they are *specified*, unlike the DeviceCMYK space itself, which 8.6.4.4 leaves
//! device-dependent.
//!
//! The case that matters is `c = 0.5, k = 0.5`. The clause and the naive
//! `(1 − c) × (1 − k)` this engine used agree wherever one of the pair is 0 or 1, which
//! is every case a casual test would pick — including the `/Separation` fixture, whose
//! `K = 1` gives black either way. Only a middling value tells them apart.

use fepdf_model::graphics::Color;

fn rgb(color: Color) -> (f64, f64, f64) {
    match color.to_rgb() {
        Color::Rgb(r, g, b) => (r, g, b),
        // Named rather than wildcarded (RR-15 Rule 5), so a `Color` variant added later
        // that `to_rgb` fails to convert breaks here instead of passing quietly.
        Color::Gray(..) | Color::Cmyk(..) | Color::Lab(..) => panic!("to_rgb kept a device space"),
    }
}

fn close(actual: (f64, f64, f64), expected: (f64, f64, f64)) -> bool {
    (actual.0 - expected.0).abs() < 1e-9_f64
        && (actual.1 - expected.1).abs() < 1e-9_f64
        && (actual.2 - expected.2).abs() < 1e-9_f64
}

#[test]
fn cmyk_adds_black_to_each_component_before_complementing() {
    // 10.4.2.5: red = 1.0 − min(1.0, cyan + black).
    //
    // This is the discriminating case. The clause gives 1 − min(1, 0.5 + 0.5) = 0; the
    // naive product gives (1 − 0.5) × (1 − 0.5) = 0.25. Anything that reports 0.25 here
    // is the old formula.
    assert!(close(rgb(Color::Cmyk(0.5, 0.5, 0.5, 0.5)), (0.0, 0.0, 0.0)));
    assert!(close(rgb(Color::Cmyk(0.25, 0.0, 0.0, 0.25)), (0.5, 0.75, 0.75)));
}

#[test]
fn cmyk_endpoints_are_the_colours_they_name() {
    assert!(close(rgb(Color::Cmyk(0.0, 0.0, 0.0, 0.0)), (1.0, 1.0, 1.0)), "no ink is white");
    assert!(close(rgb(Color::Cmyk(0.0, 0.0, 0.0, 1.0)), (0.0, 0.0, 0.0)), "full black");
    assert!(close(rgb(Color::Cmyk(1.0, 0.0, 0.0, 0.0)), (0.0, 1.0, 1.0)), "cyan absorbs red");
    assert!(close(rgb(Color::Cmyk(0.0, 1.0, 0.0, 0.0)), (1.0, 0.0, 1.0)), "magenta absorbs green");
    assert!(close(rgb(Color::Cmyk(0.0, 0.0, 1.0, 0.0)), (1.0, 1.0, 0.0)), "yellow absorbs blue");
}

#[test]
fn cmyk_saturates_rather_than_going_negative() {
    // `min(1.0, …)` is in the clause for this: ink plus black beyond 1.0 is still black,
    // not a negative channel that would wrap or clamp somewhere further downstream.
    assert!(close(rgb(Color::Cmyk(0.9, 0.9, 0.9, 0.9)), (0.0, 0.0, 0.0)));
}

#[test]
fn a_grey_level_is_the_rgb_value_with_all_three_the_same() {
    // 10.4.2.2.
    assert!(close(rgb(Color::Gray(0.0)), (0.0, 0.0, 0.0)));
    assert!(close(rgb(Color::Gray(1.0)), (1.0, 1.0, 1.0)));
    assert!(close(rgb(Color::Gray(0.4)), (0.4, 0.4, 0.4)));
}

#[test]
fn rgb_is_returned_unchanged() {
    assert!(close(rgb(Color::Rgb(0.1, 0.2, 0.3)), (0.1, 0.2, 0.3)));
}
