use crate::RenderBackend;
use crate::interpreter::Interpreter;
use fepdf_model::color::ResolvedColorSpace;
use fepdf_model::function::FunctionSet;
use fepdf_model::graphics::{Color, ColorSpaceKind};
use fepdf_model::interpretation::Decision;
use fepdf_model::{Handle, Object, Paint, PdfArena, PdfName, PdfResult};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A shading's `/Extend` pair, defaulting to extending at both ends when it says nothing.
///
/// **Read identically in both shading branches** — 8.7.4.5.3's axial and 8.7.4.5.4's
/// radial — fourteen lines apiece, differing in nothing.
fn read_extend(
    arena: &fepdf_model::PdfArena,
    dict: &std::collections::BTreeMap<Handle<PdfName>, fepdf_model::Object>,
) -> [bool; 2] {
    let extend_key = arena.intern_name(PdfName::new("Extend"));
    if let Some(fepdf_model::Object::Array(ah)) = dict.get(&extend_key).map(|o| o.resolve(arena))
        && let Some(arr) = arena.get_array(ah)
        && let [start, end, ..] = arr.as_slice()
    {
        [
            start.resolve(arena).as_bool().unwrap_or(true),
            end.resolve(arena).as_bool().unwrap_or(true),
        ]
    } else {
        [true, true]
    }
}

