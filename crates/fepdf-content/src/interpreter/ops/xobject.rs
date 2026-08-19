use crate::interpreter::Interpreter;
use fepdf_model::object::sublimation::Command;
use fepdf_model::{Handle, Object, PdfError, PdfName, PdfResult};
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
                    // An image that will not decode is skipped, not propagated.
                    //
                    // Every remaining text failure in the external corpus was this: a
                    // `/CCITTFaxDecode` or `/JPXDecode` image XObject, and in one file
                    // `/XXXDecode` — a filter invented for the test, which no codec can
                    // ever handle. An image carries no text, so decoding one would not
                    // produce any; what the failure did was abort the content stream and
                    // take the page's *real* text with it.
                    //
                    // Not recorded as a `Decision`: the interpreter holds `&Document` and
                    // the log needs `&mut`, so reaching it would change `extract_text`'s
                    // signature across the SDK to carry a note about a picture. That is a
                    // real gap in §5.3's coverage and is written down rather than hidden.
                    "Image" => {
                        if let Err(e) = self.render_image_xobject(&dict, sd) {
                            log::debug!("[content] image {name:?} not drawn: {e:?}");
                        }
                    }
                    "Form" => match sd.as_ref() {
                        fepdf_model::object::SublimatedData::Commands { items: cmds, .. } => {
                            self.execute_form_commands(&dict, cmds)?;
                        }
                        // Forms not pre-parsed into commands are replayed from raw bytes.
                        fepdf_model::object::SublimatedData::Image { .. }
                        | fepdf_model::object::SublimatedData::Compressed { .. }
                        | fepdf_model::object::SublimatedData::Raw(_) => {
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
            let m = fepdf_model::graphics::Matrix::new(a, b, c, d, e, f);
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

            self.backend.push_clip(&path, fepdf_model::graphics::WindingRule::NonZero);
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
            let m = fepdf_model::graphics::Matrix::new(a, b, c, d, e, f);
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

            self.backend.push_clip(&path, fepdf_model::graphics::WindingRule::NonZero);
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
        sd: &fepdf_model::object::SublimatedData,
    ) -> PdfResult<()> {
        let width_key = self.doc.arena().intern_name(PdfName::new("Width"));
        let height_key = self.doc.arena().intern_name(PdfName::new("Height"));

        let (width, height, format, decoded) = if let fepdf_model::object::SublimatedData::Image {
            width,
            height,
            format,
            data,
        } = sd
        {
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
                if let Some(decode_obj) = dict.get(&decode_key)
                    && let Some(arr_h) = decode_obj.resolve(self.doc.arena()).as_array()
                    && let Some(arr) = self.doc.arena().get_array(arr_h)
                    && arr.len() >= 2
                {
                    let first = arr[0].resolve(self.doc.arena()).as_f64().unwrap_or(0.0);
                    if first > 0.5 {
                        invert_mask = true;
                    }
                }
                if invert_mask {
                    fepdf_model::graphics::PixelFormat::MonoMaskInverted
                } else {
                    fepdf_model::graphics::PixelFormat::MonoMask
                }
            } else {
                self.detect_pixel_format(dict)
            };

            let decoded = self.doc.arena().process_filters(&data, dict)?;
            let (format, decoded) =
                if let Some(expanded) = expand_indexed_image(self.doc.arena(), dict, &decoded) {
                    (fepdf_model::graphics::PixelFormat::Rgb8, bytes::Bytes::from(expanded))
                } else {
                    (format, decoded)
                };
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
                    if let fepdf_model::object::SublimatedData::Image {
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

                Some(crate::SMaskData {
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
    ) -> fepdf_model::graphics::PixelFormat {
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
            "DeviceGray" | "G" | "Gray" => fepdf_model::graphics::PixelFormat::Gray8,
            "DeviceCMYK" | "CMYK" => fepdf_model::graphics::PixelFormat::Cmyk8,
            "Indexed" | "I" => fepdf_model::graphics::PixelFormat::Rgb8,
            _ => fepdf_model::graphics::PixelFormat::Rgb8,
        }
    }
}

fn get_indexed_cs_info(
    arena: &fepdf_model::PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
) -> Option<(String, usize, Vec<u8>)> {
    let cs_key = arena.intern_name(PdfName::new("ColorSpace"));
    let cs_obj = dict.get(&cs_key)?.resolve(arena);
    let arr = arena.get_array(cs_obj.as_array()?)?;
    if arr.len() < 4 {
        return None;
    }
    let first = arr[0].resolve(arena).as_name()?;
    let name = arena.get_name(first)?;
    if name.as_str() != "Indexed" && name.as_str() != "I" {
        return None;
    }
    let base_name = arr[1]
        .resolve(arena)
        .as_name()
        .and_then(|nh| arena.get_name(nh))
        .map_or_else(|| "DeviceRGB".to_string(), |n| n.as_str().to_string());
    let hival = usize::try_from(arr[2].resolve(arena).as_integer()?).ok()?;
    let lookup_bytes = match arr[3].resolve(arena) {
        Object::String(ref b) | Object::Hex(ref b) => Some(b.to_vec()),
        Object::Stream(dh, ref sd) => {
            let s_dict = arena.get_dict(dh)?;
            let raw = arena.get_stream_bytes(sd).ok()?;
            arena.process_filters(&raw, &s_dict).ok().map(|b| b.to_vec())
        }
        _ => None,
    }?;
    Some((base_name, hival, lookup_bytes))
}

fn expand_indexed_image(
    arena: &fepdf_model::PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
    decoded: &[u8],
) -> Option<Vec<u8>> {
    let (base_name, hival, lookup) = get_indexed_cs_info(arena, dict)?;
    let components = match base_name.as_str() {
        "DeviceGray" | "G" | "Gray" => 1,
        "DeviceCMYK" | "CMYK" => 4,
        _ => 3,
    };
    let mut rgb = Vec::with_capacity(decoded.len() * 3);
    for &idx in decoded {
        let offset = if usize::from(idx) <= hival { usize::from(idx) * components } else { 0 };
        if offset + components <= lookup.len() {
            match components {
                1 => rgb.extend_from_slice(&[lookup[offset], lookup[offset], lookup[offset]]),
                4 => {
                    let cyan = f64::from(lookup[offset]) / 255.0;
                    let magenta = f64::from(lookup[offset + 1]) / 255.0;
                    let yellow = f64::from(lookup[offset + 2]) / 255.0;
                    let black = f64::from(lookup[offset + 3]) / 255.0;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let red_val = ((1.0 - cyan) * (1.0 - black) * 255.0).round() as u8;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let green_val = ((1.0 - magenta) * (1.0 - black) * 255.0).round() as u8;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let blue_val = ((1.0 - yellow) * (1.0 - black) * 255.0).round() as u8;
                    rgb.extend_from_slice(&[red_val, green_val, blue_val]);
                }
                _ => rgb.extend_from_slice(&lookup[offset..offset + 3]),
            }
        } else {
            rgb.extend_from_slice(&[0, 0, 0]);
        }
    }
    Some(rgb)
}
