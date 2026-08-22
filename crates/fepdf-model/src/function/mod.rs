//! PDF functions (ISO 32000-2, 7.10).
//!
//! A function maps *m* inputs to *n* outputs and is written in a document as one of four
//! types. Before Phase P this engine evaluated none of them, and two visible rendering
//! defects came from that one gap:
//!
//! * a `/Separation` tint is not a colour until its tint transform runs (8.6.6.4), and
//!   reading the single component as a grey level inverts it — 1.0 is full ink in a
//!   separation and white in `/DeviceGray`, so every spot colour at full tint rendered
//!   **white**;
//! * a gradient with more than two stops is written as a type 3 stitching function, whose
//!   `/C0` and `/C1` live one level down inside `/Functions`. Reading them off the
//!   outermost dictionary finds neither, and the shading fell back to black-to-white.
//!
//! Both were measured against PDFKit rather than argued: `target/colour/` holds the two
//! files, `ROADMAP.md` Phase P quotes the four numbers, and
//! `./scripts/test/crosscheck_image.sh` re-derives them.
//!
//! ```text
//! cargo run --example make_colour_fixtures -p fepdf-model
//! ./scripts/test/crosscheck_image.sh
//! ```
//!
//! **Evaluation returns `Option`, not a sentinel.** A type 4 program can underflow its
//! stack and a type 0 stream can be shorter than `/Size` says; an empty `Vec` would
//! conflate "this function produced nothing" with "this function has no outputs", and the
//! callers of this module choose a fallback colour on exactly that distinction.

mod postscript;

use crate::object::{Object, PdfName};
use crate::{Handle, PdfArena};
use bytes::Bytes;
pub use postscript::PostScriptFunction;
use std::collections::BTreeMap;

type Dict = BTreeMap<Handle<PdfName>, Object>;

/// How deep `/Functions` may nest before parsing gives up (RR-15 Rule 6: no unbounded
/// recursion). A stitching function containing stitching functions is legal and rare;
/// eight levels is past anything a producer writes and short of a stack problem.
const MAX_NESTING: usize = 8;

/// How many inputs a sampled function may have before interpolation stops being
/// multilinear. 2^m corners are visited per evaluation, so this caps that at 256.
/// Colour work uses m = 1; the cap exists so a malformed `/Size` cannot ask for 2^40.
const MAX_MULTILINEAR_INPUTS: usize = 8;

/// A PDF function (7.10). The four variants are the four `/FunctionType` values the
/// standard defines; there is no fifth, so this enum is closed by the standard rather
/// than by us.
#[derive(Debug, Clone)]
pub enum PdfFunction {
    /// Type 0, a sampled function (7.10.2).
    Sampled(SampledFunction),
    /// Type 2, exponential interpolation (7.10.3).
    Exponential(ExponentialFunction),
    /// Type 3, a stitching function (7.10.4).
    Stitching(StitchingFunction),
    /// Type 4, a PostScript calculator (7.10.5).
    PostScript(PostScriptFunction),
}

impl PdfFunction {
    /// Parses a function from a dictionary or stream object.
    ///
    /// Returns `None` when the object is not a function, names a `/FunctionType` this
    /// engine does not evaluate, or is missing an entry its type requires.
    pub fn parse(obj: &Object, arena: &PdfArena) -> Option<Self> {
        Self::parse_at(obj, arena, 0)
    }

    fn parse_at(obj: &Object, arena: &PdfArena, depth: usize) -> Option<Self> {
        if depth > MAX_NESTING {
            return None;
        }
        let resolved = obj.resolve(arena);
        let dict = match &resolved {
            Object::Dictionary(dh) | Object::Stream(dh, _) => arena.get_dict(*dh)?,
            _ => return None,
        };
        let data = match &resolved {
            // A type 0 or 4 function whose stream will not decode cannot be evaluated at
            // all, so this is a parse failure rather than a value to fall back from.
            Object::Stream(_, sd) => match arena.get_stream_bytes(sd) {
                Ok(bytes) => Some(bytes),
                Err(_) => return None,
            },
            _ => None,
        };

        let bounds = Bounds::parse(&dict, arena)?;
        match entry(&dict, arena, "FunctionType")?.as_integer()? {
            0 => SampledFunction::parse(&dict, arena, bounds, data?).map(Self::Sampled),
            2 => ExponentialFunction::parse(&dict, arena, bounds).map(Self::Exponential),
            3 => StitchingFunction::parse(&dict, arena, bounds, depth).map(Self::Stitching),
            4 => PostScriptFunction::parse(bounds, &data?).map(Self::PostScript),
            _ => None,
        }
    }

