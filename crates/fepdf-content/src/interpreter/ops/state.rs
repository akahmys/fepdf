use crate::interpreter::Interpreter;
use fepdf_model::{FromPdfObject, LineCap, LineJoin, Matrix, Object, PdfName, PdfResult};

const MAX_GSTATE_STACK_DEPTH: usize = 64;

impl Interpreter<'_> {
    #[allow(clippy::many_single_char_names)]
    pub(crate) fn handle_state_operator(&mut self, op: &str) -> PdfResult<()> {
        match op {
            "q" => {
                if self.state_stack.len() < MAX_GSTATE_STACK_DEPTH {
                    self.state_stack.push(self.state.clone());
                    self.backend.push_state();
                }
            }
            "Q" => {
                let current_clips = self.state.clip_count;
                if let Some(old) = self.state_stack.pop() {
                    let target_clips = old.clip_count;

                    // Restore clip stack by popping the difference BEFORE popping state
                    if current_clips > target_clips {
                        for _ in 0..(current_clips - target_clips) {
                            self.backend.pop_clip();
                        }
                    }

                    self.state = old;
                    self.backend.pop_state();
                    self.update_backend_transform();
                }
            }
            "cm" => {
                let f = self.pop_f64()?;
                let e = self.pop_f64()?;
                let d = self.pop_f64()?;
                let c = self.pop_f64()?;
                let b = self.pop_f64()?;
                let a = self.pop_f64()?;
                let mat = Matrix::new(a, b, c, d, e, f);
                self.state.ctm = self.state.ctm.concat(&mat);
                self.update_backend_transform();
            }
            "gs" => {
                let name = self.pop_name()?;
                self.handle_gs_operator(&name)?;
            }
            "w" => {
                self.state.stroke_style.width = self.pop_f64()?;
            }
            "J" => {
                self.state.stroke_style.cap = LineCap::from_i64(self.pop_i64()?);
            }
            "j" => {
                self.state.stroke_style.join = LineJoin::from_i64(self.pop_i64()?);
            }
            "M" => {
                self.state.stroke_style.miter_limit = self.pop_f64()?;
            }
            "d" => {
                let phase = self.pop_f64()?;
                let arr_h = self.pop_array()?;
                let mut dash = Vec::new();
                if let Some(arr) = self.doc.arena().get_array(arr_h) {
                    for item in arr {
                        if let Some(f) = item.as_f64() {
                            dash.push(f);
                        }
                    }
                }
                self.state.stroke_style.dash_pattern = Some((dash, phase));
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_gs_operator(&mut self, name: &PdfName) -> PdfResult<()> {
        let entry =
            self.find_resource(&self.doc.arena().intern_name(PdfName::new("ExtGState")), name)?;
        let gs_obj = entry.resolve(self.doc.arena());
        if let Object::Dictionary(h) = gs_obj
            && let Some(gs_dict) = self.doc.arena().get_dict(h)
        {
            let ca_key = self.doc.arena().intern_name(PdfName::new("ca"));
            let ca_up_key = self.doc.arena().intern_name(PdfName::new("CA"));
            let bm_key = self.doc.arena().intern_name(PdfName::new("BM"));
            let smask_key = self.doc.arena().intern_name(PdfName::new("SMask"));

            if let Some(ca) = gs_dict.get(&ca_key).and_then(|o| o.as_f64()) {
                self.state.fill_alpha = ca;
                self.backend.set_fill_alpha(ca);
            }
            if let Some(ca_up) = gs_dict.get(&ca_up_key).and_then(|o| o.as_f64()) {
                self.state.stroke_alpha = ca_up;
                self.backend.set_stroke_alpha(ca_up);
            }
            if let Some(bm_obj) = gs_dict.get(&bm_key)
                && let Ok(bm) = fepdf_model::graphics::BlendMode::from_pdf_object(
                    bm_obj.resolve(self.doc.arena()),
                    self.doc.arena(),
                )
            {
                self.state.blend_mode = bm;
                self.backend.set_blend_mode(bm);
            }
            if let Some(smask_obj) = gs_dict.get(&smask_key) {
                let resolved = smask_obj.resolve(self.doc.arena());
                match resolved {
                    Object::Name(n) => {
                        if self.doc.arena().get_name(n).is_some_and(|nn| nn.as_str() == "None") {
                            self.state.smask = None;
                        }
                    }
                    Object::Dictionary(_) => {
                        self.state.smask = Some(resolved);
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