impl Interpreter<'_> {
    pub(crate) fn handle_color_operator(&mut self, op: &str) -> PdfResult<()> {
        match op {
            "cs" | "CS" => self.handle_cs(op),
            "g" | "G" => self.handle_gray(op),
            "rg" | "RG" => self.handle_rgb(op),
            "k" | "K" => self.handle_cmyk(op),
            "sc" | "scn" | "SC" | "SCN" => self.handle_sc(op),
            _ => Ok(()),
        }
    }

    fn handle_cs(&mut self, op: &str) -> PdfResult<()> {
        let is_fill = op == "cs";
        let name = self.pop_name()?;
        let space = self.resolve_color_space(&name);
        let kind = space.as_ref().map_or_else(|| kind_from_name(name.as_str()), |s| s.kind);
        if kind == ColorSpaceKind::Unknown {
            self.record_unknown_colour_space(op, &name);
        }
        if is_fill {
            self.state.fill_color_space = kind;
            self.state.fill_space = space;
        } else {
            self.state.stroke_color_space = kind;
            self.state.stroke_space = space;
        }
        Ok(())
    }

    /// Records a `cs` operand that names no colour space this engine could reach.
    ///
    /// **Measured before this existed**: `/Frobnicate cs 1 0 0 sc` filled red, byte for
    /// byte the same as `/DeviceRGB`, and `/Frobnicate cs 0 1 1 0 sc` filled the CMYK
    /// red — the operand count decided the model and nothing said so. `CODING.md`'s
    /// Rule 5 section names that exact failure as the thing a catch-all must not do:
    /// "a catch-all turns 'unsupported colour space' into 'silently renders black'".
    ///
    /// Rule 20 asks for the clause and what was done, so the two cases are told apart:
    /// a name the resources do not carry at all, and one they carry in a form this
    /// engine could not read. `/Indexed` reaches here too and is **not** recorded — it
    /// takes the operand-count path deliberately, because its operand is an index into a
    /// palette rather than a colour, and saying so on every indexed image would bury the
    /// case this is for.
    ///
    /// The colour drawn does not change. What the operand count produces is often right,
    /// and often right is exactly what a silent acceptance looks like.
    fn record_unknown_colour_space(&self, op: &str, name: &PdfName) {
        let shown = name.as_str().to_string();
        let key = self.doc.arena().intern_name(PdfName::new("ColorSpace"));
        let (found, action) = match self.find_resource(&key, name) {
            Ok(entry) => {
                if matches!(
                    ResolvedColorSpace::parse(&entry, self.doc.arena()).map(|s| s.kind),
                    Some(ColorSpaceKind::Indexed)
                ) {
                    return;
                }
                (
                    format!("/{shown} names a /ColorSpace resource this engine cannot read"),
                    "took the colour model from how many operands the next sc/scn carries",
                )
            }
            Err(_) => (
                format!(
                    "operator {op} was given /{shown}, which is neither a device family \
                         nor a name in /ColorSpace"
                ),
                "took the colour model from how many operands the next sc/scn carries",
            ),
        };
        self.doc.record(Decision::violation("8.6.3", found, action));
    }

    /// Resolves a `cs` operand: a device space name, or a key into the page's
    /// `/ColorSpace` resources (8.6.3).
    ///
    /// The second half is new. Every `/Separation` and every `/ICCBased` space is
    /// written as a resource name, so before this the operand never matched anything and
    /// the space came out `Unknown` — after which `scn` guessed the colour model from
    /// how many operands there were. For a separation that guess is one number, read as
    /// a grey level, which inverts the tint.
    ///
    /// Parsing is memoised on the document, because a page names the same resource on
    /// every `cs` and reading an `/ICCBased` profile out of its stream is not free.
    fn resolve_color_space(&self, name: &PdfName) -> Option<Arc<ResolvedColorSpace>> {
        if let Some(device) = ResolvedColorSpace::from_family(name.as_str()) {
            // 8.6.5.6, before the device space itself: "If such an entry is present, its
            // value shall be used as the colour space for the operation currently being
            // performed."
            //
            // Not memoised: `/DefaultRGB` is looked up through the resource stack, so the
            // answer depends on where in the stream this is, which a key made of the
            // operand name alone cannot say.
            return Some(Arc::new(self.default_space(name.as_str()).unwrap_or(device)));
        }
        let key = self.doc.arena().intern_name(PdfName::new("ColorSpace"));
        // `find_resource` reports a missing entry as an error. Here that is an ordinary
        // absence rather than a failure — the operand is simply not a resource name — so
        // it becomes `None` and the caller falls back to reading the name itself.
        let Ok(entry) = self.find_resource(&key, name) else {
            return None;
        };
        let space = self.doc.resolved_color_space(&entry)?;
        // `/Indexed` stays on the operand-count path deliberately. Its operand is an
        // index into a palette, not a colour, and turning it into one needs the lookup
        // table the image path owns. Routing it through here would change what it paints
        // on files this change has not measured, which is a separate defect from the two
        // that were.
        if matches!(space.kind, ColorSpaceKind::Indexed) {
            return None;
        }
        Some(space)
    }

    fn handle_gray(&mut self, op: &str) -> PdfResult<()> {
        let gray = self.pop_f64()?;
        let c = self.device_color("DeviceGray", &[gray], Color::Gray(gray));
        if op == "g" {
            self.state.fill_color = c;
            self.backend.set_fill_color(c);
        } else {
            self.state.stroke_color = c;
            self.backend.set_stroke_color(c);
        }
        Ok(())
    }

    fn handle_rgb(&mut self, op: &str) -> PdfResult<()> {
        let b = self.pop_f64()?;
        let g = self.pop_f64()?;
        let r = self.pop_f64()?;
        let c = self.device_color("DeviceRGB", &[r, g, b], Color::Rgb(r, g, b));
        if op == "rg" {
            self.state.fill_color = c;
            self.backend.set_fill_color(c);
        } else {
            self.state.stroke_color = c;
            self.backend.set_stroke_color(c);
        }
        Ok(())
    }

    fn handle_cmyk(&mut self, op: &str) -> PdfResult<()> {
        let k = self.pop_f64()?;
        let y = self.pop_f64()?;
        let m = self.pop_f64()?;
        let c = self.pop_f64()?;
        let col = self.device_color("DeviceCMYK", &[c, m, y, k], Color::Cmyk(c, m, y, k));
        if op == "k" {
            self.state.fill_color = col;
            self.backend.set_fill_color(col);
        } else {
            self.state.stroke_color = col;
            self.backend.set_stroke_color(col);
        }
        Ok(())
    }

    fn handle_sc(&mut self, op: &str) -> PdfResult<()> {
        let is_fill = op == "sc" || op == "scn";

        // 8.6.8.2: in a Pattern colour space the operands are an optional set of
        // numbers followed by a *name*, which keys the resource dictionary's `/Pattern`
        // subdictionary.
        if self.handle_pattern_color(is_fill) {
            return Ok(());
        }

        let resolved =
            if is_fill { self.state.fill_space.clone() } else { self.state.stroke_space.clone() };
        let col = match resolved {
            Some(space) => self.resolved_sc(&space, op)?,
            None => self.device_sc(is_fill, op)?,
        };

        if is_fill {
            self.state.fill_color = col;
            self.backend.set_fill_color(col);
        } else {
            self.state.stroke_color = col;
            self.backend.set_stroke_color(col);
        }
        Ok(())
    }

    /// Paints in a space that `cs` resolved through the page's resources, running the
    /// tint transform when the space has one (8.6.6).
    fn resolved_sc(&mut self, space: &ResolvedColorSpace, op: &str) -> PdfResult<Color> {
        let count = self.stack.len();
        if count < space.components {
            return self.fallback_sc(op, count, space.kind);
        }
        let mut components = vec![0.0_f64; space.components];
        // Operands were pushed c1 … cn, so popping fills the vector from the back.
        for slot in components.iter_mut().rev() {
            *slot = self.pop_f64()?;
        }
        if let Some(color) = space.to_color(&components) {
            return Ok(color);
        }
        // RR-15 Rule 20: a tint this engine could not transform is recorded rather than
        // logged. A black painted silently here is indistinguishable from a black the
        // file asked for, which is the whole reason the separation defect survived.
        self.doc.record(Decision::violation(
            "8.6.6",
            format!("a {:?} tint transform did not evaluate at {components:?}", space.kind),
            "Painted black. Components in a tinted space are not a colour until the \
             transform runs, so there is nothing else to fall back to"
                .to_string(),
        ));
        Ok(Color::Gray(0.0))
    }

    /// The `/DefaultGray`, `/DefaultRGB` or `/DefaultCMYK` standing in for a device
    /// space, if the resources declare one (8.6.5.6).
    ///
    /// A `shall`, and nothing read it: 30 files in the corpora carry `/DefaultRGB` —
    /// including two named after the feature — and three carry `/DefaultCMYK`, one of
    /// them `samples/fy05.pdf`. The clause reaches further than this, to the base of an
    /// `/Indexed` space, the underlying space of a `/Pattern` and the alternate of a
    /// `/Separation`; those are not done and are their own entry.
    fn default_space(&self, family: &str) -> Option<ResolvedColorSpace> {
        // Only the three device spaces are remapped. `/CalGray` and `/CalRGB` reach
        // `from_family` too, through the abbreviations an inline image may use, and they
        // are not device spaces.
        let (default, components) = match family {
            "DeviceGray" | "G" => ("DefaultGray", 1),
            "DeviceRGB" | "RGB" => ("DefaultRGB", 3),
            "DeviceCMYK" | "CMYK" => ("DefaultCMYK", 4),
            _ => return None,
        };
        let key = self.doc.arena().intern_name(PdfName::new("ColorSpace"));
        let Ok(entry) = self.find_resource(&key, &PdfName::new(default)) else {
            return None;
        };
        let space = ResolvedColorSpace::parse(&entry, self.doc.arena())?;
        // "The default colour space ... shall have the same number of components as the
        // original space." One that does not is not a substitute for it.
        (space.components == components).then_some(space)
    }

    /// A device colour, put through the default space when the resources declare one.
    ///
    /// `g`, `rg` and `k` never touch `cs`, and 8.6.5.6 covers them anyway: "Regardless of
    /// how the colour space is specified, it shall be subject to remapping as described
    /// below."
    fn device_color(&self, family: &str, components: &[f64], plain: Color) -> Color {
        self.default_space(family).and_then(|space| space.to_color(components)).unwrap_or(plain)
    }

    /// The device-space path: the colour model comes from the space `cs` named, and
    /// from the operand count where it named nothing this engine resolved.
    fn device_sc(&mut self, is_fill: bool, op: &str) -> PdfResult<Color> {
        let cs = if is_fill { self.state.fill_color_space } else { self.state.stroke_color_space };
        let count = self.stack.len();

        let col = match cs {
            ColorSpaceKind::DeviceGray => Color::Gray(self.pop_f64()?),
            ColorSpaceKind::DeviceRGB if count >= 3 => {
                let b = self.pop_f64()?;
                let g = self.pop_f64()?;
                let r = self.pop_f64()?;
                Color::Rgb(r, g, b)
            }
            ColorSpaceKind::DeviceCMYK if count >= 4 => {
                let k = self.pop_f64()?;
                let y = self.pop_f64()?;
                let m = self.pop_f64()?;
                let c = self.pop_f64()?;
                Color::Cmyk(c, m, y, k)
            }
            // Also reached when DeviceRGB/DeviceCMYK arrive with too few operands,
            // since those arms are guarded.
            ColorSpaceKind::DeviceRGB
            | ColorSpaceKind::DeviceCMYK
            | ColorSpaceKind::CalGray
            | ColorSpaceKind::CalRGB
            | ColorSpaceKind::Lab
            | ColorSpaceKind::ICCBased
            | ColorSpaceKind::Pattern
            | ColorSpaceKind::Indexed
            | ColorSpaceKind::Separation
            | ColorSpaceKind::DeviceN
            | ColorSpaceKind::Unknown => self.fallback_sc(op, count, cs)?,
        };
        Ok(col)
    }

    /// Resolves and sets a pattern paint for scn/SCN operators.
    fn handle_pattern_color(&mut self, is_fill: bool) -> bool {
        let named = matches!(self.stack.last(), Some(fepdf_model::Object::Name(_)));
        if !named {
            return false;
        }
        let Ok(name) = self.pop_name() else {
            return false;
        };
        // `c1 … cn /Name scn` for an uncoloured pattern: consume components
        while matches!(
            self.stack.last(),
            Some(fepdf_model::Object::Integer(_) | fepdf_model::Object::Real(_))
        ) {
            self.stack.pop();
        }
        let name_str = name.as_str().to_string();
        let res_key = self.doc.arena().intern_name(PdfName::new("Pattern"));
        if let Ok(entry) = self.find_resource(&res_key, &name)
            && let Some(pattern) = parse_pattern_object(&entry, self.doc.arena())
        {
            let paint = Paint::Pattern(pattern);
            if is_fill {
                self.backend.set_fill_paint(&paint);
            } else {
                self.backend.set_stroke_paint(&paint);
            }
            return true;
        }
        // 8.7.3: the operand named a pattern and the resource behind it did not yield
        // one. The paint is left as it was, so the mark is drawn in the *previous*
        // colour — which is a mark the file did not ask for rather than a missing one,
        // and nothing downstream can tell without this.
        self.doc.record(Decision::violation(
            "8.7.3",
            format!("/{name_str} is named by scn but no pattern could be built from it"),
            "left the current colour in place; the mark is painted in whatever preceded it",
        ));
        true
    }

    /// Handles the `sh` operator (ISO 32000-2 Section 8.7.4.5.2 "Painting shading patterns").
    pub(crate) fn handle_shading_operator(&mut self) -> PdfResult<()> {
        let name = self.pop_name()?;
        let name_str = name.as_str().to_string();
        let res_key = self.doc.arena().intern_name(PdfName::new("Shading"));
        if let Ok(entry) = self.find_resource(&res_key, &name)
            && let Some(shading) = parse_shading_object(&entry, self.doc.arena())
        {
            self.backend.paint_shading(&shading);
            return Ok(());
        }
        // 8.7.4.5.2: `sh` paints the shading over the current clip, so failing to build
        // one means the area is left blank. A blank area and an area a file deliberately
        // left blank are the same pixels.
        self.doc.record(Decision::violation(
            "8.7.4.5.2",
            format!("/{name_str} is named by sh but no shading could be built from it"),
            "painted nothing; the clip region is left as it was",
        ));
        Ok(())
    }

    fn fallback_sc(
        &mut self,
        op: &str,
        count: usize,
        cs: fepdf_model::graphics::ColorSpaceKind,
    ) -> PdfResult<Color> {
        match count {
            1 => Ok(Color::Gray(self.pop_f64()?)),
            3 => {
                let b = self.pop_f64()?;
                let g = self.pop_f64()?;
                let r = self.pop_f64()?;
                Ok(Color::Rgb(r, g, b))
            }
            4 => {
                let k = self.pop_f64()?;
                let y = self.pop_f64()?;
                let m = self.pop_f64()?;
                let c = self.pop_f64()?;
                Ok(Color::Cmyk(c, m, y, k))
            }
            _ => {
                // 8.6.8: the operand count matches no colour model this engine paints in.
                // Black is a colour, so a silent one here is indistinguishable from a
                // black the file asked for — which is exactly how the `/Separation`
                // defect of Phase P survived being looked at.
                self.doc.record(Decision::violation(
                    "8.6.8",
                    format!("{op} with {count} operands in a {cs:?} colour space"),
                    "painted black; no colour model this engine has takes that many \
                     components",
                ));
                Ok(Color::Gray(0.0))
            }
        }
    }
}

