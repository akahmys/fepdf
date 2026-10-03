//! Type 1, function-based shading (ISO 32000-2, 8.7.4.5.2).
//!
//! **`ROADMAP.md` carried this as "Type 1 shading is not read, and says so."** The
//! parser's `match` on `/ShadingType` had arms for 2, 3 and 4 to 7 and a `_ => None`, so
//! `sh` on a function-based shading painted nothing. One file of 524 in both corpora
//! carries one, which measures the corpus rather than the world (AGENTS.md principle 3).
//!
//! The colour at a point is `f(x, y)`, and the engine already had the evaluator: types 0,
//! 2, 3 and 4 have been read since Phase P. What was missing was the shading that uses it.

use fepdf::PdfDocument;
use fepdf_fixtures::assemble;
use fepdf_fixtures::recorder::{Event, Recorder};
use kurbo::Affine;

/// A page whose only mark is `sh` on a Type 1 shading whose function is `content`.
///
/// Type 2 (exponential) with `/N 1` interpolates from `/C0` to `/C1` across its input,
/// and takes two inputs here because 8.7.4.5.2 evaluates over a two-dimensional domain.
fn page_with_function_shading(function: &str) -> Vec<u8> {
    let content = "q /Sh0 sh Q\n";
    assemble(&[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R \
          /Resources << /Shading << /Sh0 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        format!(
            "<< /ShadingType 1 /ColorSpace /DeviceRGB /Domain [0 1 0 1] \
             /Matrix [200 0 0 200 0 0] /Function {function} >>"
        ),
    ])
}

/// What the page painted, as the shading events and the fills that carry them.
fn drawn(file: Vec<u8>) -> Recorder {
    let doc = PdfDocument::open(file.into()).expect("the fixture opens");
    let mut recorder = Recorder::new();
    doc.render_page(0, &mut recorder, Affine::IDENTITY).expect("the page interprets");
    recorder
}

/// Exponential interpolation (type 2), the simplest function a shading can name.
///
/// Red to blue across its input. 8.7.4.5.2 evaluates over two dimensions and this takes
/// one, which the clause allows: what it says is that the function's domain must cover
/// the shading's, and a 1-in function is applied to the first coordinate.
const TWO_VARIABLE: &str = "<< /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >>";

#[test]
fn a_type_1_shading_is_read_rather_than_dropped() {
    let recorder = drawn(page_with_function_shading(TWO_VARIABLE));
    assert_eq!(
        recorder.count("shading"),
        1,
        "`sh` on a Type 1 shading reached the backend: {} calls",
        recorder.events.len()
    );
}

/// And what reached it is the grid, not an empty one.
#[test]
fn the_shading_carries_its_samples() {
    let recorder = drawn(page_with_function_shading(TWO_VARIABLE));
    let spec = recorder
        .events
        .iter()
        .find_map(|e| if let Event::Shading(s) = e { Some(s.clone()) } else { None })
        .expect("a shading reached the backend");
    let fepdf_content::ShadingSpec::FunctionBased(function) = spec else {
        panic!("a /ShadingType 1 read as something else: {spec:?}");
    };
    assert_eq!(function.resolution * function.resolution, function.samples.len());
    // Compared element by element: these are the numbers the dictionary wrote, not
    // arithmetic, so an exact match is the claim — but clippy is right that an array
    // comparison hides which element moved.
    for (read, wrote) in function.domain.iter().zip([0.0, 1.0, 0.0, 1.0]) {
        assert!((read - wrote).abs() < f64::EPSILON, "/Domain: {read} for {wrote}");
    }
    for (read, wrote) in function.matrix.iter().zip([200.0, 0.0, 0.0, 200.0, 0.0, 0.0]) {
        assert!((read - wrote).abs() < f64::EPSILON, "/Matrix: {read} for {wrote}");
    }
}

/// The cells cover the domain rather than dotting it, which is the difference between a
/// shading and a scatter of points.
#[test]
fn the_cells_tile_the_domain() {
    let recorder = drawn(page_with_function_shading(TWO_VARIABLE));
    let spec = recorder
        .events
        .iter()
        .find_map(|e| if let Event::Shading(s) = e { Some(s.clone()) } else { None })
        .expect("a shading reached the backend");
    let fepdf_content::ShadingSpec::FunctionBased(function) = spec else { panic!("wrong type") };

    let cells: Vec<_> = function.cells().collect();
    assert_eq!(cells.len(), function.samples.len(), "one cell per sample");
    let area: f64 = cells.iter().map(|(r, _)| (r[2] - r[0]) * (r[3] - r[1])).sum();
    assert!((area - 1.0).abs() < 1e-9, "the cells cover the unit domain exactly: {area}");
}