    /// Evaluates the function, returning `None` if it cannot produce a value.
    pub fn eval(&self, inputs: &[f64]) -> Option<Vec<f64>> {
        match self {
            Self::Sampled(f) => f.eval(inputs),
            Self::Exponential(f) => f.eval(inputs),
            Self::Stitching(f) => f.eval(inputs),
            Self::PostScript(f) => f.eval(inputs),
        }
    }

    /// The `/Domain` this function was declared over, as `[min0, max0, min1, max1, …]`.
    pub fn domain(&self) -> &[f64] {
        &self.bounds().domain
    }

    fn bounds(&self) -> &Bounds {
        match self {
            Self::Sampled(f) => &f.bounds,
            Self::Exponential(f) => &f.bounds,
            Self::Stitching(f) => &f.bounds,
            Self::PostScript(f) => f.bounds(),
        }
    }
}

/// One or more functions evaluated together, which is how a shading's `/Function` entry
/// is written: either a single function producing all *n* components, or an array of *n*
/// functions producing one each (8.7.4.5.2). Both forms occur, and a reader that handles
/// only the first paints the second black.
#[derive(Debug, Clone)]
pub struct FunctionSet {
    parts: Vec<PdfFunction>,
}

impl FunctionSet {
    /// Parses either a single function or an array of them.
    pub fn parse(obj: &Object, arena: &PdfArena) -> Option<Self> {
        if let Object::Array(ah) = obj.resolve(arena) {
            let items = arena.get_array(ah)?;
            let mut parts = Vec::with_capacity(items.len());
            for item in &items {
                parts.push(PdfFunction::parse(item, arena)?);
            }
            if parts.is_empty() {
                return None;
            }
            return Some(Self { parts });
        }
        Some(Self { parts: vec![PdfFunction::parse(obj, arena)?] })
    }

    /// Evaluates every part and concatenates the outputs in order.
    pub fn eval(&self, inputs: &[f64]) -> Option<Vec<f64>> {
        let mut out = Vec::new();
        for part in &self.parts {
            out.extend(part.eval(inputs)?);
        }
        if out.is_empty() { None } else { Some(out) }
    }

    /// The `/Domain` of the first part, which is the domain of the set: the standard
    /// requires every function in such an array to take the same single input.
    pub fn domain(&self) -> &[f64] {
        self.parts.first().map_or(&[] as &[f64], PdfFunction::domain)
    }
}

/// `/Domain` and the optional `/Range` every function type carries (Table 38).
#[derive(Debug, Clone)]
pub(crate) struct Bounds {
    domain: Vec<f64>,
    range: Option<Vec<f64>>,
}

impl Bounds {
    fn parse(dict: &Dict, arena: &PdfArena) -> Option<Self> {
        let domain = number_array(dict, arena, "Domain")?;
        if domain.len() < 2 || domain.len() % 2 != 0 {
            return None;
        }
        let range = number_array(dict, arena, "Range").filter(|r| r.len() >= 2 && r.len() % 2 == 0);
        Some(Self { domain, range })
    }

    /// Clips each input to its declared interval, as 7.10 requires before evaluation.
    fn clip_inputs(&self, inputs: &[f64]) -> Option<Vec<f64>> {
        let arity = self.domain.len() / 2;
        if inputs.len() < arity {
            return None;
        }
        Some(
            (0..arity)
                .map(|i| clip(inputs[i], self.domain[2 * i], self.domain[2 * i + 1]))
                .collect(),
        )
    }

    /// Clips outputs to `/Range` when one was declared. Type 0 and 4 must declare it;
    /// for types 2 and 3 it is optional and absent means no clipping.
    fn clip_outputs(&self, mut outputs: Vec<f64>) -> Vec<f64> {
        let Some(range) = &self.range else {
            return outputs;
        };
        for (j, value) in outputs.iter_mut().enumerate() {
            if let (Some(lo), Some(hi)) = (range.get(2 * j), range.get(2 * j + 1)) {
                *value = clip(*value, *lo, *hi);
            }
        }
        outputs
    }
}