/// The family a bare `cs` operand names, for the operands that resolved to no space.
///
/// Several of these cannot legally appear as a `cs` operand at all — `/Separation` and
/// `/Indexed` are always written as resource names — but they are matched here because
/// this is what the interpreter did before it consulted resources, and narrowing it is a
/// change to files that have not been measured rather than a fix to the two that were.
fn kind_from_name(name: &str) -> ColorSpaceKind {
    match name {
        "DeviceGray" | "G" => ColorSpaceKind::DeviceGray,
        "DeviceRGB" | "RGB" => ColorSpaceKind::DeviceRGB,
        "DeviceCMYK" | "CMYK" => ColorSpaceKind::DeviceCMYK,
        "CalGray" => ColorSpaceKind::CalGray,
        "CalRGB" => ColorSpaceKind::CalRGB,
        "Lab" => ColorSpaceKind::Lab,
        "ICCBased" => ColorSpaceKind::ICCBased,
        "Pattern" => ColorSpaceKind::Pattern,
        "Indexed" => ColorSpaceKind::Indexed,
        "Separation" => ColorSpaceKind::Separation,
        "DeviceN" => ColorSpaceKind::DeviceN,
        _ => ColorSpaceKind::Unknown,
    }
}

/// `/Coords`, or `defaults` where the array is absent, short, or carries a non-number.
///
/// The per-element default matters: a radial shading's fourth entry is a radius and 0 is
/// not the same answer as 1 there.
fn read_coords<const N: usize>(
    arena: &fepdf_model::PdfArena,
    dict: &std::collections::BTreeMap<fepdf_model::Handle<PdfName>, fepdf_model::Object>,
    defaults: [f64; N],
) -> [f64; N] {
    let coords_key = arena.intern_name(PdfName::new("Coords"));
    let Some(fepdf_model::Object::Array(ah)) = dict.get(&coords_key).map(|o| o.resolve(arena))
    else {
        return defaults;
    };
    let Some(arr) = arena.get_array(ah) else { return defaults };
    if arr.len() < N {
        return defaults;
    }
    let mut out = defaults;
    for (slot, item) in out.iter_mut().zip(arr.iter()) {
        if let Some(value) = item.resolve(arena).as_f64() {
            *slot = value;
        }
    }
    out
}

