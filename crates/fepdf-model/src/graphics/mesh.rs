//! Shading types 4 to 7 (ISO 32000-2, 8.7.4.5.5 to 8.7.4.5.8), decoded to triangles.
//!
//! Four shading types, one output. A free-form mesh (4) and a lattice (5) are already
//! triangles; a Coons patch (6) and a tensor-product patch (7) are bicubic surfaces, and
//! 8.7.4.5.8 says outright that "the Coons patch is actually a special case of the
//! tensor-product patch" with its four internal control points implied by the boundary.
//! So type 6 becomes a type 7 with four computed points, the surface is evaluated on a
//! grid, and every type ends up as triangles with a colour at each corner.
//!
//! **The renderer cannot Gouraud-shade.** Vello fills a path with one brush, so a
//! triangle with three different corner colours has no direct representation.
//! [`TriangleMesh::flatten`] subdivides until the colour across a triangle is close
//! enough to constant and hands back flat ones — the approximation is made here, where it
//! can be tested, rather than in the backend where it could not.
//!
//! Both halves of the vertex format were built already and are reused rather than
//! re-derived: `/Function` is a [`crate::function::FunctionSet`] (7.10) and `/ColorSpace`
//! a [`crate::color::ResolvedColorSpace`] (8.6), so a mesh over a `/Separation` runs its
//! tint transform per vertex without this module knowing what a separation is.

use crate::color::ResolvedColorSpace;
use crate::function::FunctionSet;
use crate::graphics::Color;
use crate::object::{Object, PdfName};
use crate::{Handle, PdfArena};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// How finely a patch surface is sampled, per side. 8.7.4.5.7 patches are bicubic, so a
/// grid this size reproduces the curvature well inside a pixel at ordinary page scales
/// and costs 2 × 10² triangles per patch.
const PATCH_GRID: usize = 10;

/// How far apart two corner colours may be, per channel in `[0, 1]`, before `flatten`
/// splits a triangle rather than painting it one colour.
const FLAT_TOLERANCE: f64 = 1.0 / 128.0;

/// How many times `flatten` may split. 4 gives at most 256 pieces per source triangle,
/// which bounds a pathological mesh (RR-15 Rule 6 in spirit: no unbounded recursion).
const MAX_SPLITS: u32 = 4;

/// A triangle with a colour at each corner, interpolated across the interior (8.7.4.5.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshTriangle {
    /// The three corners, in the shading's target coordinate space.
    pub points: [(f64, f64); 3],
    /// The colour at each corner, in the same order.
    pub colors: [Color; 3],
}

/// A shading of type 4, 5, 6 or 7 after decoding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriangleMesh {
    /// The triangles the stream described, in the order it described them.
    pub triangles: Vec<MeshTriangle>,
}

/// A triangle painted in one colour, which is what a backend can actually draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatTriangle {
    /// The three corners.
    pub points: [(f64, f64); 3],
    /// The single colour to fill it with.
    pub color: Color,
}

impl TriangleMesh {
    /// Decodes a type 4 to 7 shading stream.
    ///
    /// `data` is the stream already decoded by its filters; `dict` is the shading
    /// dictionary, which is also the stream dictionary for these types.
    pub fn parse(shading_type: i64, dict: &Dict, data: &[u8], arena: &PdfArena) -> Option<Self> {
        let params = MeshParams::parse(dict, arena)?;
        let mut reader = MeshReader::new(data);
        let triangles = match shading_type {
            4 => free_form(&mut reader, &params),
            5 => lattice(&mut reader, &params, integer(dict, arena, "VerticesPerRow")?),
            6 | 7 => patches(&mut reader, &params, shading_type == 7),
            _ => return None,
        };
        if triangles.is_empty() { None } else { Some(Self { triangles }) }
    }

    /// Subdivides until each piece is close enough to one colour, for backends that can
    /// only fill a path with a single brush.
    pub fn flatten(&self) -> Vec<FlatTriangle> {
        let mut out = Vec::new();
        for triangle in &self.triangles {
            split(triangle.points, triangle.colors, 0, &mut out);
        }
        out
    }
}

