//! Shading and Pattern integration tests for content-stream interpreter.

#![allow(clippy::float_cmp)]

use fepdf_model::{AxialShading, Color, ColorStop, RadialShading, ShadingSpec};

#[test]
fn test_shading_spec_construction() {
    let axial = AxialShading {
        coords: [0.0, 0.0, 100.0, 100.0],
        stops: vec![
            ColorStop::new(0.0, Color::Rgb(1.0, 0.0, 0.0)),
            ColorStop::new(1.0, Color::Rgb(0.0, 0.0, 1.0)),
        ],
        extend: [true, true],
    };
    let spec = ShadingSpec::Axial(axial);
    match spec {
        ShadingSpec::Axial(a) => {
            assert_eq!(a.coords, [0.0, 0.0, 100.0, 100.0]);
            assert_eq!(a.stops.len(), 2);
            assert_eq!(a.stops[0].offset, 0.0);
            assert_eq!(a.stops[1].offset, 1.0);
        }
        ShadingSpec::Radial(_) | ShadingSpec::Mesh(_) => panic!("Expected Axial shading"),
    }

    let radial = RadialShading {
        coords: [50.0, 50.0, 0.0, 50.0, 50.0, 100.0],
        stops: vec![ColorStop::new(0.0, Color::Gray(0.0)), ColorStop::new(1.0, Color::Gray(1.0))],
        extend: [false, true],
    };
    let rad_spec = ShadingSpec::Radial(radial);
    match rad_spec {
        ShadingSpec::Radial(r) => {
            assert_eq!(r.coords, [50.0, 50.0, 0.0, 50.0, 50.0, 100.0]);
            assert_eq!(r.extend, [false, true]);
        }
        ShadingSpec::Axial(_) | ShadingSpec::Mesh(_) => panic!("Expected Radial shading"),
    }
}
