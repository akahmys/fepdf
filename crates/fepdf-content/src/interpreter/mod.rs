use crate::RenderBackend;
use crate::canvas::Canvas;
use crate::interpreter::ops::marked::MarkedSection;
use crate::path::PathBuilder;
use fepdf_model::graphics::{GraphicsState, TextMatrices, WindingRule};
use fepdf_model::interpretation::Decision;
use fepdf_model::object::sublimation::Command;
use fepdf_model::optional_content::OptionalContentState;
use fepdf_model::{Document, Handle, Object, PdfError, PdfName, PdfResult};
use std::collections::{BTreeMap, BTreeSet};

/// Captured advance and BBox from d0/d1 operator.
#[derive(Debug, Clone, Copy)]
pub struct Type3Advance {
    /// Horizontal advance.
    pub wx: f64,
    /// Vertical advance.
    pub wy: f64,
    /// Lower-left X coordinate of the bounding box.
    pub llx: f64,
    /// Lower-left Y coordinate of the bounding box.
    pub lly: f64,
    /// Upper-right X coordinate of the bounding box.
    pub urx: f64,
    /// Upper-right Y coordinate of the bounding box.
    pub ury: f64,
}

/// Font resolution and rescue logic.
pub mod font;
/// Operators handling submodules.
pub mod ops;

/// A content stream interpreter that translates PDF operators into [RenderBackend] calls.
pub struct Interpreter<'a> {
    /// The rendering backend used to draw items, behind the gate that withholds marks
    /// while an optional-content section is off (`crate::canvas`).
    pub(crate) backend: Canvas<'a>,
    /// The document being interpreted.
    pub(crate) doc: &'a Document,
    /// Stack of resource dictionaries for hierarchical lookup (Form XObjects).
    pub(crate) resource_stack: Vec<Handle<BTreeMap<Handle<PdfName>, Object>>>,
    /// Operand stack for operators.
    pub(crate) stack: Vec<Object>,
    /// Current path being constructed.
    pub(crate) path: PathBuilder,
    /// Pending clipping rule from W or W* operator.
    pub(crate) pending_clip: Option<WindingRule>,
    /// Graphics state stack (managed by q/Q).
    pub(crate) state_stack: Vec<GraphicsState>,
    /// Current active graphics state.
    pub(crate) state: GraphicsState,
    /// Current text object state (managed by BT/ET).
    pub(crate) text_matrices: Option<TextMatrices>,
    /// Cache of fonts already defined in the backend.
    pub(crate) defined_fonts: BTreeSet<String>,
    pub(crate) font_name_map: BTreeMap<Handle<Object>, String>,
    /// Index of the current operator in the content stream.
    pub op_index: usize,
    /// Captured advance and BBox from d0/d1 operator during Type 3 glyph execution.
    pub(crate) type3_advance: Option<Type3Advance>,
    /// Whether we are currently executing a Type 3 glyph stream.
    pub(crate) in_type3_glyph: bool,
    /// The initial transformation matrix (device transform).
    pub(crate) initial_transform: kurbo::Affine,
    /// Which optional content groups the document turns off (8.11), read on the first
    /// `/OC` this interpreter meets.
    ///
    /// Lazy: `None` means "not read yet", and a page carrying no `/OC` at all never
    /// touches the catalogue — which is every page of both corpora, since the one file of
    /// the 251 that declares `/OCProperties` declares an empty `/OCGs`.
    pub(crate) optional_content: Option<OptionalContentState>,
    /// The optional-content sections open at this point in the stream, and whether each
    /// one hid what followed. `EMC` needs to know which, and the depth cannot be taken
    /// from the canvas alone: a section that was *visible* also has to be closed.
    pub(crate) marked_sections: Vec<MarkedSection>,
    /// The page's area in square points, when the caller knows which page this is.
    ///
    /// Only one thing reads it: an image the engine cannot decode reports **how much of
    /// the page it would have covered**, which is the difference between "a picture was
    /// dropped" and "62% of this page is missing". Optional because the interpreter also
    /// runs over form XObjects and Type 3 glyph streams, where there is no page to be a
    /// fraction of.
    pub(crate) page_area: Option<f64>,
    /// The soft-mask brackets open at this point in the stream (11.6.5.2).
    ///
    /// A soft mask is set by `gs` and lasts until the graphics state that set it is
    /// restored, so it is a *scope* and not an operator pair the way `BDC`/`EMC` is.
    /// Each entry remembers the `q` depth it was opened at, which is what `Q` compares
    /// against to know that the scope has ended.
    pub(crate) mask_scopes: Vec<MaskScope>,
}

