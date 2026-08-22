//! Shading types 4 to 7 (ISO 32000-2, 8.7.4.5.5 to 8.7.4.5.8), decoded from bytes laid
//! out by hand against the clause.
//!
//! `target/mesh/` holds four fixtures compared against PDFKit, and they cover what those
//! can: geometry that lands in the right place with the right colours. They cover it with
//! every field a whole number of bytes, so the **padding rule** — "each set of vertex data
//! shall occupy a whole number of bytes" — is untested by them, as is every edge flag but
//! the two the fixtures use. Those are here.

use fepdf_model::graphics::{Color, TriangleMesh};
use fepdf_model::{Handle, Object, PdfArena, PdfName};
use std::collections::BTreeMap;

/// Writes fields at arbitrary bit widths, so a test can build a stream the padding rule
/// actually applies to.
struct Bits {
    data: Vec<u8>,
    bit: usize,
}

impl Bits {
    fn new() -> Self {
        Self { data: Vec::new(), bit: 0 }
    }

    fn push(&mut self, value: u64, width: u32) {
        for i in (0..width).rev() {
            if self.bit.is_multiple_of(8) {
                self.data.push(0);
            }
            let set = (value >> i) & 1 == 1;
            if set {
                let last = self.data.len() - 1;
                self.data[last] |= 1 << (7 - (self.bit % 8));
            }
            self.bit += 1;
        }
    }

    /// The end of a vertex or a patch: skip to the next byte boundary.
    fn align(&mut self) {
        while !self.bit.is_multiple_of(8) {
            self.push(0, 1);
        }
    }
}

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

fn name(arena: &PdfArena, n: &str) -> Object {
    Object::Name(arena.intern_name(PdfName::new(n)))
}

/// A shading dictionary whose `/Decode` makes an 8-bit coordinate equal its raw byte.
fn rgb_dict<'a>(arena: &PdfArena, extra: Vec<(&'a str, Object)>) -> Vec<(&'a str, Object)> {
    let mut base = vec![
        ("ColorSpace", name(arena, "DeviceRGB")),
        ("BitsPerCoordinate", Object::Integer(8)),
        ("BitsPerComponent", Object::Integer(8)),
        ("BitsPerFlag", Object::Integer(8)),
        ("Decode", nums(arena, &[0.0, 255.0, 0.0, 255.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0])),
    ];
    base.extend(extra);
    base
}

fn vertex(out: &mut Bits, flag: Option<u64>, x: u64, y: u64, rgb: [u64; 3]) {
    if let Some(f) = flag {
        out.push(f, 8);
    }
    out.push(x, 8);
    out.push(y, 8);
    for c in rgb {
        out.push(c, 8);
    }
    out.align();
}

const RED: [u64; 3] = [255, 0, 0];
const GREEN: [u64; 3] = [0, 255, 0];
const BLUE: [u64; 3] = [0, 0, 255];
const WHITE: [u64; 3] = [255, 255, 255];

fn parse(
    arena: &PdfArena,
    shading_type: i64,
    entries: Vec<(&str, Object)>,
    data: &[u8],
) -> TriangleMesh {
    let dh = dict(arena, entries);
    let d = arena.get_dict(dh).expect("dictionary");
    TriangleMesh::parse(shading_type, &d, data, arena).expect("the mesh decodes")
}

fn approx(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-6_f64 && (a.1 - b.1).abs() < 1e-6_f64
}

#[test]
fn type4_flag_one_continues_on_the_far_edge() {
    let arena = PdfArena::new();
    let mut bits = Bits::new();
    vertex(&mut bits, Some(0), 0, 0, RED);
    vertex(&mut bits, Some(0), 10, 0, GREEN);
    vertex(&mut bits, Some(0), 0, 10, BLUE);
    // 8.7.4.5.5: "If the edge flag is fd = 1 (side vbc), the next vertex forms the
    // triangle (vb, vc, vd)."
    vertex(&mut bits, Some(1), 10, 10, WHITE);

    let mesh = parse(&arena, 4, rgb_dict(&arena, vec![]), &bits.data);
    assert_eq!(mesh.triangles.len(), 2);
    assert!(approx(mesh.triangles[0].points[0], (0.0, 0.0)));
    assert_eq!(mesh.triangles[0].colors[0], Color::Rgb(1.0, 0.0, 0.0));
    assert!(approx(mesh.triangles[1].points[0], (10.0, 0.0)), "vb starts the second");
    assert!(approx(mesh.triangles[1].points[1], (0.0, 10.0)), "vc follows it");
    assert!(approx(mesh.triangles[1].points[2], (10.0, 10.0)), "vd closes it");
}