/// Type 2, exponential interpolation (7.10.3): `y_j = C0_j + x^N × (C1_j − C0_j)`.
#[derive(Debug, Clone)]
pub struct ExponentialFunction {
    bounds: Bounds,
    c0: Vec<f64>,
    c1: Vec<f64>,
    exponent: f64,
}

impl ExponentialFunction {
    fn parse(dict: &Dict, arena: &PdfArena, bounds: Bounds) -> Option<Self> {
        let exponent = number(dict, arena, "N")?;
        // Table 40: both default to a single component, so `/C0` and `/C1` absent means
        // the function maps x to x. This is the form a `/Separation` over `/DeviceGray`
        // is usually written in.
        let c0 = number_array(dict, arena, "C0").unwrap_or_else(|| vec![0.0_f64]);
        let c1 = number_array(dict, arena, "C1").unwrap_or_else(|| vec![1.0_f64]);
        if c0.is_empty() || c0.len() != c1.len() {
            return None;
        }
        Some(Self { bounds, c0, c1, exponent })
    }

    fn eval(&self, inputs: &[f64]) -> Option<Vec<f64>> {
        let x = *self.bounds.clip_inputs(inputs)?.first()?;
        // `x.powf(n)` is NaN for negative x and fractional N. 7.10.3 makes it the
        // producer's job to keep `/Domain` inside where x^N is defined; a file that does
        // not gets 0 rather than a NaN propagating into a colour channel.
        let factor = if (self.exponent - 1.0_f64).abs() < f64::EPSILON {
            x
        } else {
            let raised = x.powf(self.exponent);
            if raised.is_finite() { raised } else { 0.0_f64 }
        };
        let out = self.c0.iter().zip(&self.c1).map(|(a, b)| a + factor * (b - a)).collect();
        Some(self.bounds.clip_outputs(out))
    }
}

/// Type 3, a stitching function (7.10.4): *k* subdomains, each mapped onto one of *k*
/// functions through `/Encode`.
#[derive(Debug, Clone)]
pub struct StitchingFunction {
    bounds: Bounds,
    parts: Vec<PdfFunction>,
    /// `/Bounds`, the k−1 interior split points.
    splits: Vec<f64>,
    encode: Vec<f64>,
}

impl StitchingFunction {
    fn parse(dict: &Dict, arena: &PdfArena, bounds: Bounds, depth: usize) -> Option<Self> {
        let Object::Array(ah) = entry(dict, arena, "Functions")? else {
            return None;
        };
        let items = arena.get_array(ah)?;
        let mut parts = Vec::with_capacity(items.len());
        for item in &items {
            parts.push(PdfFunction::parse_at(item, arena, depth + 1)?);
        }
        let k = parts.len();
        if k == 0 {
            return None;
        }
        let splits = number_array(dict, arena, "Bounds").unwrap_or_default();
        let encode = number_array(dict, arena, "Encode")?;
        if splits.len() + 1 != k || encode.len() < 2 * k {
            return None;
        }
        Some(Self { bounds, parts, splits, encode })
    }

    fn eval(&self, inputs: &[f64]) -> Option<Vec<f64>> {
        let x = *self.bounds.clip_inputs(inputs)?.first()?;
        let (low, high, index) = self.subdomain(x);
        let encoded = interpolate(
            x,
            low,
            high,
            *self.encode.get(2 * index)?,
            *self.encode.get(2 * index + 1)?,
        );
        let out = self.parts.get(index)?.eval(&[encoded])?;
        Some(self.bounds.clip_outputs(out))
    }

    /// The subdomain `x` falls in, as `(low, high, function index)`. 7.10.4 makes every
    /// subinterval half-open at the top except the last, which is closed — so a `x`
    /// exactly on a `/Bounds` entry belongs to the interval *above* it.
    fn subdomain(&self, x: f64) -> (f64, f64, usize) {
        let last = self.parts.len() - 1;
        let mut index = 0;
        while index < self.splits.len() && x >= self.splits[index] {
            index += 1;
        }
        let low = if index == 0 { self.bounds.domain[0] } else { self.splits[index - 1] };
        let high = if index == last {
            self.bounds.domain[1]
        } else {
            self.splits.get(index).copied().unwrap_or(self.bounds.domain[1])
        };
        (low, high, index)
    }
}