/// Recursively splits one triangle at its edge midpoints until its corners agree.
fn split(points: [(f64, f64); 3], colors: [Color; 3], depth: u32, out: &mut Vec<FlatTriangle>) {
    if depth >= MAX_SPLITS || corners_agree(&colors) {
        out.push(FlatTriangle { points, color: mean_color(&colors) });
        return;
    }
    let mid = |a: (f64, f64), b: (f64, f64)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let (m01, m12, m20) =
        (mid(points[0], points[1]), mid(points[1], points[2]), mid(points[2], points[0]));
    let (c01, c12, c20) = (
        blend(colors[0], colors[1], 0.5),
        blend(colors[1], colors[2], 0.5),
        blend(colors[2], colors[0], 0.5),
    );
    let next = depth + 1;
    split([points[0], m01, m20], [colors[0], c01, c20], next, out);
    split([m01, points[1], m12], [c01, colors[1], c12], next, out);
    split([m20, m12, points[2]], [c20, c12, colors[2]], next, out);
    split([m01, m12, m20], [c01, c12, c20], next, out);
}

fn corners_agree(colors: &[Color; 3]) -> bool {
    let [a, b, c] = [rgb_parts(colors[0]), rgb_parts(colors[1]), rgb_parts(colors[2])];
    (0..3).all(|i| {
        let (lo, hi) = (a[i].min(b[i]).min(c[i]), a[i].max(b[i]).max(c[i]));
        hi - lo <= FLAT_TOLERANCE
    })
}

/// A colour as three sRGB channels, for interpolating and comparing.
///
/// `to_rgb` already maps every current variant onto `Rgb`, so only the first arm is
/// reachable. The rest are spelled out rather than wildcarded (RR-15 Rule 5) so that a
/// `Color` variant added later — one `to_rgb` does *not* convert — fails to compile here
/// instead of silently interpolating as black.
fn rgb_parts(color: Color) -> [f64; 3] {
    match color.to_rgb() {
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Gray(g) => [g, g, g],
        Color::Cmyk(c, m, y, k) => {
            let ink = |v: f64| (1.0 - v) * (1.0 - k);
            [ink(c), ink(m), ink(y)]
        }
        Color::Lab(l, _, _) => {
            let v = (l / 100.0).clamp(0.0, 1.0);
            [v, v, v]
        }
    }
}

fn blend(a: Color, b: Color, t: f64) -> Color {
    let (x, y) = (rgb_parts(a), rgb_parts(b));
    Color::Rgb(x[0] + (y[0] - x[0]) * t, x[1] + (y[1] - x[1]) * t, x[2] + (y[2] - x[2]) * t)
}

fn mean_color(colors: &[Color; 3]) -> Color {
    let parts: Vec<[f64; 3]> = colors.iter().map(|c| rgb_parts(*c)).collect();
    let mean = |i: usize| (parts[0][i] + parts[1][i] + parts[2][i]) / 3.0;
    Color::Rgb(mean(0), mean(1), mean(2))
}

/// Everything the vertex format needs, read once from the shading dictionary.
struct MeshParams {
    bits_coord: u32,
    bits_comp: u32,
    bits_flag: u32,
    decode: Vec<f64>,
    /// How many numbers follow each coordinate pair: 1 when `/Function` is present, and
    /// the colour space's component count otherwise (Table 81).
    inputs: usize,
    space: ResolvedColorSpace,
    function: Option<FunctionSet>,
}

impl MeshParams {
    fn parse(dict: &Dict, arena: &PdfArena) -> Option<Self> {
        let space_obj = entry(dict, arena, "ColorSpace")?;
        let space = ResolvedColorSpace::parse(&space_obj, arena)?;
        let function =
            entry(dict, arena, "Function").and_then(|obj| FunctionSet::parse(&obj, arena));
        let inputs = if function.is_some() { 1 } else { space.components };

        let bits_coord = u32::try_from(integer(dict, arena, "BitsPerCoordinate")?).ok()?;
        let bits_comp = u32::try_from(integer(dict, arena, "BitsPerComponent")?).ok()?;
        // Required for types 4, 6 and 7 and absent for 5, which has no flags. Reading it
        // as 0 lets one reader serve all four: a zero-bit field consumes nothing.
        let bits_flag =
            integer(dict, arena, "BitsPerFlag").and_then(|v| u32::try_from(v).ok()).unwrap_or(0);
        if !matches!(bits_coord, 1 | 2 | 4 | 8 | 12 | 16 | 24 | 32)
            || !matches!(bits_comp, 1 | 2 | 4 | 8 | 12 | 16)
        {
            return None;
        }

        let decode = number_array(dict, arena, "Decode")?;
        if decode.len() < 4 + 2 * inputs {
            return None;
        }
        Some(Self { bits_coord, bits_comp, bits_flag, decode, inputs, space, function })
    }