pub(crate) fn parse_shading_object(
    obj: &fepdf_model::Object,
    arena: &fepdf_model::PdfArena,
) -> Option<fepdf_model::ShadingSpec> {
    let resolved = obj.resolve(arena);
    let dict = match resolved {
        fepdf_model::Object::Dictionary(dh) => arena.get_dict(dh)?,
        fepdf_model::Object::Stream(dh, _) => arena.get_dict(dh)?,
        _ => return None,
    };

    let st_key = arena.intern_name(PdfName::new("ShadingType"));
    let shading_type = i32::try_from(dict.get(&st_key)?.resolve(arena).as_integer()?).ok()?;

    match shading_type {
        // 8.7.4.5.3 and 8.7.4.5.4 differ in the length of `/Coords` and in nothing else
        // this engine reads: both take their stops from `/Function` and their `/Extend`
        // from the same two booleans.
        2 => Some(fepdf_model::ShadingSpec::Axial(fepdf_model::AxialShading {
            coords: read_coords(arena, &dict, [0.0, 0.0, 1.0, 0.0]),
            stops: shading_stops(&dict, arena),
            extend: read_extend(arena, &dict),
        })),
        3 => Some(fepdf_model::ShadingSpec::Radial(fepdf_model::RadialShading {
            coords: read_coords(arena, &dict, [0.0, 0.0, 0.0, 1.0, 0.0, 1.0]),
            stops: shading_stops(&dict, arena),
            extend: read_extend(arena, &dict),
        })),
        // 8.7.4.5.5 to 8.7.4.5.8: the four mesh types. Unlike 1 to 3 these "shall be
        // represented as streams", so the geometry is in the bytes rather than the
        // dictionary and the shading object has to be a stream to carry any.
        4..=7 => {
            let fepdf_model::Object::Stream(_, ref sd) = resolved else {
                return None;
            };
            let bytes = arena.get_stream_bytes(sd).ok()?;
            let mesh = fepdf_model::graphics::TriangleMesh::parse(
                i64::from(shading_type),
                &dict,
                &bytes,
                arena,
            )?;
            Some(fepdf_model::ShadingSpec::Mesh(mesh))
        }
        // 8.7.4.5.2: the colour at a point is `f(x, y)` over `/Domain`, placed by
        // `/Matrix`. Sampled into a grid here, because the other four types reach the
        // renderer as geometry and a function would make this the one that needs an
        // evaluator on the far side of the contract.
        1 => function_shading(&dict, arena).map(fepdf_model::ShadingSpec::FunctionBased),
        _ => None,
    }
}