/// One open soft-mask bracket.
///
/// The group is kept as the object rather than as bytes: closing the scope replays it
/// through this same interpreter, which is the only thing that knows how to run a content
/// stream, and 11.6.5.2 puts the mask in the coordinate system that was current when
/// `gs` set it — so the matrix is saved here rather than taken from wherever `Q` leaves
/// it.
pub(crate) struct MaskScope {
    /// The `q` depth this was opened at. `Q` closes the scope when the depth drops below.
    pub(crate) depth: usize,
    /// How the group's drawing becomes an alpha.
    pub(crate) spec: fepdf_model::graphics::SoftMaskSpec,
    /// The `/G` form XObject that defines the mask.
    pub(crate) group: Object,
    /// The CTM at the moment `gs` set the mask.
    pub(crate) ctm: fepdf_model::graphics::Matrix,
}

impl<'a> Interpreter<'a> {
    /// Creates a new interpreter tied to a specific rendering backend.
    pub fn new(
        backend: &'a mut dyn RenderBackend,
        doc: &'a Document,
        initial_resources: Handle<BTreeMap<Handle<PdfName>, Object>>,
        initial_transform: kurbo::Affine,
    ) -> Self {
        let state = GraphicsState {
            ctm: fepdf_model::graphics::Matrix::default(),
            ..GraphicsState::default()
        };

        backend.set_transform(initial_transform);

        Self {
            backend: Canvas::new(backend, initial_transform),
            doc,
            resource_stack: vec![initial_resources],
            stack: Vec::new(),
            path: PathBuilder::new(),
            pending_clip: None,
            state_stack: Vec::new(),
            state,
            text_matrices: None,
            defined_fonts: BTreeSet::new(),
            font_name_map: BTreeMap::new(),
            op_index: 0,
            type3_advance: None,
            in_type3_glyph: false,
            optional_content: None,
            marked_sections: Vec::new(),
            mask_scopes: Vec::new(),
            initial_transform,
            page_area: None,
        }
    }

    /// Tells the interpreter how large the page is, in square points.
    ///
    /// A setter rather than a constructor argument: `Interpreter::new` has six callers
    /// and five of them have no page — the remediation walks, the visual audit and the
    /// sample renderers all drive it over content they have already chosen.
    pub fn set_page_area(&mut self, area: f64) {
        self.page_area = (area > 0.0).then_some(area);
    }

    /// Where each `/MCID` on this stream drew, in default user space (14.7.4.2).
    ///
    /// Taken rather than borrowed, and taken once: the boxes are the interpreter's only
    /// output that outlives it, and a caller that reads them twice would be asking a
    /// question the second answer to is empty.
    pub fn take_mark_bounds(&mut self) -> BTreeMap<u32, kurbo::Rect> {
        self.backend.take_mark_bounds()
    }

    pub(crate) fn update_backend_transform(&mut self) {
        let total = self.initial_transform * self.state.ctm.as_affine();
        self.backend.set_transform(total);
    }

