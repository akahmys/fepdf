//! Type 0, 2, 3 and 4 functions (ISO 32000-2, 7.10), evaluated against values worked out
//! from the clause rather than from this engine's output.
//!
//! The two functions in `crates/fepdf-model/examples/make_colour_fixtures.rs` are here by
//! name: they are the ones `crosscheck_image.sh` measures against PDFKit, and a unit test
//! that agrees with the cross-check tells you the evaluator is right rather than that two
//! halves of this engine agree with each other.

use fepdf_model::function::{FunctionSet, PdfFunction};
use fepdf_model::object::SublimatedData;
use fepdf_model::{Handle, Object, PdfArena, PdfName};
use std::collections::BTreeMap;
use std::sync::Arc;

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

fn ints(arena: &PdfArena, values: &[i64]) -> Object {
    Object::Array(arena.alloc_array(values.iter().map(|v| Object::Integer(*v)).collect()))
}

fn close(actual: &[f64], expected: &[f64]) -> bool {
    actual.len() == expected.len()
        && actual.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-6_f64)
}

fn eval(function: &PdfFunction, x: f64) -> Vec<f64> {
    function.eval(&[x]).unwrap_or_else(|| panic!("function did not evaluate at {x}"))
}

/// The tint transform of `target/colour/separation.pdf`: `/DeviceCMYK` black at full tint.
fn separation_transform(arena: &PdfArena) -> Object {
    Object::Dictionary(dict(
        arena,
        vec![
            ("FunctionType", Object::Integer(2)),
            ("Domain", nums(arena, &[0.0, 1.0])),
            ("C0", nums(arena, &[0.0, 0.0, 0.0, 0.0])),
            ("C1", nums(arena, &[0.0, 0.0, 0.0, 1.0])),
            ("N", Object::Integer(1)),
        ],
    ))
}

#[test]
fn type2_carries_a_separation_tint_to_full_ink() {
    let arena = PdfArena::new();
    let obj = separation_transform(&arena);
    let f = PdfFunction::parse(&obj, &arena).expect("type 2 parses");

    // The defect this fixture was built for: tint 1.0 is full ink, and reading the tint
    // as a grey level made it white. The evaluator has to reach `k = 1`.
    assert!(close(&eval(&f, 1.0), &[0.0, 0.0, 0.0, 1.0]));
    assert!(close(&eval(&f, 0.0), &[0.0, 0.0, 0.0, 0.0]));
    assert!(close(&eval(&f, 0.5), &[0.0, 0.0, 0.0, 0.5]));
}

#[test]
fn type2_applies_the_exponent_and_clips_the_domain() {
    let arena = PdfArena::new();
    let obj = Object::Dictionary(dict(
        &arena,
        vec![
            ("FunctionType", Object::Integer(2)),
            ("Domain", nums(&arena, &[0.0, 1.0])),
            ("C0", nums(&arena, &[0.0])),
            ("C1", nums(&arena, &[1.0])),
            ("N", Object::Integer(2)),
        ],
    ));
    let f = PdfFunction::parse(&obj, &arena).expect("type 2 parses");
    assert!(close(&eval(&f, 0.5), &[0.25]));
    // 7.10: inputs are clipped to `/Domain` before evaluation, not rejected.
    assert!(close(&eval(&f, 4.0), &[1.0]));
    assert!(close(&eval(&f, -3.0), &[0.0]));
}

#[test]
fn type2_defaults_c0_and_c1_to_the_identity() {
    let arena = PdfArena::new();
    let obj = Object::Dictionary(dict(
        &arena,
        vec![
            ("FunctionType", Object::Integer(2)),
            ("Domain", nums(&arena, &[0.0, 1.0])),
            ("N", Object::Integer(1)),
        ],
    ));
    let f = PdfFunction::parse(&obj, &arena).expect("type 2 parses without /C0 and /C1");
    assert!(close(&eval(&f, 0.25), &[0.25]));
}