    /// Maps a raw field onto the interval `/Decode` gives it, exactly as an image's
    /// `/Decode` does (8.9.5.2).
    fn decode_at(&self, pair: usize, raw: u64, bits: u32) -> f64 {
        let max = if bits >= 63 { u64::MAX } else { (1_u64 << bits) - 1 };
        let (lo, hi) = (self.decode[2 * pair], self.decode[2 * pair + 1]);
        let fraction = ratio(raw, max);
        lo + fraction * (hi - lo)
    }

    /// Reads one vertex's coordinates and colour, without the edge flag.
    fn read_vertex(&self, reader: &mut MeshReader) -> Option<((f64, f64), Color)> {
        let x = self.decode_at(0, reader.read(self.bits_coord)?, self.bits_coord);
        let y = self.decode_at(1, reader.read(self.bits_coord)?, self.bits_coord);
        Some(((x, y), self.read_color(reader)?))
    }

    fn read_color(&self, reader: &mut MeshReader) -> Option<Color> {
        let mut values = Vec::with_capacity(self.inputs);
        for i in 0..self.inputs {
            let raw = reader.read(self.bits_comp)?;
            values.push(self.decode_at(2 + i, raw, self.bits_comp));
        }
        match &self.function {
            // Table 81: with a `/Function` the vertex carries one parametric value, and
            // the colour is what the function makes of it.
            Some(f) => self.space.to_color(&f.eval(&values)?),
            None => self.space.to_color(&values),
        }
    }
}

/// A bit-level cursor over the mesh stream.
struct MeshReader<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> MeshReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    fn read(&mut self, bits: u32) -> Option<u64> {
        let mut value = 0_u64;
        for _ in 0..bits {
            let byte = *self.data.get(self.bit / 8)?;
            let shift = 7 - u32::try_from(self.bit % 8).ok()?;
            value = (value << 1) | u64::from((byte >> shift) & 1);
            self.bit += 1;
        }
        Some(value)
    }

    /// Skips to the next byte boundary. 8.7.4.5.5: "Each set of vertex data shall occupy
    /// a whole number of bytes … padded at the end with extra bits, which shall be
    /// ignored." The unit is the vertex for types 4 and 5 and the patch for 6 and 7.
    fn align(&mut self) {
        self.bit = self.bit.div_ceil(8) * 8;
    }

    fn spent(&self) -> bool {
        self.bit / 8 >= self.data.len()
    }
}

/// `u64` to a `[0, 1]` fraction without a lossy-cast lint. Mesh fields are at most 32
/// bits, which `f64` holds exactly.
fn ratio(value: u64, max: u64) -> f64 {
    let to_f64 = |v: u64| u32::try_from(v).map_or(f64::from(u32::MAX), f64::from);
    let max = to_f64(max);
    if max == 0.0 { 0.0 } else { to_f64(value) / max }
}

fn entry(dict: &Dict, arena: &PdfArena, key: &str) -> Option<Object> {
    let k = arena.intern_name(PdfName::new(key));
    dict.get(&k).map(|o| o.resolve(arena))
}

fn integer(dict: &Dict, arena: &PdfArena, key: &str) -> Option<i64> {
    entry(dict, arena, key)?.as_integer()
}

fn number_array(dict: &Dict, arena: &PdfArena, key: &str) -> Option<Vec<f64>> {
    let Object::Array(ah) = entry(dict, arena, key)? else {
        return None;
    };
    let items = arena.get_array(ah)?;
    let mut out = Vec::with_capacity(items.len());
    for item in &items {
        out.push(item.resolve(arena).as_f64()?);
    }
    Some(out)
}