    /// Executes a content stream by parsing and processing its operators.
    pub fn execute(&mut self, stream_h: Handle<Object>) -> PdfResult<()> {
        let sublimated = self
            .doc
            .arena()
            .get_sublimated_data(stream_h)
            .ok_or_else(|| PdfError::Other("Not a stream object".into()))?;

        // Type 3 fonts and complex Japanese CID fonts often contain operators
        // that are sensitive to raw stream ordering. We prefer raw execution
        // for these contexts to ensure rendering fidelity.
        if self.in_type3_glyph {
            let data = self.decoded_stream(stream_h, &sublimated)?;
            return self.execute_raw(&data);
        }

        match *sublimated {
            fepdf_model::object::SublimatedData::Commands { items: ref cmds, .. } => {
                self.execute_commands(cmds)
            }
            // Anything not already parsed into commands is replayed from raw bytes.
            fepdf_model::object::SublimatedData::Image { .. }
            | fepdf_model::object::SublimatedData::Compressed { .. }
            | fepdf_model::object::SublimatedData::Raw(_) => {
                let data = self.decoded_stream(stream_h, &sublimated)?;
                self.execute_raw(&data)
            }
        }
    }

    /// A stream's bytes with its `/Filter` applied, whether or not ingestion applied it.
    ///
    /// **`get_stream_bytes` undoes the arena's own compression and nothing else.** With
    /// `IngestionOptions::active_refinement` on, refinement has already decoded every
    /// stream and removed `/Filter` from its dictionary, so this is a second no-op. With
    /// it off — `fepdf --no-refinement` — the dictionary still names the filter and the
    /// bytes are still encoded, and this used to hand them to the lexer: page 1 of
    /// `samples/fugaku.pdf` recorded **3,203 unknown operators** with names like
    /// `x\u{9c}UWK`, which is a zlib header being read as a content stream, and lost 288
    /// Type 3 glyphs to it. Found by `crates/fepdf/tests/parser_twin_test.rs`.
    fn decoded_stream(
        &self,
        stream_h: Handle<Object>,
        sublimated: &fepdf_model::object::SublimatedData,
    ) -> PdfResult<bytes::Bytes> {
        let raw = self.doc.arena().get_stream_bytes(sublimated)?;
        let Some(Object::Stream(dh, _)) = self.doc.arena().get_object(stream_h) else {
            return Ok(raw);
        };
        let Some(dict) = self.doc.arena().get_dict(dh) else { return Ok(raw) };
        self.doc.arena().process_filters(&raw, &dict)
    }

    /// Interprets a content stream from its bytes, by sublimating them first.
    ///
    /// **There was a second reader here until 2026-09-09.** This lexed the bytes itself
    /// and dispatched each operator, which made it a peer of
    /// `fepdf_model::object::sublimation::Sublimator` — except that it was not one. The
    /// two handled 66 of the same operators and the sublimator five more, and
    /// `ops/marked.rs` said so in its own words: *`BMC`, `BDC` and `EMC` become
    /// `Command::BeginMarkedContent` and `Command::EndMarkedContent` in the parser and
    /// arrive through those arms.* A stream that reached this function instead — which is
    /// what `fepdf --no-refinement` produces — had its marked content dropped, so an
    /// optional-content section that should have been hidden was drawn and `/ActualText`
    /// was not read. Measured on `samples/fugaku.pdf`: 196 backend calls that the refined
    /// path made and this one did not.
    ///
    /// One reader now. `crates/fepdf/tests/parser_twin_test.rs` is what holds it to the
    /// other path's conclusions.
    pub fn execute_raw(&mut self, data: &[u8]) -> PdfResult<()> {
        if data.is_empty() {
            return Ok(());
        }
        let fonts = self.resource_fonts();
        let mut sublimator = fepdf_model::object::sublimation::parser::Sublimator::new(&fonts);
        let commands = sublimator.sublimate(data);
        let indices = sublimator.operator_indices().to_vec();
        for decision in sublimator.take_decisions() {
            self.doc.record(decision);
        }
        // `op_index` names an operator of *these bytes*, because the only thing that reads
        // it re-lexes them: `fepdf-doc`'s redaction collects text runs here and then
        // scrubs the strings the same operators carry. Counting commands instead would
        // drift the moment one operator emitted two, which `Tf` does.
        for (position, command) in commands.iter().enumerate() {
            self.op_index = indices.get(position).copied().unwrap_or(position + 1);
            self.execute_single_command(command)?;
        }
        Ok(())
    }

