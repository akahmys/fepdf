use crate::RenderBackend;
use crate::interpreter::Interpreter;
use fepdf_model::interpretation::Decision;
use fepdf_model::{FromPdfObject, Handle, LineCap, LineJoin, Matrix, Object, PdfName, PdfResult};
use std::collections::BTreeMap;

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
            // Consumed, not modelled. Flatness tolerance and rendering intent are the
            // two graphics-state parameters this engine keeps nothing for — unlike `w`,
            // `J`, `j`, `M` and `d` above — and until now they consumed *no* operands
            // either, because they fell into the arm below.
            //
            // An operator that does not consume its operands leaves them for the next
            // one. `/Perceptual ri` ahead of a `scn` made the colour operator count one
            // operand too many and take its fallback arm, which is how the cyan square
            // of `UnknownFilter-ICC.pdf` came to be painted black. Found by measuring
            // that file, not by reading this function.
            //
            // `pop` rather than `pop_f64`: the arity is what is being fixed here, and a
            // malformed stream that writes `i` with nothing under it should not start
            // failing a whole page over an operator whose value is discarded anyway.
            "i" | "ri" => {
                self.stack.pop();
            }
            _ => {}
        }
        Ok(())
    }

    /// Table 57's `/Font`: `[font size]`, where the font is an indirect reference to a
    /// font dictionary rather than a resource name.
    ///
    /// Ignored until `NegativeFontSize.pdf` was measured — its page sets the font this
    /// way before it ever reaches a `Tf`, so `show_text` found no font, failed, and took
    /// the whole content stream with it. PDFKit read 327 characters from that page and
    /// this engine read none.
    fn apply_gs_font(&mut self, gs_dict: &BTreeMap<Handle<PdfName>, Object>) {
        let key = self.doc.arena().intern_name(PdfName::new("Font"));
        let Some(entry) = gs_dict.get(&key) else { return };
        let Some(pair) = entry.resolve(self.doc.arena()).as_array() else { return };
        let Some(items) = self.doc.arena().get_array(pair) else { return };
        let [font, size] = &items[..] else { return };
        let Some(handle) = font.as_reference() else { return };

        self.state.text_state.font_ref = Some(handle);
        // At most one of the two is ever set: whichever of `Tf` and `gs` came last wins.
        self.state.text_state.font = None;
        if let Some(size) = size.resolve(self.doc.arena()).as_f64() {
            self.state.text_state.font_size = size;
        }
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
            self.apply_gs_font(&gs_dict);

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
                        // Read into the state and used by nothing: `grep` finds this
                        // write and no reader, and `RenderBackend` has no soft-mask
                        // entry point. Measured with a `/S /Luminosity` mask whose group
                        // paints solid black — 11.6.5.2 makes that mask 0 everywhere, so
                        // the content it covers contributes nothing — and the covered
                        // rectangle was filled at full strength with no decision beside
                        // it. The same shape as a `render_page` that returns `Ok(())`
                        // having drawn nothing, in the other direction.
                        self.doc.record(Decision::violation(
                            "11.6.5.2",
                            "an /SMask soft mask in an /ExtGState".to_string(),
                            "drew the content unmasked; this engine has no soft-mask                              path, so everything the mask would have hidden is visible",
                        ));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
