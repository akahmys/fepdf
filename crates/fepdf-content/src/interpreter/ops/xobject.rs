use crate::RenderBackend;
use crate::interpreter::Interpreter;
use fepdf_model::interpretation::Decision;
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
            // 8.11.3.2: an `/OC` here turns the whole XObject off, image or form. Checked
            // before the subtype, because "is this drawn at all" is answered the same way
            // for both — and before the form's state is saved, so a hidden form costs
            // nothing but the lookup.
            let oc_key = self.doc.arena().intern_name(PdfName::new("OC"));
            if let Some(entry) = dict.get(&oc_key).cloned()
                && self.optional_content_hides(&entry, &format!("XObject /{}", name.as_str()))
            {
                return Ok(());
            }
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
                    // Recorded, not logged (`ARCHITECTURE.md` §5.3). This site read
                    // `log::debug!` and a comment saying why it could not do better:
                    // the interpreter holds `&Document` and `DecisionLog::push` needed
                    // `&mut`. The log is behind a lock now, so the skip reaches the same
                    // place the reader's decisions do (ADR-0018).
                    "Image" => {
                        if let Err(e) = self.render_image_xobject(&dict, sd) {
                            self.record_skipped_image(&dict, name.as_str(), &e);
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
        self.in_nested_content(|me| me.execute_commands(cmds))?;

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
        self.in_nested_content(|me| me.execute_raw(&decoded))?;

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

    /// Records an image the engine gave up on, with the filter that stopped it.
    ///
    /// The filter is read from the image's own dictionary rather than from the error,
    /// because the error says what failed and the caller needs to know what the file
    /// asked for: `/CCITTFaxDecode` and `/JPXDecode` are codecs this engine has decided
    /// not to build (ROADMAP Phase L), and `/XXXDecode` is a filter invented for a test
    /// suite that no engine will ever decode. Those are different facts about the
    /// document and the message says which one it is.
    fn record_skipped_image(
        &self,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        name: &str,
        error: &PdfError,
    ) {
        let filters = self.filters_named_by(dict);

        // How much of the page went with it. An image occupies the unit square
        // transformed by the CTM (8.9.5.2), so the determinant of that matrix *is* its
        // area in default user space — no rendering required, which is what makes this
        // measurable on every file rather than on the ones a GPU is available for.
        let covered = self.cost_of_losing_it();

        // Clause 7.4 when the file named a filter this engine does not decode, because
        // that is a statement about the filter table; 8.9.5 otherwise, because then the
        // image dictionary itself is what could not be honoured.
        let unsupported = matches!(error, PdfError::Filter { message, .. } if message.starts_with("Unsupported filter"));
        let decision = match (&filters, unsupported) {
            (Some(f), true) => Decision::violation(
                "7.4",
                format!(
                    "image XObject /{name} is encoded with {f}, which this engine does not \
                     decode; it covers {covered}"
                ),
                "skipped the image; the rest of the content stream, including its text, was interpreted",
            ),
            (Some(f), false) => Decision::violation(
                "8.9.5",
                format!(
                    "image XObject /{name} ({f}) covering {covered} could not be decoded: {error}"
                ),
                "skipped the image; the rest of the content stream, including its text, was interpreted",
            ),
            (None, _) => Decision::violation(
                "8.9.5",
                format!("image XObject /{name} covering {covered} could not be decoded: {error}"),
                "skipped the image; the rest of the content stream, including its text, was interpreted",
            ),
        };
        self.doc.record(decision);
    }

    /// The filters an image dictionary names, as they are written.
    ///
    /// One name or an array of them (7.4.1), and the array matters: a `/JPXDecode`
    /// wrapped in `/FlateDecode` is a different fact from either alone.
    fn filters_named_by(&self, dict: &BTreeMap<Handle<PdfName>, Object>) -> Option<String> {
        let arena = self.doc.arena();
        let named = |h| arena.get_name(h).map(|n| format!("/{}", n.as_str()));
        dict.get(&arena.intern_name(PdfName::new("Filter")))
            .map(|o| o.resolve(arena))
            .map(|o| match o {
                Object::Name(h) => named(h).unwrap_or_default(),
                Object::Array(ah) => arena
                    .get_array(ah)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|item| item.resolve(arena).as_name())
                    .filter_map(named)
                    .collect::<Vec<_>>()
                    .join(" "),
                _ => String::new(),
            })
            .filter(|f| !f.is_empty())
    }

    /// What losing the image at the current `CTM` costs, in words.
    ///
    /// As a share of the page where the page is known, and in square points otherwise —
    /// a form XObject and a Type 3 glyph stream have no page to be a fraction of. Zero
    /// area is worth saying out loud: an image drawn under a degenerate matrix paints
    /// nothing, so losing it costs nothing, and that is the answer for a file whose
    /// image no page draws at all.
    fn cost_of_losing_it(&self) -> String {
        let area = self.state.ctm.as_affine().determinant().abs();
        if area <= f64::EPSILON {
            return "no area of the page — the matrix in force paints nothing".to_string();
        }
        match self.page_area {
            Some(page) => format!("{:.1}% of the page", area / page * 100.0),
            None => format!("{area:.0} square points"),
        }
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
                self.image_layout(dict, &data)
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

        // Sub-byte samples become bytes before a backend sees them, the way an indexed
        // image already did. A scanned page is `/DeviceGray` at one bit per component —
        // the commonest image in a scanned document and, until Phase M's own fixture
        // crashed the renderer with it, one neither corpus contained.
        let bits = dict
            .get(&self.doc.arena().intern_name(PdfName::new("BitsPerComponent")))
            .and_then(|o| o.resolve(self.doc.arena()).as_integer())
            .unwrap_or(8);
        let decoded = match expand_sub_byte_gray(&decoded, width, height, bits, format) {
            Some(expanded) => bytes::Bytes::from(expanded),
            None => decoded,
        };

        // What the dictionary describes and what arrived must agree. They did not, and
        // the buffer went to the GPU anyway: `Queue::write_texture` refused it and the
        // process died — a malformed image is a document defect, not a crash.
        if let Some(needed) = bytes_needed(width, height, format)
            && decoded.len() < needed
        {
            self.doc.record(Decision::violation(
                "8.9.5.1",
                format!(
                    "an image XObject describes {needed} bytes of samples and carries {}",
                    decoded.len()
                ),
                "skipped the image; drawing it would have read past the data",
            ));
            return Ok(());
        }

        self.backend.draw_image(&decoded, width, height, format, smask_data);
        Ok(())
    }

    /// How the decoded samples are laid out, from the image's `/ColorSpace` (8.6).
    ///
    /// **What this decides is the number of components per sample**, and getting it
    /// wrong does not shift a colour — it walks the buffer at the wrong stride and
    /// renders noise. So the question asked here is "how many components", and the
    /// colour space's *identity* is a separate matter that colour management would own.
    ///
    /// The family name alone does not answer it. `[/ICCBased stream]` carries `/N`, and
    /// the corpus's largest group of images by far is exactly that — 438 of 1,053 — so
    /// assuming three there is assuming for most of the pictures in the corpus.
    /// `[/Separation …]` is one component and `[/DeviceN [names] …]` is as many as it
    /// names. Both were read as three.
    ///
    /// A one-component space that is not grey — `/Separation`, `/DeviceN` with one
    /// colorant — is reported as `Gray8` because that is its *shape*. Painting it
    /// correctly means running the tint transform, which nothing here does yet; reading
    /// it at the right stride is the difference between a wrong colour and a wrecked
    /// image.
    fn detect_pixel_format(
        &self,
        dict: &BTreeMap<Handle<PdfName>, Object>,
    ) -> fepdf_model::graphics::PixelFormat {
        pixel_format_of(self.doc.arena(), dict)
    }

    /// The layout of an image, asking the codestream when the dictionary does not say.
    ///
    /// 7.4.9 makes `/ColorSpace` **optional for a `/JPXDecode` image and no other**: the
    /// codestream carries its own, and where the dictionary does state one it overrides.
    /// So this asks the dictionary first and the data only when the dictionary is silent
    /// — which is the order the clause gives, not a preference.
    fn image_layout(
        &self,
        dict: &BTreeMap<Handle<PdfName>, Object>,
        encoded: &[u8],
    ) -> fepdf_model::graphics::PixelFormat {
        let arena = self.doc.arena();
        if dict.contains_key(&arena.intern_name(PdfName::new("ColorSpace"))) {
            return pixel_format_of(arena, dict);
        }
        if names_filter(arena, dict, "JPXDecode")
            && let Some(from_codestream) = fepdf_model::filters::jpx::layout(encoded)
        {
            return from_codestream;
        }
        pixel_format_of(arena, dict)
    }
}

