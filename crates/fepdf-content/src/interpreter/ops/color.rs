use crate::RenderBackend;
use crate::interpreter::Interpreter;
use fepdf_model::color::ResolvedColorSpace;
use fepdf_model::function::FunctionSet;
use fepdf_model::graphics::{Color, ColorSpaceKind};
use fepdf_model::interpretation::Decision;
use fepdf_model::{Handle, Object, Paint, PdfArena, PdfName, PdfResult};
use std::collections::BTreeMap;
use std::sync::Arc;

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
        if is_fill {
            self.state.fill_color_space = kind;
            self.state.fill_space = space.map(Arc::new);
        } else {
            self.state.stroke_color_space = kind;
            self.state.stroke_space = space.map(Arc::new);
        }
        Ok(())
    }

    /// Resolves a `cs` operand: a device space name, or a key into the page's
    /// `/ColorSpace` resources (8.6.3).
    ///
    /// The second half is new. Every `/Separation` and every `/ICCBased` space is
    /// written as a resource name, so before this the operand never matched anything and
    /// the space came out `Unknown` — after which `scn` guessed the colour model from
    /// how many operands there were. For a separation that guess is one number, read as
    /// a grey level, which inverts the tint.
    fn resolve_color_space(&self, name: &PdfName) -> Option<ResolvedColorSpace> {
        if let Some(device) = ResolvedColorSpace::from_family(name.as_str()) {
            return Some(device);
        }
        let key = self.doc.arena().intern_name(PdfName::new("ColorSpace"));
        // `find_resource` reports a missing entry as an error. Here that is an ordinary
        // absence rather than a failure — the operand is simply not a resource name — so
        // it becomes `None` and the caller falls back to reading the name itself.
        let Ok(entry) = self.find_resource(&key, name) else {
            return None;
        };
        let space = ResolvedColorSpace::parse(&entry, self.doc.arena())?;
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
        let c = Color::Gray(gray);
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
        let c = Color::Rgb(r, g, b);
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
        let col = Color::Cmyk(c, m, y, k);
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
        2 => {
            // Type 2: Axial (Linear)
            let coords_key = arena.intern_name(PdfName::new("Coords"));
            let coords = if let Some(fepdf_model::Object::Array(ah)) =
                dict.get(&coords_key).map(|o| o.resolve(arena))
                && let Some(arr) = arena.get_array(ah)
                && arr.len() >= 4
            {
                [
                    arr[0].resolve(arena).as_f64().unwrap_or(0.0),
                    arr[1].resolve(arena).as_f64().unwrap_or(0.0),
                    arr[2].resolve(arena).as_f64().unwrap_or(1.0),
                    arr[3].resolve(arena).as_f64().unwrap_or(0.0),
                ]
            } else {
                [0.0, 0.0, 1.0, 0.0]
            };

            let extend_key = arena.intern_name(PdfName::new("Extend"));
            let extend = if let Some(fepdf_model::Object::Array(ah)) =
                dict.get(&extend_key).map(|o| o.resolve(arena))
                && let Some(arr) = arena.get_array(ah)
                && arr.len() >= 2
            {
                [
                    arr[0].resolve(arena).as_bool().unwrap_or(true),
                    arr[1].resolve(arena).as_bool().unwrap_or(true),
                ]
            } else {
                [true, true]
            };

            let stops = shading_stops(&dict, arena);

            Some(fepdf_model::ShadingSpec::Axial(fepdf_model::AxialShading {
                coords,
                stops,
                extend,
            }))
        }
        3 => {
            // Type 3: Radial
            let coords_key = arena.intern_name(PdfName::new("Coords"));
            let coords = if let Some(fepdf_model::Object::Array(ah)) =
                dict.get(&coords_key).map(|o| o.resolve(arena))
                && let Some(arr) = arena.get_array(ah)
                && arr.len() >= 6
            {
                [
                    arr[0].resolve(arena).as_f64().unwrap_or(0.0),
                    arr[1].resolve(arena).as_f64().unwrap_or(0.0),
                    arr[2].resolve(arena).as_f64().unwrap_or(0.0),
                    arr[3].resolve(arena).as_f64().unwrap_or(1.0),
                    arr[4].resolve(arena).as_f64().unwrap_or(0.0),
                    arr[5].resolve(arena).as_f64().unwrap_or(1.0),
                ]
            } else {
                [0.0, 0.0, 0.0, 1.0, 0.0, 1.0]
            };

            let extend_key = arena.intern_name(PdfName::new("Extend"));
            let extend = if let Some(fepdf_model::Object::Array(ah)) =
                dict.get(&extend_key).map(|o| o.resolve(arena))
                && let Some(arr) = arena.get_array(ah)
                && arr.len() >= 2
            {
                [
                    arr[0].resolve(arena).as_bool().unwrap_or(true),
                    arr[1].resolve(arena).as_bool().unwrap_or(true),
                ]
            } else {
                [true, true]
            };

            let stops = shading_stops(&dict, arena);

            Some(fepdf_model::ShadingSpec::Radial(fepdf_model::RadialShading {
                coords,
                stops,
                extend,
            }))
        }
        _ => None,
    }
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
            && arr.len() >= 4
        {
            [
                arr[0].resolve(arena).as_f64().unwrap_or(0.0),
                arr[1].resolve(arena).as_f64().unwrap_or(0.0),
                arr[2].resolve(arena).as_f64().unwrap_or(100.0),
                arr[3].resolve(arena).as_f64().unwrap_or(100.0),
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
        .filter(|a| a.len() >= 2)
        .and_then(|a| Some((a[0].resolve(arena).as_f64()?, a[1].resolve(arena).as_f64()?)));
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
        match arr.len() {
            1 => {
                let g = arr[0].resolve(arena).as_f64()?;
                Some(Color::Gray(g))
            }
            3 => {
                let r = arr[0].resolve(arena).as_f64()?;
                let g = arr[1].resolve(arena).as_f64()?;
                let b = arr[2].resolve(arena).as_f64()?;
                Some(Color::Rgb(r, g, b))
            }
            4 => {
                let c = arr[0].resolve(arena).as_f64()?;
                let m = arr[1].resolve(arena).as_f64()?;
                let y = arr[2].resolve(arena).as_f64()?;
                let k = arr[3].resolve(arena).as_f64()?;
                Some(Color::Cmyk(c, m, y, k))
            }
            _ => None,
        }
    } else {
        None
    }
}
