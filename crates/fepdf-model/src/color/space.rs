//! Resolving a `/ColorSpace` entry far enough to turn operands into a colour (8.6).
//!
//! `ColorSpaceKind` names the *family* a space belongs to, which is all the interpreter
//! needed while every space it painted was a device space. It is not enough for the two
//! spaces whose components are not a colour until something runs:
//!
//! * `/Separation` (8.6.6.4) — one tint component, meaningful only through its tint
//!   transform into an alternate space;
//! * `/DeviceN` (8.6.6.5) — the same with several colorants.
//!
//! Reading a separation's single component as a grey level is not a near miss. It
//! inverts: 1.0 is full ink in a separation and white in `/DeviceGray`, so every spot
//! colour at full tint came out **white**. `ROADMAP.md` Phase P has the measurement.
//!
//! This also carries the component *count*, which is what tells a `scn` with three
//! operands in an `/ICCBased` space from one in `/DeviceRGB` without guessing from the
//! operand count — the guess the interpreter used to make.

use super::ColorSpaceKind;
use crate::PdfArena;
use crate::function::FunctionSet;
use crate::graphics::Color;
use crate::object::{Object, PdfName};
use std::collections::BTreeMap;

/// How deep an alternate space may nest before resolution gives up (RR-15 Rule 6).
const MAX_DEPTH: usize = 8;

/// A `/ColorSpace` resolved to what painting needs: the family, how many components a
/// colour in it has, and — for the two spaces that need one — the transform that turns
/// those components into a colour.
#[derive(Debug, Clone)]
pub struct ResolvedColorSpace {
    /// The family this space belongs to.
    pub kind: ColorSpaceKind,
    /// How many components a colour in this space is written with.
    pub components: usize,
    tint: Option<TintTransform>,
    calibrated: Option<CalRgb>,
}

/// The gamma and matrix of a `/CalRGB` space (8.6.5.3, Table 63).
///
/// One transformation stage: each component is decoded by its own gamma, then the three
/// are multiplied by `/Matrix` to give XYZ directly — "Decode LMN" and "Matrix LMN" are
/// implicitly the identity because there is no second stage.
#[derive(Debug, Clone)]
struct CalRgb {
    gamma: [f64; 3],
    /// `[XA YA ZA XB YB ZB XC YC ZC]`, so a component's three contributions are adjacent.
    matrix: [f64; 9],
}

/// The tint transform of a `/Separation` or `/DeviceN` space, with the shape of the
/// space its outputs land in.
#[derive(Debug, Clone)]
struct TintTransform {
    function: FunctionSet,
    alternate_components: usize,
}

impl ResolvedColorSpace {
    /// Resolves a colour space object: a name, or an array whose first element names the
    /// family. Returns `None` for anything this engine cannot paint in.
    pub fn parse(obj: &Object, arena: &PdfArena) -> Option<Self> {
        Self::parse_at(obj, arena, 0)
    }

    /// A device space of a known component count, for the paths that already know which
    /// one they are in.
    pub fn device(kind: ColorSpaceKind, components: usize) -> Self {
        Self { kind, components, tint: None, calibrated: None }
    }

    fn parse_at(obj: &Object, arena: &PdfArena, depth: usize) -> Option<Self> {
        if depth > MAX_DEPTH {
            return None;
        }
        match obj.resolve(arena) {
            Object::Name(h) => Self::from_family(&arena.get_name_str(h)?),
            Object::Array(ah) => {
                let items = arena.get_array(ah)?;
                let head = items.first()?.resolve(arena).as_name()?;
                Self::from_array(&arena.get_name_str(head)?, &items, arena, depth)
            }
            _ => None,
        }
    }

    /// The device spaces, which may be written bare as a name (8.6.3).
    ///
    /// 8.6.3: the operand of `cs` is one of these names *or* a key into the resource
    /// dictionary's `/ColorSpace` subdictionary, and these names win. The interpreter
    /// used to stop at the first half, so every named space — which is how every
    /// `/Separation` and every `/ICCBased` space is written — resolved to `Unknown` and
    /// was then guessed at from the operand count.
    pub fn from_family(family: &str) -> Option<Self> {
        // The one- and three-letter forms are the abbreviations legal inside an inline
        // image dictionary (Table 91), which reach this by the same path.
        match family {
            "DeviceGray" | "G" | "CalGray" => Some(Self::device(ColorSpaceKind::DeviceGray, 1)),
            "DeviceRGB" | "RGB" | "CalRGB" => Some(Self::device(ColorSpaceKind::DeviceRGB, 3)),
            "DeviceCMYK" | "CMYK" => Some(Self::device(ColorSpaceKind::DeviceCMYK, 4)),
            "Pattern" => Some(Self::device(ColorSpaceKind::Pattern, 1)),
            _ => None,
        }
    }