    /// Executes a sequence of pre-sublimated commands.
    pub fn execute_commands(&mut self, cmds: &[Command]) -> PdfResult<()> {
        log::debug!("[SDK] Executing {} sublimated commands", cmds.len());
        for cmd in cmds {
            self.execute_single_command(cmd)?;
        }
        Ok(())
    }

    pub(crate) fn push_real(&mut self, val: f64) {
        self.stack.push(Object::Real(val));
    }

    pub(crate) fn push_integer(&mut self, val: i64) {
        self.stack.push(Object::Integer(val));
    }

    pub(crate) fn push_name(&mut self, name: &str) {
        let handle = self.doc.arena().intern_name(PdfName::new(name));
        self.stack.push(Object::Name(handle));
    }

    pub(crate) fn push_affine(&mut self, m: &kurbo::Affine) {
        for &coeff in &m.as_coeffs() {
            self.push_real(coeff);
        }
    }

    pub(crate) fn push_point(&mut self, p: kurbo::Point) {
        self.push_real(p.x);
        self.push_real(p.y);
    }

    fn execute_single_command(&mut self, cmd: &Command) -> PdfResult<()> {
        match cmd {
            // --- Graphics State ---
            Command::PushState => self.handle_state_operator("q"),
            Command::PopState => self.handle_state_operator("Q"),
            Command::Transform(m) => {
                self.push_affine(m);
                self.handle_state_operator("cm")
            }

            // --- Path Construction ---
            Command::MoveTo(p) => {
                self.push_point(*p);
                self.handle_path_operator("m")
            }
            Command::LineTo(p) => {
                self.push_point(*p);
                self.handle_path_operator("l")
            }
            Command::CurveTo(p1, p2, p3) => {
                self.push_point(*p1);
                self.push_point(*p2);
                self.push_point(*p3);
                self.handle_path_operator("c")
            }
            Command::ClosePath => self.handle_path_operator("h"),
            Command::Rect(r) => {
                self.push_real(r.origin().x);
                self.push_real(r.origin().y);
                self.push_real(r.width());
                self.push_real(r.height());
                self.handle_path_operator("re")
            }
            Command::Clip(rule) => match rule {
                WindingRule::NonZero => self.handle_path_operator("W"),
                WindingRule::EvenOdd => self.handle_path_operator("W*"),
            },

            // --- Painting ---
            Command::Fill(rule) => match rule {
                WindingRule::NonZero => self.handle_painting_operator("f"),
                WindingRule::EvenOdd => self.handle_painting_operator("f*"),
            },
            Command::Stroke(_) => self.handle_painting_operator("S"),
            Command::FillStroke(rule, _) => match rule {
                WindingRule::NonZero => self.handle_painting_operator("B"),
                WindingRule::EvenOdd => self.handle_painting_operator("B*"),
            },

            // --- Text ---
            Command::BeginText
            | Command::EndText
            | Command::ShowText(_)
            | Command::ShowTextArray(_)
            | Command::SetFont { .. }
            | Command::MoveText(_)
            | Command::SetTextMatrix(_)
            | Command::SetTextRise(_)
            | Command::SetCharSpacing(_)
            | Command::SetWordSpacing(_)
            | Command::SetHorizontalScaling(_)
            | Command::SetTextRenderMode(_)
            | Command::SetWritingMode(_)
            | Command::SetTextLeading(_)
            | Command::MoveToNextLine
            | Command::Type3SetMetrics { .. } => self.handle_text_command(cmd),

            // --- Color ---
            Command::SetFillColor(color) => match color {
                fepdf_model::graphics::Color::Gray(g) => {
                    self.stack.push(Object::Real(*g));
                    self.handle_color_operator("g")
                }
                fepdf_model::graphics::Color::Rgb(r, g, b) => {
                    self.stack.push(Object::Real(*r));
                    self.stack.push(Object::Real(*g));
                    self.stack.push(Object::Real(*b));
                    self.handle_color_operator("rg")
                }
                fepdf_model::graphics::Color::Cmyk(c, m, y, k) => {
                    self.stack.push(Object::Real(*c));
                    self.stack.push(Object::Real(*m));
                    self.stack.push(Object::Real(*y));
                    self.stack.push(Object::Real(*k));
                    self.handle_color_operator("k")
                }
                fepdf_model::graphics::Color::Lab(..) => self.replay_lab(color, "rg"),
            },
            Command::SetStrokeColor(color) => match color {
                fepdf_model::graphics::Color::Gray(g) => {
                    self.stack.push(Object::Real(*g));
                    self.handle_color_operator("G")
                }
                fepdf_model::graphics::Color::Rgb(r, g, b) => {
                    self.stack.push(Object::Real(*r));
                    self.stack.push(Object::Real(*g));
                    self.stack.push(Object::Real(*b));
                    self.handle_color_operator("RG")
                }
                fepdf_model::graphics::Color::Cmyk(c, m, y, k) => {
                    self.stack.push(Object::Real(*c));
                    self.stack.push(Object::Real(*m));
                    self.stack.push(Object::Real(*y));
                    self.stack.push(Object::Real(*k));
                    self.handle_color_operator("K")
                }
                fepdf_model::graphics::Color::Lab(..) => self.replay_lab(color, "RG"),
            },
            Command::SetFillColorSpace(name) => {
                self.push_name(name);
                self.handle_color_operator("cs")
            }
            Command::SetStrokeColorSpace(name) => {
                self.push_name(name);
                self.handle_color_operator("CS")
            }

            // --- Graphics State Parameters ---
            Command::SetLineWidth(w) => {
                self.stack.push(Object::Real(*w));
                self.handle_state_operator("w")
            }
            Command::SetLineCap(cap) => {
                self.stack.push(Object::Integer(*cap as i64));
                self.handle_state_operator("J")
            }
            Command::SetLineJoin(join) => {
                self.stack.push(Object::Integer(*join as i64));
                self.handle_state_operator("j")
            }
            Command::SetMiterLimit(m) => {
                self.stack.push(Object::Real(*m));
                self.handle_state_operator("M")
            }
            Command::SetDashPattern(dash, phase) => {
                let items: Vec<Object> = dash.iter().map(|&d| Object::Real(d)).collect();
                let arr_h = self.doc.arena().alloc_array(items);
                self.stack.push(Object::Array(arr_h));
                self.stack.push(Object::Real(*phase));
                self.handle_state_operator("d")
            }

            // --- XObjects & Images ---
            Command::DrawXObject(h) => {
                let name_h = self.doc.arena().intern_name(PdfName::new(h));
                self.stack.push(Object::Name(name_h));
                self.handle_xobject_operator()
            }
            // The sublimated form of `BDC`/`EMC`. The property list arrives as an
            // `IrObject`, which has no reference variant — so a name is turned back into
            // one the resource lookup can use, and anything else becomes the `Null` that
            // `membership` reports as naming no group. Neither loses information: 8.11.2
            // requires a group to be an indirect object, and an `IrObject` that is not a
            // name was never one.
            Command::BeginMarkedContent { tag, properties } => {
                let operand = properties.as_ref().map(|ir| match ir {
                    fepdf_model::object::sublimation::IrObject::Name(name) => {
                        Object::Name(self.doc.arena().intern_name(PdfName::new(name)))
                    }
                    _ => Object::Null,
                });
                // Read `/ActualText` from the IR, before the flattening above: an inline
                // property list survives only there. `Object::Null` is all the optional
                // content code needs from an inline dictionary — 8.11.2 requires a group
                // to be indirect, so an inline one names nothing — but 14.9.4 puts real
                // text in exactly that place, and `volvo_xc90.pdf` writes 3,458 of them.
                let actual_text = self.actual_text_of(properties.as_ref(), operand.as_ref());
                let mcid = self.mcid_of(properties.as_ref(), operand.as_ref());
                self.begin_marked_content(tag, operand.as_ref(), actual_text, mcid);
                Ok(())
            }
            Command::EndMarkedContent => {
                self.end_marked_content();
                Ok(())
            }
            Command::DrawInlineImage { width, height, format, data } => {
                self.backend.draw_image(data, *width, *height, *format, None);
                Ok(())
            }

            // --- Fallback ---
            Command::RawOperator { name, operands } => {
                fn ir_to_refined(
                    ir: &fepdf_model::object::sublimation::IrObject,
                ) -> fepdf_model::refine::RefinedObject {
                    use fepdf_model::object::sublimation::IrObject;
                    use fepdf_model::refine::RefinedObject;
                    match ir {
                        IrObject::Boolean(b) => RefinedObject::Boolean(*b),
                        IrObject::Integer(i) => RefinedObject::Integer(*i),
                        IrObject::Real(f) => RefinedObject::Real(*f),
                        IrObject::String(b) => RefinedObject::String(b.clone()),
                        IrObject::Hex(b) => RefinedObject::Hex(b.clone()),
                        IrObject::Name(n) => RefinedObject::Name(fepdf_model::PdfName::new(n)),
                        IrObject::Array(a) => {
                            RefinedObject::Array(a.iter().map(ir_to_refined).collect())
                        }
                        IrObject::Dictionary(d) => {
                            let mut map = std::collections::BTreeMap::new();
                            for (k, v) in d {
                                map.insert(fepdf_model::PdfName::new(k), ir_to_refined(v));
                            }
                            RefinedObject::Dictionary(map)
                        }
                        IrObject::Null => RefinedObject::Null,
                    }
                }

                for op in operands {
                    let refined = ir_to_refined(op);
                    self.stack.push(fepdf_model::commit_to_arena(self.doc.arena(), refined, 0));
                }
                // Named, because the operand errors cannot see which operator asked for
                // them. "Expected number" on its own says nothing about where in a
                // content stream to look, and six pages of `samples/fy05.pdf` failed
                // with exactly that and nothing else.
                self.execute_operator(name).map_err(|e| {
                    PdfError::Other(format!("operator {name} {operands:?}: {e}").into())
                })
            }
        }
    }