/// Whether the stream's `/Filter` names `wanted`, in either of its two forms.
fn names_filter(
    arena: &fepdf_model::arena::PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
    wanted: &str,
) -> bool {
    let Some(filter) = dict.get(&arena.intern_name(PdfName::new("Filter"))) else {
        return false;
    };
    let named = |h| arena.get_name(h).is_some_and(|n| n.as_str() == wanted);
    match filter.resolve(arena) {
        Object::Name(h) => named(h),
        Object::Array(ah) => arena
            .get_array(ah)
            .unwrap_or_default()
            .iter()
            .filter_map(|o| o.resolve(arena).as_name())
            .any(named),
        _ => false,
    }
}

/// See [`Interpreter::detect_pixel_format`]. A free function because it is a pure
/// question about a dictionary, and one that needs testing without a whole backend.
pub(crate) fn pixel_format_of(
    arena: &fepdf_model::arena::PdfArena,
    dict: &BTreeMap<Handle<PdfName>, Object>,
) -> fepdf_model::graphics::PixelFormat {
    {
        use fepdf_model::graphics::PixelFormat;
        let Some(cs) = dict.get(&arena.intern_name(PdfName::new("ColorSpace"))) else {
            return PixelFormat::Rgb8;
        };

        match cs.resolve(arena) {
            Object::Name(h) => match arena.get_name(h).map(|n| n.as_str().to_string()).as_deref() {
                Some("DeviceGray" | "CalGray" | "G" | "Gray") => PixelFormat::Gray8,
                Some("DeviceCMYK" | "CMYK") => PixelFormat::Cmyk8,
                _ => PixelFormat::Rgb8,
            },
            Object::Array(ah) => components_of_array(arena, ah),
            _ => PixelFormat::Rgb8,
        }
    }
}