#[test]
fn type4_flag_two_continues_on_the_other_edge() {
    let arena = PdfArena::new();
    let mut bits = Bits::new();
    vertex(&mut bits, Some(0), 0, 0, RED);
    vertex(&mut bits, Some(0), 10, 0, GREEN);
    vertex(&mut bits, Some(0), 0, 10, BLUE);
    // "if the edge flag is fd = 2 (side vac), the next vertex forms the triangle
    // (va, vc, vd)" — the distinction flag 1 cannot show.
    vertex(&mut bits, Some(2), 10, 10, WHITE);

    let mesh = parse(&arena, 4, rgb_dict(&arena, vec![]), &bits.data);
    assert_eq!(mesh.triangles.len(), 2);
    assert!(approx(mesh.triangles[1].points[0], (0.0, 0.0)), "va, not vb");
    assert!(approx(mesh.triangles[1].points[1], (0.0, 10.0)));
    assert_eq!(mesh.triangles[1].colors[0], Color::Rgb(1.0, 0.0, 0.0), "va's colour too");
}

#[test]
fn type4_pads_each_vertex_to_a_whole_number_of_bytes() {
    let arena = PdfArena::new();
    // 2-bit flag + two 8-bit coordinates + one 8-bit grey = 26 bits, which is where the
    // rule bites: without the pad to 32 the second vertex starts six bits early and
    // every field after it is rubbish.
    let entries = vec![
        ("ColorSpace", name(&arena, "DeviceGray")),
        ("BitsPerCoordinate", Object::Integer(8)),
        ("BitsPerComponent", Object::Integer(8)),
        ("BitsPerFlag", Object::Integer(2)),
        ("Decode", nums(&arena, &[0.0, 255.0, 0.0, 255.0, 0.0, 1.0])),
    ];
    let mut bits = Bits::new();
    for (flag, x, y, g) in [(0_u64, 0_u64, 0_u64, 0_u64), (0, 20, 0, 128), (0, 0, 20, 255)] {
        bits.push(flag, 2);
        bits.push(x, 8);
        bits.push(y, 8);
        bits.push(g, 8);
        bits.align();
    }
    assert_eq!(bits.data.len(), 12, "three vertices of four padded bytes");

    let mesh = parse(&arena, 4, entries, &bits.data);
    assert_eq!(mesh.triangles.len(), 1);
    assert!(approx(mesh.triangles[0].points[1], (20.0, 0.0)));
    assert!(approx(mesh.triangles[0].points[2], (0.0, 20.0)));
    assert_eq!(mesh.triangles[0].colors[2], Color::Gray(1.0));
}

#[test]
fn type5_meshes_a_lattice_row_by_row() {
    let arena = PdfArena::new();
    let mut bits = Bits::new();
    // Three columns, two rows: four cells' worth of corners, so two rows give
    // 2 × (3 − 1) = 4 triangles.
    for (y, row) in [(0_u64, [RED, GREEN, BLUE]), (10, [WHITE, WHITE, WHITE])] {
        for (i, rgb) in row.into_iter().enumerate() {
            vertex(&mut bits, None, u64::try_from(i).unwrap() * 10, y, rgb);
        }
    }
    let entries = rgb_dict(&arena, vec![("VerticesPerRow", Object::Integer(3))]);
    let mesh = parse(&arena, 5, entries, &bits.data);
    assert_eq!(mesh.triangles.len(), 4);
    assert!(approx(mesh.triangles[0].points[0], (0.0, 0.0)));
    assert!(approx(mesh.triangles[0].points[2], (0.0, 10.0)), "the cell reaches the next row");
}

/// The twelve boundary control points of a flat square patch, in the spiral order
/// 8.7.4.5.8 gives: `p00 p01 p02 p03 p13 p23 p33 p32 p31 p30 p20 p10`.
fn square_boundary() -> [(u64, u64); 12] {
    [
        (0, 0),
        (0, 30),
        (0, 60),
        (0, 90),
        (30, 90),
        (60, 90),
        (90, 90),
        (90, 60),
        (90, 30),
        (90, 0),
        (60, 0),
        (30, 0),
    ]
}

fn patch_bytes(interior: Option<[(u64, u64); 4]>) -> Vec<u8> {
    let mut bits = Bits::new();
    bits.push(0, 8);
    for (x, y) in square_boundary() {
        bits.push(x, 8);
        bits.push(y, 8);
    }
    for (x, y) in interior.into_iter().flatten() {
        bits.push(x, 8);
        bits.push(y, 8);
    }
    for rgb in [RED, GREEN, BLUE, WHITE] {
        for c in rgb {
            bits.push(c, 8);
        }
    }
    bits.align();
    bits.data
}