    fn execute_operator(&mut self, op: &str) -> PdfResult<()> {
        match op {
            "m" | "l" | "c" | "v" | "y" | "re" | "h" | "W" | "W*" => self.handle_path_operator(op),
            "S" | "f" | "F" | "f*" | "n" | "b" | "b*" | "B" | "B*" | "s" => {
                self.handle_painting_operator(op)
            }
            "q" | "Q" | "cm" | "gs" => self.handle_state_operator(op),
            "g" | "G" | "rg" | "RG" | "k" | "K" | "cs" | "CS" => self.handle_color_operator(op),
            "Tc" | "Tw" | "Tz" | "TL" | "Tf" | "Tr" | "Ts" => self.handle_text_state_operator(op),
            "BT" | "ET" => self.handle_text_scope_operator(op),
            "Td" | "TD" | "Tm" | "T*" => self.handle_text_positioning_operator(op),
            "Tj" | "TJ" | "'" | "\"" => self.handle_text_showing_operator(op),
            "Do" => self.handle_xobject_operator(),
            "MP" | "DP" => self.handle_point_marked_content(op),
            "d0" => {
                let wy = self.pop_f64()?;
                let wx = self.pop_f64()?;
                self.set_type3_metrics(wx, wy)
            }
            "d1" => {
                let ury = self.pop_f64()?;
                let urx = self.pop_f64()?;
                let lly = self.pop_f64()?;
                let llx = self.pop_f64()?;
                let wy = self.pop_f64()?;
                let wx = self.pop_f64()?;
                self.set_type3_metrics_bbox(wx, wy, llx, lly, urx, ury)
            }
            "J" | "j" | "w" | "M" | "d" | "i" | "ri" => self.handle_state_operator(op),
            "SCN" | "scn" | "sc" | "SC" => self.handle_color_operator(op),
            "sh" => self.handle_shading_operator(),
            _ => {
                if !op.is_empty() {
                    self.record_unknown_operator(op);
                }
                Ok(())
            }
        }
    }