/// The layout of an array colour space — the forms 8.6.5 and 8.6.6 define.
fn components_of_array(
    arena: &fepdf_model::arena::PdfArena,
    array: Handle<Vec<Object>>,
) -> fepdf_model::graphics::PixelFormat {
    {
        use fepdf_model::graphics::PixelFormat;
        let items = arena.get_array(array).unwrap_or_default();
        let family = items
            .first()
            .and_then(|o| o.resolve(arena).as_name())
            .and_then(|h| arena.get_name(h))
            .map(|n| n.as_str().to_string())
            .unwrap_or_default();

        match family.as_str() {
            // Expanded to RGB before it reaches a backend (`expand_indexed_image`).
            "Indexed" | "I" => PixelFormat::Rgb8,
            "CalGray" => PixelFormat::Gray8,
            "CalRGB" | "Lab" => PixelFormat::Rgb8,
            // One colorant, whatever it is named.
            "Separation" => PixelFormat::Gray8,
            "DeviceN" => match items.get(1).and_then(|o| o.resolve(arena).as_array()) {
                Some(names) => match arena.get_array(names).map_or(0, |n| n.len()) {
                    1 => PixelFormat::Gray8,
                    4 => PixelFormat::Cmyk8,
                    _ => PixelFormat::Rgb8,
                },
                None => PixelFormat::Rgb8,
            },
            // `/N` is required, and is the whole answer (8.6.5.5).
            "ICCBased" => {
                let n = items
                    .get(1)
                    .map(|o| o.resolve(arena))
                    .and_then(|stream| match stream {
                        Object::Stream(dh, _) => arena.get_dict(dh),
                        _ => None,
                    })
                    .and_then(|d| d.get(&arena.intern_name(PdfName::new("N"))).cloned())
                    .and_then(|o| o.resolve(arena).as_integer());
                match n {
                    Some(1) => PixelFormat::Gray8,
                    Some(4) => PixelFormat::Cmyk8,
                    _ => PixelFormat::Rgb8,
                }
            }
            _ => PixelFormat::Rgb8,
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

/// How many bytes `width × height` samples occupy in `format`.
///
/// `None` for the one-bit stencils, whose layout is bits with each row padded to a byte
/// — `PixelFormat` carries no bit depth, so those are the formats where the length is
/// not this arithmetic.
fn bytes_needed(
    width: u32,
    height: u32,
    format: fepdf_model::graphics::PixelFormat,
) -> Option<usize> {
    use fepdf_model::graphics::PixelFormat;
    let per_pixel = match format {
        PixelFormat::Gray8 => 1,
        PixelFormat::Rgb8 => 3,
        PixelFormat::Cmyk8 | PixelFormat::Rgba8 => 4,
        PixelFormat::MonoMask | PixelFormat::MonoMaskInverted => return None,
    };
    Some(width as usize * height as usize * per_pixel)
}

/// Expands samples of fewer than eight bits to one byte each (8.9.5.1).
///
/// A scanned page is `/DeviceGray` with `/BitsPerComponent 1`, and `PixelFormat` has no
/// depth to carry that — `Gray8` means a byte a pixel. So the expansion happens here,
/// exactly as `expand_indexed_image` already expands a palette, and the bit depth stops
/// existing above this line.
///
/// Rows are padded to a byte boundary in the source and not in the result, which is the
/// whole reason this cannot be a simple bit-by-bit walk of the buffer.
///
/// `None` when there is nothing to expand: eight bits already, or a stencil mask, whose
/// bits the backend reads itself because what it paints them is the fill colour.
fn expand_sub_byte_gray(
    data: &[u8],
    width: u32,
    height: u32,
    bits: i64,
    format: fepdf_model::graphics::PixelFormat,
) -> Option<Vec<u8>> {
    use fepdf_model::graphics::PixelFormat;
    if format != PixelFormat::Gray8 || !(1..8).contains(&bits) {
        return None;
    }
    let bits = u32::try_from(bits).ok()?;
    let max = (1_u32 << bits) - 1;
    let stride = (width as usize * bits as usize).div_ceil(8);

    let mut out = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height as usize {
        let row = data.get(y * stride..(y + 1) * stride)?;
        for x in 0..width {
            let at = x * bits;
            let (byte, shift) = ((at / 8) as usize, at % 8);
            // A sample may straddle two bytes when the depth is not a power of two that
            // divides eight — four bits never do, two and one never do, so this reads at
            // most two.
            let window = u32::from(*row.get(byte)?) << 8
                | u32::from(row.get(byte + 1).copied().unwrap_or(0));
            let sample = (window >> (16 - shift - bits)) & max;
            out.push(u8::try_from(sample * 255 / max).unwrap_or(255));
        }
    }
    Some(out)
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

#[cfg(test)]
mod tests {
    use super::*;
    use fepdf_model::arena::PdfArena;
    use fepdf_model::graphics::PixelFormat;

    /// An image dictionary carrying just a `/ColorSpace`, parsed the way a file writes it.
    fn image_with(colour_space: &str) -> (PdfArena, BTreeMap<Handle<PdfName>, Object>) {
        let arena = PdfArena::new();
        let source = format!("<< /Width 2 /Height 2 /ColorSpace {colour_space} >>\n");
        let mut parser = fepdf_model::parser::Parser::new(bytes::Bytes::from(source), &arena);
        let Object::Dictionary(dh) = parser.parse_object().expect("parses") else {
            panic!("a dictionary");
        };
        let dict = arena.get_dict(dh).expect("in the arena");
        (arena, dict)
    }

    /// The device spaces, which are the easy half.
    #[test]
    fn a_device_space_gives_its_own_component_count() {
        for (space, expected) in [
            ("/DeviceGray", PixelFormat::Gray8),
            ("/CalGray", PixelFormat::Gray8),
            ("/DeviceRGB", PixelFormat::Rgb8),
            ("/DeviceCMYK", PixelFormat::Cmyk8),
        ] {
            let (arena, dict) = image_with(space);
            assert_eq!(pixel_format_of(&arena, &dict), expected, "{space}");
        }
    }

    /// `[/ICCBased stream]` carries `/N`, and **the family name does not answer the
    /// question** — 438 of the 1,053 images in the two corpora are this shape, so
    /// assuming three components here is assuming for most of the pictures there are.
    #[test]
    fn an_icc_based_space_takes_its_count_from_n() {
        for (n, expected) in
            [(1, PixelFormat::Gray8), (3, PixelFormat::Rgb8), (4, PixelFormat::Cmyk8)]
        {
            let arena = PdfArena::new();
            let profile = arena.alloc_dict(BTreeMap::from([(arena.name("N"), Object::Integer(n))]));
            let space = arena.alloc_array(vec![
                Object::Name(arena.name("ICCBased")),
                Object::Stream(
                    profile,
                    std::sync::Arc::new(fepdf_model::object::SublimatedData::Raw(
                        bytes::Bytes::new(),
                    )),
                ),
            ]);
            let dict = BTreeMap::from([(arena.name("ColorSpace"), Object::Array(space))]);
            assert_eq!(pixel_format_of(&arena, &dict), expected, "/N {n}");
        }
    }

    /// A `/Separation` is one colorant, so one component — it was read as three.
    ///
    /// Reported as `Gray8` for its *shape*, not its colour: painting it properly means
    /// running the tint transform, and nothing here does yet. Reading it at the right
    /// stride is the difference between a wrong colour and a wrecked image.
    #[test]
    fn a_separation_is_one_component_and_a_device_n_is_as_many_as_it_names() {
        let (arena, dict) = image_with("[/Separation /Spot /DeviceCMYK null]");
        assert_eq!(pixel_format_of(&arena, &dict), PixelFormat::Gray8);

        let (arena, dict) = image_with("[/DeviceN [/C /M /Y /K] /DeviceCMYK null]");
        assert_eq!(pixel_format_of(&arena, &dict), PixelFormat::Cmyk8);

        let (arena, dict) = image_with("[/DeviceN [/Spot] /DeviceCMYK null]");
        assert_eq!(pixel_format_of(&arena, &dict), PixelFormat::Gray8);
    }

    /// `/Indexed` is expanded to RGB before a backend sees it.
    #[test]
    fn an_indexed_space_is_reported_as_what_it_expands_to() {
        let (arena, dict) = image_with("[/Indexed /DeviceRGB 255 <00>]");
        assert_eq!(pixel_format_of(&arena, &dict), PixelFormat::Rgb8);
    }

    /// No `/ColorSpace` at all — an `/ImageMask` has none, and the caller handles that
    /// before asking. Anything else falls back rather than refusing.
    #[test]
    fn an_image_with_no_colour_space_falls_back_to_rgb() {
        let arena = PdfArena::new();
        assert_eq!(pixel_format_of(&arena, &BTreeMap::new()), PixelFormat::Rgb8);
    }
}
