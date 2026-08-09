use crate::interpreter::Interpreter;
use fepdf_core::object::sublimation::Command;
use fepdf_core::{Handle, Object, PdfError, PdfName, PdfResult};
use std::collections::BTreeMap;

impl Interpreter<'_> {
    pub(crate) fn handle_xobject_operator(&mut self) -> PdfResult<()> {
        let name = self.pop_name()?;
        let entry =
            self.find_resource(&self.doc.arena().intern_name(PdfName::new("XObject")), &name)?;
        let xobj = entry.resolve(self.doc.arena());
        if let Object::Stream(dh, _) = xobj
            && let Some(dict) = self.doc.arena().get_dict(dh)
        {
            let subtype_key = self.doc.arena().intern_name(PdfName::new("Subtype"));
            if let Some(sub) =
                dict.get(&subtype_key).and_then(|o| o.resolve(self.doc.arena()).as_name())
            {
                let sub_name = self
                    .doc
                    .arena()
                    .get_name(sub)
                    .ok_or_else(|| PdfError::Other("Subtype name not found".into()))?;
                let sub_str = sub_name.as_str();
                let sd = if let Object::Stream(_, ref sd) = xobj {
                    sd
                } else {
                    return Ok(());
                };

                match sub_str {
                    "Image" => self.render_image_xobject(&dict, sd)?,
                    "Form" => match sd.as_ref() {
                        fepdf_core::object::SublimatedData::Commands { items: cmds, .. } => {
                            self.execute_form_commands(&dict, cmds)?;
                        }
                        // Forms not pre-parsed into commands are replayed from raw bytes.
                        fepdf_core::object::SublimatedData::Image { .. }
                        | fepdf_core::object::SublimatedData::Compressed { .. }
                        | fepdf_core::object::SublimatedData::Raw(_) => {
                            let bytes = self.doc.arena().get_stream_bytes(sd)?;
                            self.render_form_xobject(&dict, &bytes)?;
                        }
                    },
                    _ => {}
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::many_single_char_names)]
    pub(crate) fn execute_form_commands(
        &mut self,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        cmds: &[Command],
    ) -> PdfResult<()> {
        // 1. Save state
        self.state_stack.push(self.state.clone());
        self.backend.push_state();

        // 2. Apply Matrix
        let matrix_key = self.doc.arena().intern_name(PdfName::new("Matrix"));
        if let Some(Object::Array(h)) = dict.get(&matrix_key).map(|o| o.resolve(self.doc.arena()))
            && let Some(arr) = self.doc.arena().get_array(h)
            && arr.len() == 6
        {
            let a = arr[0].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let b = arr[1].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let c = arr[2].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let d = arr[3].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let e = arr[4].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let f = arr[5].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let m = fepdf_core::graphics::Matrix::new(a, b, c, d, e, f);
            self.state.ctm = self.state.ctm.concat(&m);
            self.backend.transform(m.as_affine());
        }

        // 2.5 Apply BBox clipping
        let bbox_key = self.doc.arena().intern_name(PdfName::new("BBox"));
        if let Some(Object::Array(h)) = dict.get(&bbox_key).map(|o| o.resolve(self.doc.arena()))
            && let Some(arr) = self.doc.arena().get_array(h)
            && arr.len() == 4
        {
            let x1 = arr[0].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let y1 = arr[1].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let x2 = arr[2].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let y2 = arr[3].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);

            let mut path = kurbo::BezPath::new();
            path.move_to((x1, y1));
            path.line_to((x2, y1));
            path.line_to((x2, y2));
            path.line_to((x1, y2));
            path.close_path();

            self.backend.push_clip(&path, fepdf_core::graphics::WindingRule::NonZero);
            self.state.clip_count += 1;
        }

        // 3. Setup Resources
        let mut pushed = false;
        let res_key = self.doc.arena().intern_name(PdfName::new("Resources"));
        if let Some(Object::Dictionary(h)) = dict.get(&res_key).map(|o| o.resolve(self.doc.arena()))
        {
            self.resource_stack.push(h);
            pushed = true;
        }

        // 4. Recursive Execute
        self.execute_commands(cmds)?;

        // 5. Cleanup
        if pushed {
            self.resource_stack.pop();
        }
        let current_clips = self.state.clip_count;
        if let Some(old) = self.state_stack.pop() {
            let target_clips = old.clip_count;
            if current_clips > target_clips {
                for _ in 0..(current_clips - target_clips) {
                    self.backend.pop_clip();
                }
            }
            self.state = old;
            self.backend.pop_state();
        }

        Ok(())
    }

    #[allow(clippy::many_single_char_names)]
    pub(crate) fn render_form_xobject(
        &mut self,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        data: &[u8],
    ) -> PdfResult<()> {
        let decoded = self.doc.arena().process_filters(data, dict)?;
        // 1. Save state
        self.state_stack.push(self.state.clone());
        self.backend.push_state();

        // 2. Apply Matrix
        let matrix_key = self.doc.arena().intern_name(PdfName::new("Matrix"));
        if let Some(Object::Array(h)) = dict.get(&matrix_key).map(|o| o.resolve(self.doc.arena()))
            && let Some(arr) = self.doc.arena().get_array(h)
            && arr.len() == 6
        {
            let a = arr[0].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let b = arr[1].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let c = arr[2].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let d = arr[3].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let e = arr[4].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let f = arr[5].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let m = fepdf_core::graphics::Matrix::new(a, b, c, d, e, f);
            self.state.ctm = self.state.ctm.concat(&m);
            self.backend.transform(m.as_affine());
        }

        // 2.5 Apply BBox clipping (ISO 32000-2 8.10.1)
        let bbox_key = self.doc.arena().intern_name(PdfName::new("BBox"));
        if let Some(Object::Array(h)) = dict.get(&bbox_key).map(|o| o.resolve(self.doc.arena()))
            && let Some(arr) = self.doc.arena().get_array(h)
            && arr.len() == 4
        {
            let x1 = arr[0].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let y1 = arr[1].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let x2 = arr[2].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
            let y2 = arr[3].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);

            let mut path = kurbo::BezPath::new();
            path.move_to((x1, y1));
            path.line_to((x2, y1));
            path.line_to((x2, y2));
            path.line_to((x1, y2));
            path.close_path();

            self.backend.push_clip(&path, fepdf_core::graphics::WindingRule::NonZero);
            self.state.clip_count += 1;
        }

        // 3. Setup Resources
        let mut pushed = false;
        let res_key = self.doc.arena().intern_name(PdfName::new("Resources"));
        if let Some(Object::Dictionary(h)) = dict.get(&res_key).map(|o| o.resolve(self.doc.arena()))
        {
            self.resource_stack.push(h);
            pushed = true;
        }

        // 4. Recursive Execute
        self.execute_raw(&decoded)?;

        // 5. Cleanup
        if pushed {
            self.resource_stack.pop();
        }
        let current_clips = self.state.clip_count;
        if let Some(old) = self.state_stack.pop() {
            let target_clips = old.clip_count;
            if current_clips > target_clips {
                for _ in 0..(current_clips - target_clips) {
                    self.backend.pop_clip();
                }
            }
            self.state = old;
            self.backend.pop_state();
        }

        Ok(())
    }

    pub(crate) fn render_image_xobject(
        &mut self,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        sd: &fepdf_core::object::SublimatedData,
    ) -> PdfResult<()> {
        let width_key = self.doc.arena().intern_name(PdfName::new("Width"));
        let height_key = self.doc.arena().intern_name(PdfName::new("Height"));

        let (width, height, format, decoded) =
            if let fepdf_core::object::SublimatedData::Image { width, height, format, data } = sd {
                (*width, *height, *format, bytes::Bytes::copy_from_slice(data))
            } else {
                let data = self.doc.arena().get_stream_bytes(sd)?;
                let w = u32::try_from(
                    dict.get(&width_key)
                        .and_then(|o| o.resolve(self.doc.arena()).as_integer())
                        .unwrap_or(0),
                )
                .unwrap_or(0);
                let h = u32::try_from(
                    dict.get(&height_key)
                        .and_then(|o| o.resolve(self.doc.arena()).as_integer())
                        .unwrap_or(0),
                )
                .unwrap_or(0);

                let im_key = self.doc.arena().intern_name(PdfName::new("ImageMask"));
                let is_mask = dict
                    .get(&im_key)
                    .and_then(|o| o.resolve(self.doc.arena()).as_bool())
                    .unwrap_or(false);

                let format = if is_mask {
                    let decode_key = self.doc.arena().intern_name(PdfName::new("Decode"));
                    let mut invert_mask = false;
                    if let Some(decode_obj) = dict.get(&decode_key) {
                        let decode_resolved = decode_obj.resolve(self.doc.arena());
                        if let Some(arr_h) = decode_resolved.as_array() {
                            if let Some(arr) = self.doc.arena().get_array(arr_h) {
                                if arr.len() >= 2 {
                                    let first =
                                        arr[0].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
                                    if first > 0.5 {
                                        invert_mask = true;
                                    }
                                }
                            }
                        }
                    }
                    if invert_mask {
                        fepdf_core::graphics::PixelFormat::MonoMaskInverted
                    } else {
                        fepdf_core::graphics::PixelFormat::MonoMask
                    }
                } else {
                    self.detect_pixel_format(dict)
                };

                let decoded = self.doc.arena().process_filters(&data, dict)?;
                (w, h, format, decoded)
            };

        let smask_key = self.doc.arena().intern_name(PdfName::new("SMask"));
        let smask_data = if let Some(smask_obj) = dict.get(&smask_key) {
            let smask_stream = smask_obj.resolve(self.doc.arena());
            if let Object::Stream(dh, ref sd) = smask_stream {
                let smask_dict = self
                    .doc
                    .arena()
                    .get_dict(dh)
                    .ok_or_else(|| PdfError::Other("SMask dictionary not found".into()))?;
                let (sw, sh, sf, smask_decoded) =
                    if let fepdf_core::object::SublimatedData::Image {
                        width,
                        height,
                        format,
                        data,
                    } = sd.as_ref()
                    {
                        (*width, *height, *format, bytes::Bytes::copy_from_slice(data))
                    } else {
                        let sw = u32::try_from(
                            smask_dict
                                .get(&width_key)
                                .and_then(|o| o.resolve(self.doc.arena()).as_integer())
                                .unwrap_or(0),
                        )
                        .unwrap_or(0);
                        let sh = u32::try_from(
                            smask_dict
                                .get(&height_key)
                                .and_then(|o| o.resolve(self.doc.arena()).as_integer())
                                .unwrap_or(0),
                        )
                        .unwrap_or(0);
                        let smask_bytes = self.doc.arena().get_stream_bytes(sd)?;
                        let smask_decoded =
                            self.doc.arena().process_filters(&smask_bytes, &smask_dict)?;
                        (sw, sh, self.detect_pixel_format(&smask_dict), smask_decoded)
                    };

                Some(fepdf_render::SMaskData {
                    data: smask_decoded.to_vec(),
                    width: sw,
                    height: sh,
                    format: sf,
                })
            } else {
                None
            }
        } else {
            None
        };

        self.backend.draw_image(&decoded, width, height, format, smask_data);
        Ok(())
    }

    fn detect_pixel_format(
        &self,
        dict: &BTreeMap<Handle<PdfName>, Object>,
    ) -> fepdf_core::graphics::PixelFormat {
        let cs_key = self.doc.arena().intern_name(PdfName::new("ColorSpace"));
        let cs_obj = dict.get(&cs_key).map(|o: &Object| o.resolve(self.doc.arena()));

        let cs_name = match cs_obj {
            Some(Object::Name(h)) => self.doc.arena().get_name(h).map(|n| n.as_str().to_string()),
            Some(Object::Array(h)) => {
                // For Array color spaces like [/Indexed /DeviceRGB ...], use the first element
                self.doc
                    .arena()
                    .get_array(h)
                    .and_then(|a| a.first().cloned())
                    .and_then(|o| o.resolve(self.doc.arena()).as_name())
                    .and_then(|nh| self.doc.arena().get_name(nh))
                    .map(|n| n.as_str().to_string())
            }
            _ => None,
        }
        .unwrap_or_else(|| "DeviceRGB".to_string());

        match cs_name.as_str() {
            "DeviceGray" | "G" | "Gray" => fepdf_core::graphics::PixelFormat::Gray8,
            "DeviceCMYK" | "CMYK" => fepdf_core::graphics::PixelFormat::Cmyk8,
            "Indexed" | "I" => {
                // TODO(RR-15-EXT): Transition from fallback RGB8 and implement proper index expansion lookup mapping.
                fepdf_core::graphics::PixelFormat::Rgb8
            }
            _ => fepdf_core::graphics::PixelFormat::Rgb8,
        }
    }
}