    /// Replays a `Lab` colour from a sublimated `Command` through `rg`/`RG`.
    ///
    /// This used to push `l`, `a` and `b` and then return without running an operator,
    /// so three operands were left on the stack for whatever came next — the same defect
    /// `i` and `ri` had. It logged that Lab was "not directly mappable" while
    /// `Color::to_rgb` had a Lab branch the *renderer* already used, so the two paths
    /// disagreed about a colour the engine could convert all along.
    ///
    /// 8.6.5.4 defines the space and not the conversion to a device one, so the choice
    /// of D65 sRGB is this engine's and is recorded as an `Ambiguity` rather than made
    /// silently.
    fn replay_lab(&mut self, color: &fepdf_model::graphics::Color, op: &str) -> PdfResult<()> {
        let fepdf_model::graphics::Color::Rgb(r, g, b) = color.to_rgb() else {
            return Ok(());
        };
        self.doc.record(Decision::ambiguity(
            "8.6.5.4",
            format!("a /Lab colour {color:?} where the operator set takes device components"),
            "converted through D65 sRGB and set with rg/RG; the standard defines the \
             space, not the conversion out of it",
        ));
        self.stack.push(Object::Real(r));
        self.stack.push(Object::Real(g));
        self.stack.push(Object::Real(b));
        self.handle_color_operator(op)
    }

