//! Drawing a page: its content interpreted, and the annotations a reader would show over
//! it.

use super::{PdfDocument, read_numbers};
use fepdf_content::Interpreter;
use fepdf_model::{Object, Page, PdfName, PdfResult, SublimatedData};

impl PdfDocument {
    /// The page's content stream, or `None` where 7.7.3.3 allows it to have none.
    ///
    /// `/Contents` is **optional**: a page without one is empty, and it is a shape that
    /// occurs — a page whose only mark is an annotation's appearance stream. This
    /// returned `Err("Page has no contents")`, so rendering such a page failed and
    /// `extract_text` failed with it. Seven files of the corpus fetched for Phase O-1 are
    /// exactly that, and they were counted as text-extraction failures until the corpus
    /// arrived to disagree — a check firing on conforming input, which ADR-0008 is about.
    pub(super) fn resolve_page_contents(&self, page: &Page) -> PdfResult<Option<Object>> {
        let Some(contents_obj) = page.resolve_attribute("Contents") else {
            return Ok(None);
        };
        let resolved_contents = match contents_obj {
            Object::Reference(h) => {
                log::debug!("[SDK] Resolving Contents reference {h:?}");
                self.inner.resolve(&h)?
            }
            _ => contents_obj,
        };
        Ok(Some(resolved_contents))
    }

    pub(super) fn execute_interpreter(
        &self,
        interpreter: &mut Interpreter<'_>,
        resolved_contents: Object,
    ) -> PdfResult<()> {
        let arena = self.inner.arena();
        match resolved_contents {
            Object::Stream(dh, ref data) => {
                if let SublimatedData::Commands { items: cmds, .. } = &**data {
                    interpreter.execute_commands(cmds)?;
                } else {
                    let data = self.inner.decode_stream(&Object::Stream(dh, (*data).clone()))?;
                    interpreter.execute_raw(&data)?;
                }
            }
            Object::Array(ah) => {
                if let Some(arr) = arena.get_array(ah) {
                    for item in arr {
                        let resolved_item = match item {
                            Object::Reference(h) => self.inner.resolve(&h)?,
                            _ => item,
                        };
                        if let Object::Stream(_, ref data) = resolved_item {
                            if let SublimatedData::Commands { items: cmds, .. } = &**data {
                                interpreter.execute_commands(cmds)?;
                            } else {
                                let data = self.inner.decode_stream(&resolved_item)?;
                                interpreter.execute_raw(&data)?;
                            }
                        }
                    }
                }
            }
            _ => {
                let data = self.inner.decode_stream(&resolved_contents)?;
                interpreter.execute_raw(&data)?;
            }
        }
        Ok(())
    }

    /// Renders a page to a provide backend.
    pub fn render_page(
        &self,
        index: usize,
        backend: &mut dyn fepdf_content::RenderBackend,
        initial_transform: kurbo::Affine,
    ) -> PdfResult<()> {
        let page = self.inner.get_page(index)?;
        let res_dh = page.resources_handle();
        // Anything the backend concluded on an earlier page belongs to that page, not
        // this one. Draining first means what is recorded below is what *this* call drew.
        let _ = backend.take_decisions();
        let mut interpreter = Interpreter::new(backend, &self.inner, res_dh, initial_transform);
        // So that an image the engine cannot decode can say how much of *this page* it
        // would have covered, rather than only that one was dropped (ROADMAP Phase L).
        let box_ = page.media_box();
        interpreter.set_page_area((box_.x2 - box_.x1).abs() * (box_.y2 - box_.y1).abs());

        // The contents and the annotations are two halves of what 6.3.2.2 asks a renderer
        // for, and one failing does not cancel the other. `UnknownFilter-PageContentStream.pdf`
        // is the case: its content stream dictionary is malformed, and drawing nothing at
        // all because of that would lose every annotation the page carries. The error is
        // still returned — `extract_text` reports it, and the corpus measurement counts
        // it — but it is returned *after* the page has been drawn as far as it can be.
        let drawn = match self.resolve_page_contents(&page)? {
            Some(resolved_contents) => {
                self.execute_interpreter(&mut interpreter, resolved_contents)
            }
            None => Ok(()),
        };
        // Taken before the drop and handed over after it, because the interpreter holds
        // the backend borrowed until then. Only the page's own content stream is
        // measured: an annotation's appearance is a separate stream with its own
        // interpreter, and 14.7.4.2 numbers marked content per stream — an `/MCID 0` in a
        // widget is not the `/MCID 0` in the page.
        let marks = interpreter.take_mark_bounds();
        drop(interpreter);
        backend.receive_mark_bounds(marks);
        self.render_annotations(index, backend, initial_transform)?;
        // A backend sits below any `Document` and cannot record for itself, so what it
        // concluded about the font programs it was handed is folded in here — after the
        // annotations, because their appearance streams draw glyphs too (ARCHITECTURE
        // §4.3).
        for decision in backend.take_decisions() {
            self.inner.record(decision);
        }
        drawn
    }