    fn from_array(family: &str, items: &[Object], arena: &PdfArena, depth: usize) -> Option<Self> {
        match family {
            "CalGray" => Some(Self::device(ColorSpaceKind::CalGray, 1)),
            "CalRGB" => Some(Self::calibrated_rgb(items, arena)),
            "Lab" => Some(Self::device(ColorSpaceKind::Lab, 3)),
            "ICCBased" => Some(Self::from_icc(items, arena)),
            // An indexed colour is written as one index into the palette; converting it
            // to a colour needs the lookup table, which the image path owns rather than
            // this one.
            "Indexed" | "I" => Some(Self::device(ColorSpaceKind::Indexed, 1)),
            "Pattern" => Some(Self::device(ColorSpaceKind::Pattern, 1)),
            "Separation" => Self::from_separation(items, arena, depth),
            "DeviceN" => Self::from_devicen(items, arena, depth),
            _ => Self::from_family(family),
        }
    }

    /// `[/CalRGB dict]` (8.6.5.3). A missing `/Gamma` is `[1 1 1]` and a missing
    /// `/Matrix` the identity, which together make the space behave as `/DeviceRGB` —
    /// which is what this engine did for *every* `CalRGB`, calibrated or not, until the
    /// parameters were read.
    fn calibrated_rgb(items: &[Object], arena: &PdfArena) -> Self {
        let mut space = Self::device(ColorSpaceKind::CalRGB, 3);
        let Some(dict) = items
            .get(1)
            .map(|o| o.resolve(arena))
            .and_then(|o| o.as_dict_handle())
            .and_then(|dh| arena.get_dict(dh))
        else {
            return space;
        };
        let gamma = numbers(&dict, arena, "Gamma", 3).unwrap_or([1.0_f64, 1.0, 1.0].to_vec());
        let matrix = numbers(&dict, arena, "Matrix", 9)
            .unwrap_or_else(|| vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        let (Ok(gamma), Ok(matrix)) = (<[f64; 3]>::try_from(gamma), <[f64; 9]>::try_from(matrix))
        else {
            return space;
        };
        space.calibrated = Some(CalRgb { gamma, matrix });
        space
    }

    /// `[/ICCBased stream]`: the stream's `/N` gives the component count, and 8.6.5.5
    /// makes `/N` the authority even when a profile disagrees.
    fn from_icc(items: &[Object], arena: &PdfArena) -> Self {
        let components = items
            .get(1)
            .map(|o| o.resolve(arena))
            .and_then(|o| o.as_dict_handle())
            .and_then(|dh| arena.get_dict(dh))
            .and_then(|d| d.get(&arena.intern_name(PdfName::new("N"))).cloned())
            .and_then(|n| n.resolve(arena).as_integer())
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| matches!(n, 1 | 3 | 4))
            .unwrap_or(3);
        Self { kind: ColorSpaceKind::ICCBased, components, tint: None, calibrated: None }
    }

    /// `[/Separation name alternateSpace tintTransform]` (8.6.6.4).
    fn from_separation(items: &[Object], arena: &PdfArena, depth: usize) -> Option<Self> {
        let tint = Self::tint_transform(items.get(2)?, items.get(3)?, arena, depth)?;
        Some(Self {
            kind: ColorSpaceKind::Separation,
            components: 1,
            tint: Some(tint),
            calibrated: None,
        })
    }

    /// `[/DeviceN names alternateSpace tintTransform …]` (8.6.6.5).
    fn from_devicen(items: &[Object], arena: &PdfArena, depth: usize) -> Option<Self> {
        let Object::Array(names) = items.get(1)?.resolve(arena) else {
            return None;
        };
        let components = arena.get_array(names)?.len();
        if components == 0 {
            return None;
        }
        let tint = Self::tint_transform(items.get(2)?, items.get(3)?, arena, depth)?;
        Some(Self { kind: ColorSpaceKind::DeviceN, components, tint: Some(tint), calibrated: None })
    }