/// The stitching function of `target/colour/gradient.pdf`: red → green → blue.
#[test]
fn type3_stitches_two_subdomains() {
    let arena = PdfArena::new();
    let leg = |c0: &[f64], c1: &[f64]| {
        Object::Dictionary(dict(
            &arena,
            vec![
                ("FunctionType", Object::Integer(2)),
                ("Domain", nums(&arena, &[0.0, 1.0])),
                ("C0", nums(&arena, c0)),
                ("C1", nums(&arena, c1)),
                ("N", Object::Integer(1)),
            ],
        ))
    };
    let legs = Object::Array(arena.alloc_array(vec![
        leg(&[1.0, 0.0, 0.0], &[0.0, 1.0, 0.0]),
        leg(&[0.0, 1.0, 0.0], &[0.0, 0.0, 1.0]),
    ]));
    let obj = Object::Dictionary(dict(
        &arena,
        vec![
            ("FunctionType", Object::Integer(3)),
            ("Domain", nums(&arena, &[0.0, 1.0])),
            ("Bounds", nums(&arena, &[0.5])),
            ("Encode", nums(&arena, &[0.0, 1.0, 0.0, 1.0])),
            ("Functions", legs),
        ],
    ));
    let f = PdfFunction::parse(&obj, &arena).expect("type 3 parses");

    assert!(close(&eval(&f, 0.0), &[1.0, 0.0, 0.0]), "starts red");
    assert!(close(&eval(&f, 0.25), &[0.5, 0.5, 0.0]), "half way through the first leg");
    // 7.10.4 makes each subinterval half-open at the top, so 0.5 belongs to the *second*
    // function — where it encodes to 0.0 and gives green either way. The test that tells
    // the two apart is 0.75.
    assert!(close(&eval(&f, 0.5), &[0.0, 1.0, 0.0]), "green at the seam");
    assert!(close(&eval(&f, 0.75), &[0.0, 0.5, 0.5]), "half way through the second leg");
    assert!(close(&eval(&f, 1.0), &[0.0, 0.0, 1.0]), "ends blue");
}

#[test]
fn type3_honours_a_reversing_encode() {
    let arena = PdfArena::new();
    let leg = Object::Dictionary(dict(
        &arena,
        vec![
            ("FunctionType", Object::Integer(2)),
            ("Domain", nums(&arena, &[0.0, 1.0])),
            ("C0", nums(&arena, &[0.0])),
            ("C1", nums(&arena, &[1.0])),
            ("N", Object::Integer(1)),
        ],
    ));
    let obj = Object::Dictionary(dict(
        &arena,
        vec![
            ("FunctionType", Object::Integer(3)),
            ("Domain", nums(&arena, &[0.0, 1.0])),
            ("Bounds", nums(&arena, &[])),
            // `/Encode [1 0]` runs the single subfunction backwards, which is how a
            // reversed gradient is written without a second function.
            ("Encode", nums(&arena, &[1.0, 0.0])),
            ("Functions", Object::Array(arena.alloc_array(vec![leg]))),
        ],
    ));
    let f = PdfFunction::parse(&obj, &arena).expect("type 3 parses with no /Bounds");
    assert!(close(&eval(&f, 0.0), &[1.0]));
    assert!(close(&eval(&f, 1.0), &[0.0]));
}

fn sampled(arena: &PdfArena, bits: i64, size: &[i64], bytes: Vec<u8>) -> Object {
    let dh = dict(
        arena,
        vec![
            ("FunctionType", Object::Integer(0)),
            ("Domain", nums(arena, &[0.0, 1.0])),
            ("Range", nums(arena, &[0.0, 1.0])),
            ("Size", ints(arena, size)),
            ("BitsPerSample", Object::Integer(bits)),
        ],
    );
    Object::Stream(dh, Arc::new(SublimatedData::Raw(bytes::Bytes::from(bytes))))
}

#[test]
fn type0_interpolates_between_eight_bit_samples() {
    let arena = PdfArena::new();
    let obj = sampled(&arena, 8, &[2], vec![0x00, 0xFF]);
    let f = PdfFunction::parse(&obj, &arena).expect("type 0 parses");
    assert!(close(&eval(&f, 0.0), &[0.0]));
    assert!(close(&eval(&f, 1.0), &[1.0]));
    assert!(close(&eval(&f, 0.5), &[0.5]), "multilinear between the two samples");
    assert!(close(&eval(&f, 0.25), &[0.25]));
}

#[test]
fn type0_reads_samples_narrower_than_a_byte() {
    let arena = PdfArena::new();
    // Three 4-bit samples — 0, 8, 15 — which straddle the byte boundary the naive
    // reader gets wrong: nibbles 0x0 0x8 then 0xF in the high half of the second byte.
    let obj = sampled(&arena, 4, &[3], vec![0x08, 0xF0]);
    let f = PdfFunction::parse(&obj, &arena).expect("type 0 parses at 4 bits");
    assert!(close(&eval(&f, 0.0), &[0.0]));
    assert!(close(&eval(&f, 0.5), &[8.0 / 15.0]));
    assert!(close(&eval(&f, 1.0), &[1.0]));
}

#[test]
fn type0_refuses_a_stream_shorter_than_its_size() {
    let arena = PdfArena::new();
    // `/Size [4]` over one byte of samples: the last grid point is off the end. This has
    // to be a refusal rather than a zero, because a zero is a colour.
    let obj = sampled(&arena, 8, &[4], vec![0x00]);
    let f = PdfFunction::parse(&obj, &arena).expect("type 0 parses");
    assert_eq!(f.eval(&[1.0]), None);
}

fn calculator(arena: &PdfArena, range: &[f64], program: &str) -> Object {
    let dh = dict(
        arena,
        vec![
            ("FunctionType", Object::Integer(4)),
            ("Domain", nums(arena, &[0.0, 1.0])),
            ("Range", nums(arena, range)),
        ],
    );
    Object::Stream(dh, Arc::new(SublimatedData::Raw(bytes::Bytes::from(program.to_string()))))
}