/// Type 4 (8.7.4.5.5): each vertex carries an edge flag saying how it joins the last two.
///
/// Flag 0 starts a triangle and the two vertices after it have their flags ignored;
/// flag 1 continues on side `vbc` giving `(vb, vc, vd)`, and flag 2 on side `vac` giving
/// `(va, vc, vd)`.
fn free_form(reader: &mut MeshReader, params: &MeshParams) -> Vec<MeshTriangle> {
    let mut triangles = Vec::new();
    let mut previous: Option<[((f64, f64), Color); 3]> = None;
    while !reader.spent() {
        let Some(flag) = reader.read(params.bits_flag) else { break };
        let Some(vertex) = params.read_vertex(reader) else { break };
        reader.align();

        let corners = match (flag & 3, previous) {
            (1, Some([_a, b, c])) => Some([b, c, vertex]),
            (2, Some([a, _b, c])) => Some([a, c, vertex]),
            // Flag 0, or a continuation with nothing to continue from: two more vertices
            // follow and their own flags are ignored.
            _ => start_triangle(reader, params, vertex),
        };
        let Some(corners) = corners else { break };
        triangles.push(to_triangle(&corners));
        previous = Some(corners);
    }
    triangles
}

/// Reads the two vertices that complete a new triangle, discarding their edge flags.
fn start_triangle(
    reader: &mut MeshReader,
    params: &MeshParams,
    first: ((f64, f64), Color),
) -> Option<[((f64, f64), Color); 3]> {
    let mut rest = [first; 3];
    for slot in rest.iter_mut().skip(1) {
        reader.read(params.bits_flag)?;
        *slot = params.read_vertex(reader)?;
        reader.align();
    }
    Some(rest)
}

/// Type 5 (8.7.4.5.6): no flags, vertices in rows of `per_row`, each pair of adjacent
/// rows meshed into two triangles per cell.
fn lattice(reader: &mut MeshReader, params: &MeshParams, per_row: i64) -> Vec<MeshTriangle> {
    let Ok(per_row) = usize::try_from(per_row) else {
        return Vec::new();
    };
    if per_row < 2 {
        return Vec::new();
    }
    let mut rows: Vec<Vec<((f64, f64), Color)>> = Vec::new();
    while !reader.spent() {
        let mut row = Vec::with_capacity(per_row);
        for _ in 0..per_row {
            let Some(vertex) = params.read_vertex(reader) else { break };
            reader.align();
            row.push(vertex);
        }
        if row.len() < per_row {
            break;
        }
        rows.push(row);
    }

    let mut triangles = Vec::new();
    for pair in rows.windows(2) {
        for i in 0..per_row - 1 {
            let (a, b, c, d) = (pair[0][i], pair[0][i + 1], pair[1][i], pair[1][i + 1]);
            triangles.push(to_triangle(&[a, b, c]));
            triangles.push(to_triangle(&[b, d, c]));
        }
    }
    triangles
}

fn to_triangle(corners: &[((f64, f64), Color); 3]) -> MeshTriangle {
    MeshTriangle {
        points: [corners[0].0, corners[1].0, corners[2].0],
        colors: [corners[0].1, corners[1].1, corners[2].1],
    }
}

/// Where each control point sits in the 4×4 array, by its position in the stream.
///
/// 8.7.4.5.8 gives the stream order as a picture rather than a list — a spiral round the
/// boundary and then inward:
///
/// ```text
///    4  5  6  7          p03 p13 p23 p33
///    3 14 15  8    is    p02 p12 p22 p32
///    2 13 16  9          p01 p11 p21 p31
///    1 12 11 10          p00 p10 p20 p30
/// ```
///
/// so stream position 1 is `p00`, position 5 is `p13`, and the last four are the interior
/// a Coons patch computes instead of reading.
const STREAM_ORDER: [(usize, usize); 16] = [
    (0, 0),
    (0, 1),
    (0, 2),
    (0, 3),
    (1, 3),
    (2, 3),
    (3, 3),
    (3, 2),
    (3, 1),
    (3, 0),
    (2, 0),
    (1, 0),
    (1, 1),
    (1, 2),
    (2, 2),
    (2, 1),
];