    fn tint_transform(
        alternate: &Object,
        function: &Object,
        arena: &PdfArena,
        depth: usize,
    ) -> Option<TintTransform> {
        let alternate = Self::parse_at(alternate, arena, depth + 1)?;
        let function = FunctionSet::parse(function, arena)?;
        Some(TintTransform { function, alternate_components: alternate.components })
    }

    /// Turns components written in this space into a colour, running the tint transform
    /// when there is one.
    ///
    /// Returns `None` rather than a guess when the components do not fit the space or a
    /// tint transform will not evaluate: the caller records what it fell back to, and a
    /// silent black is indistinguishable from a black the file asked for.
    pub fn to_color(&self, components: &[f64]) -> Option<Color> {
        if let Some(cal) = &self.calibrated {
            return cal.to_color(components);
        }
        let Some(tint) = &self.tint else {
            return components_to_color(components);
        };
        if components.len() < self.components {
            return None;
        }
        let out = tint.function.eval(&components[..self.components])?;
        if out.len() < tint.alternate_components {
            return None;
        }
        components_to_color(&out[..tint.alternate_components])
    }

    /// Whether painting in this space needs a tint transform run first.
    pub fn is_tinted(&self) -> bool {
        self.tint.is_some()
    }

    /// A colour from components whose space is not known, taken from how many there are.
    ///
    /// For the callers that have components and no `/ColorSpace` to read them against —
    /// a shading missing the entry, which is malformed but occurs. The count is the only
    /// evidence left, and it is the same evidence the interpreter used to run on.
    pub fn color_from_components(values: &[f64]) -> Option<Color> {
        components_to_color(values)
    }
}

/// Maps a component vector onto the colour model its length names. The three lengths are
/// the three `Color` carries; anything else is a space this engine does not paint.
fn components_to_color(values: &[f64]) -> Option<Color> {
    match values {
        [g] => Some(Color::Gray(*g)),
        [r, g, b] => Some(Color::Rgb(*r, *g, *b)),
        [c, m, y, k] => Some(Color::Cmyk(*c, *m, *y, *k)),
        _ => None,
    }
}

impl CalRgb {
    /// 8.6.5.3's one transformation stage: gamma-decode each component, then
    /// `X = XA·A^GR + XB·B^GG + XC·C^GB` and likewise for Y and Z.
    fn to_color(&self, components: &[f64]) -> Option<Color> {
        let [a, b, c] = <[f64; 3]>::try_from(components.get(..3)?).ok()?;
        // "These three colour components shall be in the range 0.0 to 1.0; component
        // values falling outside that range shall be adjusted to the nearest valid value
        // without error indication."
        let decode = |value: f64, gamma: f64| value.clamp(0.0, 1.0).powf(gamma);
        let abc = [decode(a, self.gamma[0]), decode(b, self.gamma[1]), decode(c, self.gamma[2])];
        let m = &self.matrix;
        // `/Matrix` lists a *component's* three contributions together, so the entries
        // for X are 0, 3 and 6 rather than 0, 1 and 2. Reading it as three rows instead
        // of three columns transposes the space and is the easy mistake here.
        let x = m[0] * abc[0] + m[3] * abc[1] + m[6] * abc[2];
        let y = m[1] * abc[0] + m[4] * abc[1] + m[7] * abc[2];
        let z = m[2] * abc[0] + m[5] * abc[1] + m[8] * abc[2];
        Some(crate::graphics::xyz_to_srgb(x, y, z))
    }
}

/// A numeric array of exactly `want` entries, or `None`.
fn numbers(
    dict: &BTreeMap<crate::Handle<PdfName>, Object>,
    arena: &PdfArena,
    key: &str,
    want: usize,
) -> Option<Vec<f64>> {
    let entry = dict.get(&arena.intern_name(PdfName::new(key)))?.resolve(arena);
    let Object::Array(handle) = entry else {
        return None;
    };
    let items = arena.get_array(handle)?;
    if items.len() != want {
        return None;
    }
    let mut out = Vec::with_capacity(want);
    for item in &items {
        out.push(item.resolve(arena).as_f64()?);
    }
    Some(out)
}