#[test]
fn type4_runs_a_calculator_program() {
    let arena = PdfArena::new();
    let obj = calculator(&arena, &[0.0, 1.0], "{ 1 exch sub }");
    let f = PdfFunction::parse(&obj, &arena).expect("type 4 parses");
    assert!(close(&eval(&f, 0.3), &[0.7]));
    assert!(close(&eval(&f, 0.0), &[1.0]));
}

#[test]
fn type4_takes_both_branches_of_ifelse() {
    let arena = PdfArena::new();
    let obj = calculator(&arena, &[0.0, 1.0], "{ dup 0.5 lt { pop 0 } { pop 1 } ifelse }");
    let f = PdfFunction::parse(&obj, &arena).expect("type 4 parses");
    assert!(close(&eval(&f, 0.2), &[0.0]));
    assert!(close(&eval(&f, 0.8), &[1.0]));
}

#[test]
fn type4_expands_one_tint_into_four_components() {
    let arena = PdfArena::new();
    // The shape a `/Separation` over `/DeviceCMYK` is written in when the transform is
    // not a simple exponential: one input, four outputs, the tint landing in K.
    //
    // `4 3 roll` on `[tint, 0, 0, 0]` rolls the top four up by three, which moves the
    // tint to the *top* of the stack — and the top of the stack is the **last** output,
    // not the first. Getting that backwards is the easy mistake here, and this test was
    // written asserting the wrong end before the evaluator corrected it.
    let obj = calculator(&arena, &[0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0], "{ 0 0 0 4 3 roll }");
    let f = PdfFunction::parse(&obj, &arena).expect("type 4 parses");
    assert!(close(&eval(&f, 0.6), &[0.0, 0.0, 0.0, 0.6]));
}

#[test]
fn type4_handles_comments_and_stack_operators() {
    let arena = PdfArena::new();
    let obj = calculator(&arena, &[0.0, 1.0], "{ % a comment\n dup 1 index sub abs }");
    let f = PdfFunction::parse(&obj, &arena).expect("type 4 parses past a comment");
    assert!(close(&eval(&f, 0.4), &[0.0]));
}

#[test]
fn type4_refuses_rather_than_guessing() {
    let arena = PdfArena::new();
    // Stack underflow. Skipping the offending operator would leave the stack the wrong
    // depth and produce a plausible number from nonsense, so evaluation fails and the
    // caller falls back to something it can name.
    let underflow = calculator(&arena, &[0.0, 1.0], "{ pop pop pop }");
    let f = PdfFunction::parse(&underflow, &arena).expect("type 4 parses");
    assert_eq!(f.eval(&[0.5]), None);

    // An operator Table 42 does not define.
    let unknown = calculator(&arena, &[0.0, 1.0], "{ 2 setflatness }");
    let g = PdfFunction::parse(&unknown, &arena).expect("type 4 parses");
    assert_eq!(g.eval(&[0.5]), None);
}

#[test]
fn an_array_of_functions_concatenates_its_outputs() {
    let arena = PdfArena::new();
    let channel = |c0: f64, c1: f64| {
        Object::Dictionary(dict(
            &arena,
            vec![
                ("FunctionType", Object::Integer(2)),
                ("Domain", nums(&arena, &[0.0, 1.0])),
                ("C0", nums(&arena, &[c0])),
                ("C1", nums(&arena, &[c1])),
                ("N", Object::Integer(1)),
            ],
        ))
    };
    // 8.7.4.5.2: a shading's `/Function` may be n one-output functions instead of one
    // n-output function. Both forms occur and a reader that handles one paints the
    // other black.
    let array = Object::Array(arena.alloc_array(vec![
        channel(1.0, 0.0),
        channel(0.0, 1.0),
        channel(0.0, 0.0),
    ]));
    let set = FunctionSet::parse(&array, &arena).expect("an array of functions parses");
    let out = set.eval(&[0.25]).expect("evaluates");
    assert!(close(&out, &[0.75, 0.25, 0.0]));

    let single = FunctionSet::parse(&channel(0.0, 1.0), &arena).expect("a lone function parses");
    assert!(close(&single.eval(&[0.5]).expect("evaluates"), &[0.5]));
}

#[test]
fn an_unknown_function_type_is_not_parsed() {
    let arena = PdfArena::new();
    // Type 1 was withdrawn and type 5 does not exist. Returning `None` is what lets the
    // caller record a `Decision` naming the type rather than painting a guess.
    let obj = Object::Dictionary(dict(
        &arena,
        vec![("FunctionType", Object::Integer(5)), ("Domain", nums(&arena, &[0.0, 1.0]))],
    ));
    assert!(PdfFunction::parse(&obj, &arena).is_none());
}