/// The stream positions a continuation patch inherits, for edge flags 1, 2 and 3
/// (Table 84): the new patch's positions 1 to 4 are the previous patch's shared edge.
const INHERITED: [[usize; 4]; 3] = [[4, 5, 6, 7], [7, 8, 9, 10], [10, 11, 12, 1]];

/// One patch's control net and corner colours, kept so a continuation can inherit the
/// edge it shares with the patch before it.
#[derive(Clone, Copy)]
struct Patch {
    /// The 4×4 control net, indexed `[i][j]` as `p_ij` in Figure 47.
    points: [[(f64, f64); 4]; 4],
    /// The corner colours `c1` to `c4`, at `p00`, `p03`, `p33` and `p30`.
    colors: [Color; 4],
}

/// Types 6 and 7 (8.7.4.5.7, 8.7.4.5.8).
fn patches(reader: &mut MeshReader, params: &MeshParams, tensor: bool) -> Vec<MeshTriangle> {
    let total = if tensor { 16 } else { 12 };
    let mut triangles = Vec::new();
    let mut previous: Option<Patch> = None;

    while !reader.spent() {
        let Some(flag) = reader.read(params.bits_flag) else { break };
        let edge = usize::try_from(flag & 3).unwrap_or(0);
        let Some(mut patch) = read_patch(reader, params, total, edge, previous.as_ref()) else {
            break;
        };
        reader.align();
        if !tensor {
            fill_interior(&mut patch.points);
        }
        emit_patch(&patch, &mut triangles);
        previous = Some(patch);
    }
    triangles
}

/// Reads one patch, taking the shared edge from the previous one when the flag says to.
fn read_patch(
    reader: &mut MeshReader,
    params: &MeshParams,
    total: usize,
    edge: usize,
    previous: Option<&Patch>,
) -> Option<Patch> {
    let mut patch = Patch { points: [[(0.0_f64, 0.0_f64); 4]; 4], colors: [Color::Gray(0.0); 4] };

    // A non-zero flag with no previous patch has nothing to inherit, so it is read as a
    // fresh one rather than abandoned — the alternative is dropping every patch in a
    // stream whose first flag is wrong.
    let inherit = previous.filter(|_| (1..=3).contains(&edge));
    let first = match inherit {
        Some(prev) => {
            for (slot, source) in INHERITED[edge - 1].iter().enumerate() {
                let (di, dj) = STREAM_ORDER[slot];
                let (si, sj) = STREAM_ORDER[source - 1];
                patch.points[di][dj] = prev.points[si][sj];
            }
            patch.colors[0] = prev.colors[edge % 4];
            patch.colors[1] = prev.colors[(edge + 1) % 4];
            4
        }
        None => 0,
    };

    for &(i, j) in STREAM_ORDER.iter().take(total).skip(first) {
        let x = params.decode_at(0, reader.read(params.bits_coord)?, params.bits_coord);
        let y = params.decode_at(1, reader.read(params.bits_coord)?, params.bits_coord);
        patch.points[i][j] = (x, y);
    }
    for slot in patch.colors.iter_mut().skip(if first == 4 { 2 } else { 0 }) {
        *slot = params.read_color(reader)?;
    }
    Some(patch)
}