/// How finely a Type 1 shading's domain is sampled, per side.
///
/// **A grid of 32 by 32, and it is a sampling.** The 1D case uses 33 points so that a
/// `/Bounds` at a half, quarter or eighth lands exactly on one; there is no equivalent
/// property in two dimensions, and a function with a step between samples is approximated
/// rather than solved. Bigger costs the square: 64 would be four times the cells for a
/// difference no page of the corpus presents — one file of 524 carries a Type 1 shading
/// at all.
const FUNCTION_GRID: u16 = 32;

/// A Type 1 shading, evaluated over its domain (8.7.4.5.2).
fn function_shading(
    dict: &BTreeMap<Handle<PdfName>, Object>,
    arena: &PdfArena,
) -> Option<fepdf_model::graphics::FunctionShading> {
    let func_key = arena.intern_name(PdfName::new("Function"));
    let functions = FunctionSet::parse(dict.get(&func_key)?, arena)?;
    let space = shading_space(dict, arena);
    let domain = read_coords(arena, &dict_with(arena, dict, "Domain"), [0.0, 1.0, 0.0, 1.0]);
    let matrix =
        read_coords(arena, &dict_with(arena, dict, "Matrix"), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    let [x0, x1, y0, y1] = domain;
    // The centre of cell `i`, not its corner: a sample taken at the edge of a domain is
    // the colour of a boundary rather than of the cell it stands for.
    let side = f64::from(FUNCTION_GRID);
    let step = |lo: f64, hi: f64, i: u16| lo + (hi - lo) * (f64::from(i) + 0.5) / side;
    let cells = usize::from(FUNCTION_GRID) * usize::from(FUNCTION_GRID);
    let mut samples = Vec::with_capacity(cells);
    for row in 0..FUNCTION_GRID {
        for column in 0..FUNCTION_GRID {
            let components = functions.eval(&[step(x0, x1, column), step(y0, y1, row)])?;
            let colour = match &space {
                Some(sp) => sp.to_color(&components)?,
                None => ResolvedColorSpace::color_from_components(&components)?,
            };
            samples.push(colour);
        }
    }
    Some(fepdf_model::graphics::FunctionShading {
        domain,
        matrix,
        samples,
        resolution: usize::from(FUNCTION_GRID),
    })
}

/// A dictionary holding just `key`, so `read_coords` can be reused for `/Domain` and
/// `/Matrix` as it is for `/Coords`.
fn dict_with(
    arena: &PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
    key: &str,
) -> BTreeMap<Handle<PdfName>, Object> {
    let mut out = BTreeMap::new();
    if let Some(value) = dict.get(&arena.intern_name(PdfName::new(key))) {
        out.insert(arena.intern_name(PdfName::new("Coords")), value.clone());
    }
    out
}

pub(crate) fn parse_pattern_object(
    obj: &fepdf_model::Object,
    arena: &fepdf_model::PdfArena,
) -> Option<fepdf_model::PatternSpec> {
    let resolved = obj.resolve(arena);
    let dict = match resolved {
        fepdf_model::Object::Dictionary(dh) => arena.get_dict(dh)?,
        fepdf_model::Object::Stream(dh, _) => arena.get_dict(dh)?,
        _ => return None,
    };

    let pt_key = arena.intern_name(PdfName::new("PatternType"));
    let pattern_type = dict.get(&pt_key)?.resolve(arena).as_integer().unwrap_or(1);

    if pattern_type == 2 {
        let sh_key = arena.intern_name(PdfName::new("Shading"));
        let sh_obj = dict.get(&sh_key)?;
        let shading = parse_shading_object(sh_obj, arena)?;
        Some(fepdf_model::PatternSpec::Shading(shading))
    } else {
        let bbox_key = arena.intern_name(PdfName::new("BBox"));
        let bbox = if let Some(fepdf_model::Object::Array(ah)) =
            dict.get(&bbox_key).map(|o| o.resolve(arena))
            && let Some(arr) = arena.get_array(ah)
            && let [x0, y0, x1, y1, ..] = arr.as_slice()
        {
            [
                x0.resolve(arena).as_f64().unwrap_or(0.0),
                y0.resolve(arena).as_f64().unwrap_or(0.0),
                x1.resolve(arena).as_f64().unwrap_or(100.0),
                y1.resolve(arena).as_f64().unwrap_or(100.0),
            ]
        } else {
            [0.0, 0.0, 100.0, 100.0]
        };

        let xs_key = arena.intern_name(PdfName::new("XStep"));
        let ys_key = arena.intern_name(PdfName::new("YStep"));
        let x_step = dict.get(&xs_key).and_then(|o| o.resolve(arena).as_f64()).unwrap_or(100.0);
        let y_step = dict.get(&ys_key).and_then(|o| o.resolve(arena).as_f64()).unwrap_or(100.0);

        let content_bytes = match resolved {
            fepdf_model::Object::Stream(_, ref sd) => {
                arena.get_stream_bytes(sd).map(|b| b.to_vec()).unwrap_or_default()
            }
            _ => Vec::new(),
        };

        Some(fepdf_model::PatternSpec::Tiling { bbox, x_step, y_step, matrix: None, content_bytes })
    }
}

/// How many points a shading's function is sampled at to build the stop list.
///
/// The renderer interpolates linearly between stops, so a piecewise-linear function is
/// reproduced **exactly** when its breakpoints land on the grid: 33 points puts a stop
/// on every 1/32, covering the halves, quarters and eighths that `/Bounds` are written
/// at in practice. It is a sampling and says so — a type 4 program with a step somewhere
/// else is approximated, not solved.
const SHADING_SAMPLES: u16 = 33;

/// The colour stops of a shading, from its `/Function` evaluated across its `/Domain`.
///
/// Falls back to reading `/C0` and `/C1` off the function dictionary when the function
/// will not parse. That fallback *was* the whole implementation, and it is why a
/// three-stop gradient rendered black-to-white: a stitching function has neither key,
/// because its colours live one level down in `/Functions`.
fn shading_stops(
    dict: &BTreeMap<Handle<PdfName>, Object>,
    arena: &PdfArena,
) -> Vec<fepdf_model::ColorStop> {
    let func_key = arena.intern_name(PdfName::new("Function"));
    let func_obj = dict.get(&func_key);
    if let Some(stops) = sampled_stops(dict, func_obj, arena) {
        return stops;
    }
    endpoint_stops(func_obj, arena)
}

fn sampled_stops(
    dict: &BTreeMap<Handle<PdfName>, Object>,
    func_obj: Option<&Object>,
    arena: &PdfArena,
) -> Option<Vec<fepdf_model::ColorStop>> {
    let functions = FunctionSet::parse(func_obj?, arena)?;
    let space = shading_space(dict, arena);
    let (t0, t1) = shading_domain(dict, arena);
    let mut stops = Vec::with_capacity(usize::from(SHADING_SAMPLES));
    for i in 0..SHADING_SAMPLES {
        let offset = f32::from(i) / f32::from(SHADING_SAMPLES - 1);
        let t = t0 + f64::from(offset) * (t1 - t0);
        let components = functions.eval(&[t])?;
        // With a `/ColorSpace` the components mean what that space says — including a
        // `/Separation`, whose own tint transform then runs on this function's output.
        // Without one, the component count is all there is to go on.
        let color = match &space {
            Some(sp) => sp.to_color(&components)?,
            None => ResolvedColorSpace::color_from_components(&components)?,
        };
        stops.push(fepdf_model::ColorStop::new(offset, color));
    }
    Some(stops)
}

fn shading_space(
    dict: &BTreeMap<Handle<PdfName>, Object>,
    arena: &PdfArena,
) -> Option<ResolvedColorSpace> {
    let key = arena.intern_name(PdfName::new("ColorSpace"));
    ResolvedColorSpace::parse(dict.get(&key)?, arena)
}

/// A shading's `/Domain`, `[t0 t1]`, defaulting to `[0 1]` (Table 78).
fn shading_domain(dict: &BTreeMap<Handle<PdfName>, Object>, arena: &PdfArena) -> (f64, f64) {
    let key = arena.intern_name(PdfName::new("Domain"));
    let pair = dict
        .get(&key)
        .map(|o| o.resolve(arena))
        .and_then(|o| o.as_array())
        .and_then(|ah| arena.get_array(ah))
        .and_then(|a| match a.as_slice() {
            [low, high, ..] => Some((low.resolve(arena).as_f64()?, high.resolve(arena).as_f64()?)),
            _ => None,
        });
    pair.unwrap_or((0.0_f64, 1.0_f64))
}

fn endpoint_stops(
    func_obj: Option<&fepdf_model::Object>,
    arena: &fepdf_model::PdfArena,
) -> Vec<fepdf_model::ColorStop> {
    let mut stops = Vec::new();
    if let Some(obj) = func_obj {
        let resolved = obj.resolve(arena);
        if let fepdf_model::Object::Dictionary(dh) = resolved
            && let Some(fdict) = arena.get_dict(dh)
        {
            let c0_key = arena.intern_name(PdfName::new("C0"));
            let c1_key = arena.intern_name(PdfName::new("C1"));
            let c0_col = parse_color_from_array(fdict.get(&c0_key), arena)
                .unwrap_or(Color::Rgb(0.0, 0.0, 0.0));
            let c1_col = parse_color_from_array(fdict.get(&c1_key), arena)
                .unwrap_or(Color::Rgb(1.0, 1.0, 1.0));
            stops.push(fepdf_model::ColorStop::new(0.0, c0_col));
            stops.push(fepdf_model::ColorStop::new(1.0, c1_col));
        }
    }

    if stops.is_empty() {
        stops.push(fepdf_model::ColorStop::new(0.0, Color::Rgb(0.0, 0.0, 0.0)));
        stops.push(fepdf_model::ColorStop::new(1.0, Color::Rgb(1.0, 1.0, 1.0)));
    }
    stops
}

fn parse_color_from_array(
    obj: Option<&fepdf_model::Object>,
    arena: &fepdf_model::PdfArena,
) -> Option<Color> {
    let resolved = obj?.resolve(arena);
    if let fepdf_model::Object::Array(ah) = resolved
        && let Some(arr) = arena.get_array(ah)
    {
        let num = |o: &fepdf_model::Object| o.resolve(arena).as_f64();
        match arr.as_slice() {
            [gray] => Some(Color::Gray(num(gray)?)),
            [red, green, blue] => Some(Color::Rgb(num(red)?, num(green)?, num(blue)?)),
            [cyan, magenta, yellow, black] => {
                Some(Color::Cmyk(num(cyan)?, num(magenta)?, num(yellow)?, num(black)?))
            }
            _ => None,
        }
    } else {
        None
    }
}