/// Type 0, a sampled function (7.10.2): a table of samples with multilinear
/// interpolation between them.
#[derive(Debug, Clone)]
pub struct SampledFunction {
    bounds: Bounds,
    range: Vec<f64>,
    size: Vec<u32>,
    bits: u32,
    encode: Vec<f64>,
    decode: Vec<f64>,
    samples: Bytes,
}

impl SampledFunction {
    fn parse(dict: &Dict, arena: &PdfArena, bounds: Bounds, samples: Bytes) -> Option<Self> {
        // `/Range` is required for a type 0 function, not optional as it is for 2 and 3:
        // without it there is no way to know how many outputs a sample row holds.
        let range = bounds.range.clone()?;
        let size = Self::parse_size(dict, arena, bounds.domain.len() / 2)?;
        let bits = u32::try_from(entry(dict, arena, "BitsPerSample")?.as_integer()?).ok()?;
        if !matches!(bits, 1 | 2 | 4 | 8 | 12 | 16 | 24 | 32) {
            return None;
        }
        let encode = number_array(dict, arena, "Encode")
            .filter(|e| e.len() >= 2 * size.len())
            .unwrap_or_else(|| {
                size.iter().flat_map(|s| [0.0_f64, f64::from(s.saturating_sub(1))]).collect()
            });
        let decode = number_array(dict, arena, "Decode")
            .filter(|d| d.len() >= range.len())
            .unwrap_or_else(|| range.clone());
        Some(Self { bounds, range, size, bits, encode, decode, samples })
    }

    fn parse_size(dict: &Dict, arena: &PdfArena, arity: usize) -> Option<Vec<u32>> {
        let Object::Array(ah) = entry(dict, arena, "Size")? else {
            return None;
        };
        let items = arena.get_array(ah)?;
        if items.len() < arity || arity == 0 {
            return None;
        }
        let mut size = Vec::with_capacity(arity);
        for item in items.iter().take(arity) {
            let n = u32::try_from(item.resolve(arena).as_integer()?).ok()?;
            if n == 0 {
                return None;
            }
            size.push(n);
        }
        Some(size)
    }

    fn eval(&self, inputs: &[f64]) -> Option<Vec<f64>> {
        let clipped = self.bounds.clip_inputs(inputs)?;
        let outputs = self.range.len() / 2;
        let coords = self.encode_inputs(&clipped)?;
        let raw = if coords.len() > MAX_MULTILINEAR_INPUTS {
            let nearest: Vec<u32> = coords.iter().map(|c| round_to_index(*c)).collect();
            (0..outputs).map(|j| self.sample_at(&nearest, j)).collect::<Option<Vec<f64>>>()?
        } else {
            self.interpolate_corners(&coords, outputs)?
        };
        let decoded = raw
            .iter()
            .enumerate()
            .map(|(j, v)| {
                let lo = self.decode.get(2 * j).copied().unwrap_or(0.0_f64);
                let hi = self.decode.get(2 * j + 1).copied().unwrap_or(1.0_f64);
                lo + v * (hi - lo)
            })
            .collect();
        Some(self.bounds.clip_outputs(decoded))
    }

    /// Maps each clipped input onto its sample grid coordinate, per 7.10.2's
    /// `Interpolate` then clip to `[0, Size_i − 1]`.
    fn encode_inputs(&self, clipped: &[f64]) -> Option<Vec<f64>> {
        let mut coords = Vec::with_capacity(self.size.len());
        for (i, size) in self.size.iter().enumerate() {
            let e = interpolate(
                clipped[i],
                self.bounds.domain[2 * i],
                self.bounds.domain[2 * i + 1],
                *self.encode.get(2 * i)?,
                *self.encode.get(2 * i + 1)?,
            );
            coords.push(clip(e, 0.0_f64, f64::from(size.saturating_sub(1))));
        }
        Some(coords)
    }