/// The equations 8.7.4.5.8 gives for a Coons patch's four interior points, as
/// `(weight, i, j)` terms over the boundary net. One row each for `p11`, `p12`, `p21`
/// and `p22`, every term divided by 9:
///
/// ```text
/// p11 = 1/9 (−4·p00 + 6·(p01+p10) − 2·(p03+p30) + 3·(p31+p13) − p33)
/// ```
///
/// A table rather than four expressions because they are one equation under a symmetry,
/// and written out they ran past the Rule 1 limit while hiding which index moved.
const INTERIOR: [[(f64, usize, usize); 8]; 4] = [
    [
        (-4.0, 0, 0),
        (6.0, 0, 1),
        (6.0, 1, 0),
        (-2.0, 0, 3),
        (-2.0, 3, 0),
        (3.0, 3, 1),
        (3.0, 1, 3),
        (-1.0, 3, 3),
    ],
    [
        (-4.0, 0, 3),
        (6.0, 0, 2),
        (6.0, 1, 3),
        (-2.0, 0, 0),
        (-2.0, 3, 3),
        (3.0, 3, 2),
        (3.0, 1, 0),
        (-1.0, 3, 0),
    ],
    [
        (-4.0, 3, 0),
        (6.0, 3, 1),
        (6.0, 2, 0),
        (-2.0, 3, 3),
        (-2.0, 0, 0),
        (3.0, 0, 1),
        (3.0, 2, 3),
        (-1.0, 0, 3),
    ],
    [
        (-4.0, 3, 3),
        (6.0, 3, 2),
        (6.0, 2, 3),
        (-2.0, 3, 0),
        (-2.0, 0, 3),
        (3.0, 0, 2),
        (3.0, 2, 0),
        (-1.0, 0, 0),
    ],
];

/// Where each row of [`INTERIOR`] lands in the control net.
const INTERIOR_AT: [(usize, usize); 4] = [(1, 1), (1, 2), (2, 1), (2, 2)];

/// Fills in the four interior points a Coons patch implies rather than carries, turning
/// it into the tensor-product patch 8.7.4.5.8 says it is a special case of.
fn fill_interior(p: &mut [[(f64, f64); 4]; 4]) {
    let mut computed = [(0.0_f64, 0.0_f64); 4];
    for (slot, terms) in INTERIOR.iter().enumerate() {
        let mut acc = (0.0_f64, 0.0_f64);
        for &(weight, i, j) in terms {
            acc.0 += weight * p[i][j].0;
            acc.1 += weight * p[i][j].1;
        }
        computed[slot] = (acc.0 / 9.0, acc.1 / 9.0);
    }
    for (slot, (i, j)) in INTERIOR_AT.into_iter().enumerate() {
        p[i][j] = computed[slot];
    }
}

/// Samples the surface on a grid and emits two triangles per cell.
fn emit_patch(patch: &Patch, out: &mut Vec<MeshTriangle>) {
    let step = 1.0_f64 / to_f64(PATCH_GRID);
    let at = |gu: usize, gv: usize| {
        let (u, v) = (to_f64(gu) * step, to_f64(gv) * step);
        (surface(&patch.points, u, v), corner_blend(&patch.colors, u, v))
    };
    for gu in 0..PATCH_GRID {
        for gv in 0..PATCH_GRID {
            let (a, b, c, d) = (at(gu, gv), at(gu + 1, gv), at(gu, gv + 1), at(gu + 1, gv + 1));
            out.push(to_triangle(&[a, b, c]));
            out.push(to_triangle(&[b, d, c]));
        }
    }
}

/// `S(u,v) = ΣΣ p_ij × B_i(u) × B_j(v)` — the bicubic tensor-product surface.
fn surface(p: &[[(f64, f64); 4]; 4], u: f64, v: f64) -> (f64, f64) {
    let (bu, bv) = (bernstein(u), bernstein(v));
    let mut point = (0.0_f64, 0.0_f64);
    for (i, bui) in bu.iter().enumerate() {
        for (j, bvj) in bv.iter().enumerate() {
            let weight = bui * bvj;
            point.0 += p[i][j].0 * weight;
            point.1 += p[i][j].1 * weight;
        }
    }
    point
}

/// The four cubic Bernstein polynomials at `t`.
fn bernstein(t: f64) -> [f64; 4] {
    let s = 1.0_f64 - t;
    [s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t]
}

/// Bilinear interpolation of the four corner colours over the unit square (8.7.4.5.7).
///
/// The corners are `c1` at `p00`, `c2` at `p03`, `c3` at `p33` and `c4` at `p30`, which
/// is `(u,v)` of `(0,0)`, `(0,1)`, `(1,1)` and `(1,0)`.
fn corner_blend(c: &[Color; 4], u: f64, v: f64) -> Color {
    let left = blend(c[0], c[1], v);
    let right = blend(c[3], c[2], v);
    blend(left, right, u)
}

fn to_f64(v: usize) -> f64 {
    u32::try_from(v).map_or(f64::from(u32::MAX), f64::from)
}