    /// Records a content-stream operator this engine does not run.
    ///
    /// A `Decision` and not a warning on stderr (ARCHITECTURE §4.3): a caller has to be able
    /// to tell *this page drew* from *this page drew everything it asked for*, and a
    /// line on stderr cannot say which. Measured across both corpora before the change:
    /// 524 files produce six firings — three `UnknownOP` from a file built to be
    /// malformed and three runs of binary rubbish — and **none** on the nine conforming
    /// samples, so this is a signal rather than the constant §4.3 warns about.
    fn record_unknown_operator(&self, op: &str) {
        self.doc.record(Decision::violation(
            "8.2",
            format!("operator {op:?} at index {} is not one this engine runs", self.op_index),
            "skipped it and carried on with the rest of the content stream",
        ));
    }

    /// Records an operand outside the set its table defines, and what stood in for it.
    ///
    /// **Rule 20's ground, and the lint cannot reach it.** `/LC`, `/LJ` and `Tr` arrive as
    /// integers read out of a file, not as enums, so `clippy::wildcard_enum_match_arm`
    /// never sees the `match` and Rule 5 has nothing to say. The conversions used to
    /// answer an undefined value with the initial graphics state's — `J 7` a butt cap,
    /// `Tr 9` filled text — and a page that asked for something this engine does not know
    /// was drawn as though it had asked for something else, silently.
    ///
    /// The substitute is the value 8.4 gives the *initial* graphics state, which is what
    /// the conversions were already returning: this changes what is said, not what is
    /// drawn.
    fn record_undefined_enumerant(&self, clause: &'static str, table: &str, op: &str, val: i64) {
        self.doc.record(Decision::violation(
            clause,
            format!("operator {op} was given {val}, which {table} does not define"),
            "substituted the value the initial graphics state carries and drew the rest of \
             the page",
        ));
    }

    pub(crate) fn pop_i64(&mut self) -> PdfResult<i64> {
        match self.stack.pop() {
            Some(obj) => obj.as_integer().ok_or_else(|| PdfError::Other("Expected integer".into())),
            None => Err(PdfError::Other("Stack underflow".into())),
        }
    }

    pub(crate) fn pop_f64(&mut self) -> PdfResult<f64> {
        match self.stack.pop() {
            Some(obj) => obj.as_f64().ok_or_else(|| PdfError::Other("Expected number".into())),
            None => Err(PdfError::Other("Stack underflow".into())),
        }
    }

    pub(crate) fn pop_string(&mut self) -> PdfResult<bytes::Bytes> {
        match self.stack.pop() {
            Some(Object::String(s)) => Ok(s),
            Some(Object::Hex(s)) => Ok(s),
            Some(Object::Text(s)) => Ok(bytes::Bytes::copy_from_slice(s.as_bytes())),
            _ => Err(PdfError::Other("Expected string".into())),
        }
    }

