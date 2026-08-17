use crate::interpreter::Interpreter;
use fepdf_model::PdfResult;
use fepdf_model::graphics::Color;

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
        // subdictionary. `fallback_sc` dispatched on how many operands there were and
        // not on what they are, so `/P1 scn` — one operand — was read as one grey
        // component and failed. Six pages of `samples/fy05.pdf` were unreadable for it.
        if self.pattern_name().is_some() {
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

    /// Consumes a pattern name and the components an uncoloured pattern precedes it
    /// with, returning the name when this operator named one.
    ///
    /// The colour is deliberately left alone. `Color` has no way to say "a pattern", and
    /// adding a variant nothing can render would be a container built before its
    /// contents; setting black instead would be a definite wrong answer where the
    /// previous colour is merely an unchanged one. What matters is that the operands are
    /// consumed, so the next operator does not read them as its own.
    fn pattern_name(&mut self) -> Option<String> {
        let named = matches!(self.stack.last(), Some(fepdf_model::Object::Name(_)));
        if !named {
            return None;
        }
        let name = self.pop_name().ok()?;
        // `c1 … cn /Name scn` for an uncoloured pattern: the components are this
        // operator's too, and leaving them would corrupt the next one's operands.
        while matches!(
            self.stack.last(),
            Some(fepdf_model::Object::Integer(_) | fepdf_model::Object::Real(_))
        ) {
            self.stack.pop();
        }
        let name = name.as_str().to_string();
        log::warn!("[SDK] pattern /{name} is named but not painted; the colour is unchanged");
        Some(name)
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