    /// Multilinear interpolation over the 2^m grid corners surrounding `coords`.
    fn interpolate_corners(&self, coords: &[f64], outputs: usize) -> Option<Vec<f64>> {
        let m = coords.len();
        let mut acc = vec![0.0_f64; outputs];
        let mut corner = vec![0_u32; m];
        for mask in 0..(1_usize << m) {
            let mut weight = 1.0_f64;
            for (i, c) in coords.iter().enumerate() {
                let base = floor_to_index(*c);
                let frac = c - f64::from(base);
                let high = (mask >> i) & 1 == 1;
                let limit = self.size[i].saturating_sub(1);
                corner[i] = if high { base.saturating_add(1).min(limit) } else { base };
                weight *= if high { frac } else { 1.0_f64 - frac };
            }
            if weight == 0.0_f64 {
                continue;
            }
            for (j, slot) in acc.iter_mut().enumerate() {
                *slot += weight * self.sample_at(&corner, j)?;
            }
        }
        Some(acc)
    }

    /// One sample, normalised to `[0, 1]`. Samples are stored with the first input
    /// dimension varying fastest (7.10.2), and the *n* outputs of one grid point are
    /// adjacent.
    fn sample_at(&self, corner: &[u32], output: usize) -> Option<f64> {
        let outputs = self.range.len() / 2;
        let mut index = 0_u64;
        let mut stride = 1_u64;
        for (i, c) in corner.iter().enumerate() {
            index += u64::from(*c) * stride;
            stride *= u64::from(*self.size.get(i)?);
        }
        let bit = (index * u64::from(u32::try_from(outputs).ok()?)
            + u64::from(u32::try_from(output).ok()?))
            * u64::from(self.bits);
        let raw = read_bits(&self.samples, bit, self.bits)?;
        let max = if self.bits >= 64 { u64::MAX } else { (1_u64 << self.bits) - 1 };
        Some(u64_as_f64(raw) / u64_as_f64(max))
    }
}

/// Reads `count` bits big-endian from `data` starting at `offset` bits.
///
/// Bit by bit rather than by word: `/BitsPerSample` may be 12 or 24, which straddle byte
/// boundaries, and a sampled function in colour work is evaluated at a few dozen points,
/// not per pixel. Returns `None` past the end of the stream, which is how a `/Size` that
/// promises more samples than the stream holds becomes a fallback instead of a panic.
fn read_bits(data: &[u8], offset: u64, count: u32) -> Option<u64> {
    let mut value = 0_u64;
    for i in 0..u64::from(count) {
        let bit = offset.checked_add(i)?;
        let byte = *data.get(usize::try_from(bit / 8).ok()?)?;
        let shift = 7 - u32::try_from(bit % 8).ok()?;
        value = (value << 1) | u64::from((byte >> shift) & 1);
    }
    Some(value)
}

/// 7.10's `Interpolate`: maps `x` from `[x_min, x_max]` onto `[y_min, y_max]`.
/// A degenerate source interval yields `y_min`, which is what a stitching function with
/// a zero-width subdomain needs rather than a division by zero.
fn interpolate(x: f64, x_min: f64, x_max: f64, y_min: f64, y_max: f64) -> f64 {
    let span = x_max - x_min;
    if span.abs() < f64::EPSILON {
        return y_min;
    }
    y_min + (x - x_min) * (y_max - y_min) / span
}

/// Clips `x` into the interval the two bounds describe, in either order.
///
/// Written out rather than `f64::clamp` because `clamp` panics when `min > max` or either
/// bound is NaN, and both come straight out of a file here — RR-15 Rule 2 forbids the
/// unwrap that would be, and a reversed `/Domain` is a thing producers write.
fn clip(x: f64, a: f64, b: f64) -> f64 {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

fn floor_to_index(x: f64) -> u32 {
    let f = x.floor();
    if f <= 0.0_f64 {
        0
    } else if f >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        f as u32
    }
}

fn round_to_index(x: f64) -> u32 {
    floor_to_index(x + 0.5_f64)
}

/// `u64` to `f64` without a lossy-cast lint: sample values and their maxima are at most
/// 2^32 − 1 here, which `f64` holds exactly.
fn u64_as_f64(v: u64) -> f64 {
    u32::try_from(v).map_or(f64::from(u32::MAX), f64::from)
}

fn entry(dict: &Dict, arena: &PdfArena, key: &str) -> Option<Object> {
    let k = arena.intern_name(PdfName::new(key));
    dict.get(&k).map(|o| o.resolve(arena))
}

fn number(dict: &Dict, arena: &PdfArena, key: &str) -> Option<f64> {
    entry(dict, arena, key)?.as_f64()
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