    /// Draws every annotation on the page that has an appearance and is meant to be seen
    /// (12.5.5).
    ///
    /// **Not optional for anything that renders.** 6.3.2.2 says a PDF processor that
    /// renders a page shall render the appropriate appearance stream for all annotations
    /// that have one, unless the annotation flags say otherwise — and this engine drew
    /// none at all, so a page whose only mark is an annotation came out blank while every
    /// other reader painted it.
    ///
    /// Each appearance is a form XObject with its own coordinate system, so it gets its
    /// own interpreter: the transform is the page's, with the placement 12.5.5's
    /// algorithm computes applied inside it, and the resources are the appearance's own.
    pub(super) fn render_annotations(
        &self,
        index: usize,
        backend: &mut dyn fepdf_content::RenderBackend,
        page_transform: kurbo::Affine,
    ) -> PdfResult<()> {
        let arena = self.inner.arena();
        let page = self.inner.get_page(index)?;
        let Some(annots) = page.get_attribute("Annots") else { return Ok(()) };
        let Object::Array(handle) = annots.resolve(arena) else { return Ok(()) };
        let optional_content =
            fepdf_model::optional_content::OptionalContentState::read(&self.inner);
        for entry in arena.get_array(handle).unwrap_or_default() {
            let Some(dict) = entry.resolve(arena).as_dict_handle().and_then(|h| arena.get_dict(h))
            else {
                continue;
            };
            if !self.annotation_is_shown(&dict, &optional_content) {
                continue;
            }
            if let Some((stream, placement, resources)) = self.appearance_of(&dict) {
                let mut inner = Interpreter::new(
                    backend,
                    &self.inner,
                    resources,
                    page_transform * placement.as_affine(),
                );
                // An appearance that will not execute is one annotation, not the page.
                let _ = inner.execute(stream);
            }
        }
        Ok(())
    }

    /// Whether an annotation is meant to be seen on screen (12.5.3, Table 167).
    ///
    /// `Hidden` is bit 2 and means never; `NoView` is bit 6 and means not on a screen,
    /// which is what this renders to. `Invisible` is bit 1 and is **not** applied: it
    /// governs only annotations of no standard type *for which no handler exists*, and
    /// this engine has no handlers at all — it draws appearance streams, which is what
    /// the flag's own "if clear" branch describes.
    pub(super) fn annotation_is_shown(
        &self,
        dict: &std::collections::BTreeMap<fepdf_model::Handle<PdfName>, Object>,
        optional_content: &fepdf_model::optional_content::OptionalContentState,
    ) -> bool {
        let arena = self.inner.arena();
        let flags =
            dict.get(&arena.name("F")).and_then(|f| f.resolve(arena).as_integer()).unwrap_or(0);
        if flags & 0b10 != 0 || flags & 0b10_0000 != 0 {
            return false;
        }
        match dict.get(&arena.name("OC")) {
            Some(oc) => !matches!(
                optional_content.membership(arena, oc),
                fepdf_model::optional_content::Membership::Hidden
            ),
            None => true,
        }
    }

    /// The annotation's normal appearance, where it goes, and what it draws with.
    pub(super) fn appearance_of(
        &self,
        dict: &std::collections::BTreeMap<fepdf_model::Handle<PdfName>, Object>,
    ) -> Option<(
        fepdf_model::Handle<Object>,
        fepdf_model::graphics::Matrix,
        fepdf_model::Handle<std::collections::BTreeMap<fepdf_model::Handle<PdfName>, Object>>,
    )> {
        let arena = self.inner.arena();
        let appearances =
            arena.get_dict(dict.get(&arena.name("AP"))?.resolve(arena).as_dict_handle()?)?;
        let normal = appearances.get(&arena.name("N"))?;
        let stream = self.appearance_state(normal, dict)?;
        let stream_dict =
            arena.get_object(stream)?.as_dict_handle().and_then(|h| arena.get_dict(h))?;

        let bbox = read_numbers(arena, stream_dict.get(&arena.name("BBox")), 4)?;
        let matrix = read_numbers(arena, stream_dict.get(&arena.name("Matrix")), 6)
            .map_or_else(fepdf_model::graphics::Matrix::default, |m| {
                fepdf_model::graphics::Matrix::new(m[0], m[1], m[2], m[3], m[4], m[5])
            });
        let rect = read_numbers(arena, dict.get(&arena.name("Rect")), 4)?;
        let placement = fepdf_model::annotation::appearance_placement(
            [bbox[0], bbox[1], bbox[2], bbox[3]],
            matrix,
            &fepdf_model::graphics::Rect::new(rect[0], rect[1], rect[2], rect[3]),
        );
        let resources = stream_dict
            .get(&arena.name("Resources"))
            .and_then(|r| r.resolve(arena).as_dict_handle())
            .unwrap_or_else(|| arena.alloc_dict(std::collections::BTreeMap::new()));
        Some((stream, placement, resources))
    }

    /// `/N` is a stream, or a dictionary of them keyed by the state `/AS` names (12.5.5).
    ///
    /// A checkbox keeps `/Off` and `/Yes` under `/N` and says which is current in `/AS`.
    /// Where `/AS` is missing and the dictionary holds exactly one appearance there is
    /// nothing to choose between, so that one is drawn and the omission recorded.
    pub(super) fn appearance_state(
        &self,
        normal: &Object,
        annotation: &std::collections::BTreeMap<fepdf_model::Handle<PdfName>, Object>,
    ) -> Option<fepdf_model::Handle<Object>> {
        let arena = self.inner.arena();
        if matches!(normal.resolve(arena), Object::Stream(..)) {
            return normal.as_reference();
        }
        let states = arena.get_dict(normal.resolve(arena).as_dict_handle()?)?;
        if let Some(state) =
            annotation.get(&arena.name("AS")).and_then(|s| s.resolve(arena).as_name())
        {
            return states.get(&state).and_then(Object::as_reference);
        }
        if states.len() == 1 {
            self.inner.record(fepdf_model::interpretation::Decision::repaired(
                "12.5.5",
                "an annotation's /AP /N is a dictionary of states and it names none in /AS",
                "drew the only appearance it holds, since there was nothing to choose between",
            ));
            return states.values().next().and_then(Object::as_reference);
        }
        self.inner.record(fepdf_model::interpretation::Decision::violation(
            "12.5.5",
            format!("an annotation's /AP /N holds {} states and /AS names none", states.len()),
            "drew no appearance, because choosing one would be this engine's choice",
        ));
        None
    }
}