#[test]
fn type6_maps_the_unit_square_onto_the_patch_corners() {
    let arena = PdfArena::new();
    let mesh = parse(&arena, 6, rgb_dict(&arena, vec![]), &patch_bytes(None));

    // The grid is sampled corner-first, so the very first triangle starts at p00 and the
    // last one ends at p33 — the two corners that pin the mapping's orientation.
    assert!(approx(mesh.triangles[0].points[0], (0.0, 0.0)), "u=0,v=0 is p00");
    assert_eq!(mesh.triangles[0].colors[0], Color::Rgb(1.0, 0.0, 0.0), "c1 sits at p00");
    let last = mesh.triangles.last().expect("triangles");
    assert!(approx(last.points[1], (90.0, 90.0)), "u=1,v=1 is p33");
    assert_eq!(last.colors[1], Color::Rgb(0.0, 0.0, 1.0), "c3 sits at p33");
}

#[test]
fn a_coons_patch_is_a_tensor_patch_with_its_interior_computed() {
    let arena = PdfArena::new();
    let coons = parse(&arena, 6, rgb_dict(&arena, vec![]), &patch_bytes(None));
    // 8.7.4.5.8's equations, for a flat square, put the four interior points on the
    // thirds. Giving type 7 exactly those must reproduce type 6 point for point — which
    // is the only check that the equations were transcribed right, since a wrong
    // interior still produces a plausible surface.
    let interior = [(30, 30), (30, 60), (60, 60), (60, 30)];
    let tensor = parse(&arena, 7, rgb_dict(&arena, vec![]), &patch_bytes(Some(interior)));

    assert_eq!(coons.triangles.len(), tensor.triangles.len());
    for (a, b) in coons.triangles.iter().zip(&tensor.triangles) {
        for i in 0..3 {
            assert!(approx(a.points[i], b.points[i]), "{:?} vs {:?}", a.points, b.points);
        }
    }
}

#[test]
fn a_function_turns_one_parametric_value_into_a_colour() {
    let arena = PdfArena::new();
    // Table 81: with `/Function` present the vertex carries a single value t, and
    // `/Decode` then holds only one pair for it.
    let function = Object::Dictionary(dict(
        &arena,
        vec![
            ("FunctionType", Object::Integer(2)),
            ("Domain", nums(&arena, &[0.0, 1.0])),
            ("C0", nums(&arena, &[1.0, 0.0, 0.0])),
            ("C1", nums(&arena, &[0.0, 0.0, 1.0])),
            ("N", Object::Integer(1)),
        ],
    ));
    let entries = vec![
        ("ColorSpace", name(&arena, "DeviceRGB")),
        ("BitsPerCoordinate", Object::Integer(8)),
        ("BitsPerComponent", Object::Integer(8)),
        ("BitsPerFlag", Object::Integer(8)),
        ("Decode", nums(&arena, &[0.0, 255.0, 0.0, 255.0, 0.0, 1.0])),
        ("Function", function),
    ];
    let mut bits = Bits::new();
    for (x, y, t) in [(0_u64, 0_u64, 0_u64), (10, 0, 128), (0, 10, 255)] {
        bits.push(0, 8);
        bits.push(x, 8);
        bits.push(y, 8);
        bits.push(t, 8);
        bits.align();
    }
    let mesh = parse(&arena, 4, entries, &bits.data);
    assert_eq!(mesh.triangles[0].colors[0], Color::Rgb(1.0, 0.0, 0.0), "t=0 is C0");
    assert_eq!(mesh.triangles[0].colors[2], Color::Rgb(0.0, 0.0, 1.0), "t=1 is C1");
}

#[test]
fn flattening_keeps_a_constant_triangle_whole() {
    let arena = PdfArena::new();
    let mut bits = Bits::new();
    for (x, y) in [(0_u64, 0_u64), (90, 0), (0, 90)] {
        vertex(&mut bits, Some(0), x, y, RED);
    }
    let mesh = parse(&arena, 4, rgb_dict(&arena, vec![]), &bits.data);
    // Every corner is the same colour, so there is nothing to interpolate and no reason
    // to split: one triangle in, one out.
    let flat = mesh.flatten();
    assert_eq!(flat.len(), 1);
    assert_eq!(flat[0].color, Color::Rgb(1.0, 0.0, 0.0));
}

#[test]
fn flattening_splits_a_triangle_that_changes_colour() {
    let arena = PdfArena::new();
    let mut bits = Bits::new();
    vertex(&mut bits, Some(0), 0, 0, [0, 0, 0]);
    vertex(&mut bits, Some(0), 90, 0, WHITE);
    vertex(&mut bits, Some(0), 0, 90, [0, 0, 0]);
    let mesh = parse(&arena, 4, rgb_dict(&arena, vec![]), &bits.data);
    let flat = mesh.flatten();
    assert!(flat.len() > 1, "black to white cannot be one flat triangle");
    // Bounded: the split depth is capped, which is what stops a pathological mesh.
    assert!(flat.len() <= 256, "got {}", flat.len());
}
