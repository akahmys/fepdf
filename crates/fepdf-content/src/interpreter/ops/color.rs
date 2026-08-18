use crate::interpreter::Interpreter;
use fepdf_model::graphics::Color;
use fepdf_model::{Paint, PdfName, PdfResult};

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
        use fepdf_model::graphics::ColorSpaceKind;
        let is_fill = op == "cs";
        let name = self.pop_name()?;
        let cs = match name.as_str() {
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
        };
        if is_fill {
            self.state.fill_color_space = cs;
        } else {
            self.state.stroke_color_space = cs;
        }
        Ok(())
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
        use fepdf_model::graphics::ColorSpaceKind;
        let is_fill = op == "sc" || op == "scn";
        let cs = if is_fill { self.state.fill_color_space } else { self.state.stroke_color_space };

        // 8.6.8.2: in a Pattern colour space the operands are an optional set of
        // numbers followed by a *name*, which keys the resource dictionary's `/Pattern`
        // subdictionary.
        if self.handle_pattern_color(is_fill) {
            return Ok(());
        }

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

        if is_fill {
            self.state.fill_color = col;
            self.backend.set_fill_color(col);
        } else {
            self.state.stroke_color = col;
            self.backend.set_stroke_color(col);
        }
        Ok(())
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
        log::warn!(
            "[SDK] pattern /{name_str} is named but could not be parsed; colour left unchanged"
        );
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
        log::warn!("[SDK] Shading /{name_str} could not be resolved or parsed");
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
                log::warn!("[SDK] Unhandled {op} with {count} operands in CS {cs:?}");
                // Return Gray(0) as ultimate fallback to avoid stopping execution
                Ok(Color::Gray(0.0))
            }
        }
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

            let func_key = arena.intern_name(PdfName::new("Function"));
            let func_obj = dict.get(&func_key);
            let stops = parse_shading_stops(func_obj, arena);

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

            let func_key = arena.intern_name(PdfName::new("Function"));
            let func_obj = dict.get(&func_key);
            let stops = parse_shading_stops(func_obj, arena);

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

fn parse_shading_stops(
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