    pub(crate) fn pop_array(&mut self) -> PdfResult<Handle<Vec<Object>>> {
        match self.stack.pop() {
            Some(Object::Array(a)) => Ok(a),
            _ => Err(PdfError::Other("Expected array".into())),
        }
    }

    pub(crate) fn pop_name(&mut self) -> PdfResult<PdfName> {
        match self.stack.pop() {
            Some(Object::Name(h)) => self
                .doc
                .arena()
                .get_name(h)
                .ok_or_else(|| PdfError::Other("Invalid name handle".into())),
            _ => Err(PdfError::Other("Expected name".into())),
        }
    }

    /// Runs a nested content stream — a form XObject, a Type 3 glyph — with a
    /// marked-content stack of its own.
    ///
    /// A stream that leaves sections open, or closes ones it never opened, must not
    /// change what happens after the `Do` that invoked it. The *canvas* depth is carried
    /// in rather than reset: a form invoked from inside a hidden section is still hidden,
    /// and it is restored afterwards so an unbalanced `EMC` inside the form cannot bring
    /// the enclosing section back.
    pub(crate) fn in_nested_content<T>(
        &mut self,
        run: impl FnOnce(&mut Self) -> PdfResult<T>,
    ) -> PdfResult<T> {
        let enclosing = std::mem::take(&mut self.marked_sections);
        let hidden = self.backend.hidden_depth();
        let outcome = run(self);
        self.marked_sections = enclosing;
        self.backend.restore_hidden_depth(hidden);
        outcome
    }

    /// The `/Font` entries the current resource scope names, for the sublimator.
    ///
    /// **Loaded rather than resolved lazily**, which is the one thing this costs. The
    /// interpreter resolves a font when a `Tf` selects it; the sublimator wants the map up
    /// front, because it emits `SetWritingMode` beside `SetFont` and records a 9.6.2
    /// repair for a name the resources do not define. Only streams that were not refined
    /// reach here, so a page whose contents ingestion already turned into `Command`s pays
    /// nothing.
    fn resource_fonts(&self) -> BTreeMap<String, std::sync::Arc<fepdf_model::font::FontResource>> {
        let arena = self.doc.arena();
        let font_key = arena.intern_name(PdfName::new("Font"));
        let mut fonts = BTreeMap::new();
        for &res_dh in self.resource_stack.iter().rev() {
            let Some(dict) = arena.get_dict(res_dh) else { continue };
            let Some(font_dh) = dict.get(&font_key).and_then(|o| o.resolve(arena).as_dict_handle())
            else {
                continue;
            };
            let Some(font_dict) = arena.get_dict(font_dh) else { continue };
            for (name_h, entry) in &font_dict {
                let Some(name) = arena.get_name(*name_h) else { continue };
                let handle =
                    entry.as_reference().unwrap_or_else(|| arena.alloc_object(entry.clone()));
                if let Ok(res) = self.doc.get_font(handle) {
                    fonts.entry(name.as_str().to_string()).or_insert(res);
                }
            }
        }
        fonts
    }

    pub(crate) fn find_resource(
        &self,
        res_type: &Handle<PdfName>,
        name: &PdfName,
    ) -> PdfResult<Object> {
        let res_type_key = *res_type;
        let name_handle = self.doc.arena().intern_name(name.clone());

        for &res_dh in self.resource_stack.iter().rev() {
            let dict = self
                .doc
                .arena()
                .get_dict(res_dh)
                .ok_or_else(|| PdfError::Other("Invalid resource dict handle".into()))?;

            if let Some(entry) =
                dict.get(&res_type_key).and_then(|o| o.resolve(self.doc.arena()).as_dict_handle())
            {
                let res_dict = self
                    .doc
                    .arena()
                    .get_dict(entry)
                    .ok_or_else(|| PdfError::Other("Invalid resource type dict".into()))?;
                if let Some(res) = res_dict.get(&name_handle) {
                    return Ok(res.clone());
                }
            }
        }
        Err(PdfError::Other(format!("Resource not found: {:?} {}", res_type, name.as_str()).into()))
    }
}
