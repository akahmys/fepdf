use crate::RenderBackend;
use crate::path::PathBuilder;
use fepdf_model::graphics::{GraphicsState, Rect, TextMatrices, WindingRule};
use fepdf_model::lexer::Token;
use fepdf_model::object::sublimation::Command;
use fepdf_model::parser::Parser;
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
    /// The rendering backend used to draw items.
    pub(crate) backend: &'a mut dyn RenderBackend,
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
    /// Bounding box of the current text object (between BT and ET).
    pub current_text_bbox: Option<Rect>,
    /// Bounding box of all text objects combined on the page.
    pub page_text_bbox: Option<Rect>,
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
    /// The page's area in square points, when the caller knows which page this is.
    ///
    /// Only one thing reads it: an image the engine cannot decode reports **how much of
    /// the page it would have covered**, which is the difference between "a picture was
    /// dropped" and "62% of this page is missing". Optional because the interpreter also
    /// runs over form XObjects and Type 3 glyph streams, where there is no page to be a
    /// fraction of.
    pub(crate) page_area: Option<f64>,
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
            backend,
            doc,
            resource_stack: vec![initial_resources],
            stack: Vec::new(),
            path: PathBuilder::new(),
            pending_clip: None,
            state_stack: Vec::new(),
            state,
            text_matrices: None,
            current_text_bbox: None,
            page_text_bbox: None,
            defined_fonts: BTreeSet::new(),
            font_name_map: BTreeMap::new(),
            op_index: 0,
            type3_advance: None,
            in_type3_glyph: false,
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
            let data = self.doc.arena().get_stream_bytes(&sublimated)?;
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
                let data = self.doc.arena().get_stream_bytes(&sublimated)?;
                self.execute_raw(&data)
            }
        }
    }

    /// Executes a raw PDF content stream, tokenizing and dispatching each operator.
    pub fn execute_raw(&mut self, data: &[u8]) -> PdfResult<()> {
        if data.is_empty() {
            return Ok(());
        }

        let mut parser = Parser::new(bytes::Bytes::copy_from_slice(data), self.doc.arena());

        while let Ok(token) = parser.peek() {
            if token == Token::EOF {
                break;
            }
            match token {
                Token::Keyword(ref op) => {
                    let op_str = op.clone();

                    let _ = parser.next_token()?; // Consume operator
                    if self.in_type3_glyph {
                        log::debug!("[TYPE3] op={}, stack={:?}", op_str, self.stack);
                    }
                    self.op_index += 1;
                    // Named, because the operand errors below cannot see which operator
                    // asked for them. "Expected number" on its own says nothing about
                    // where in a content stream to look, and six pages of
                    // `samples/fy05.pdf` failed with exactly that and nothing else.
                    self.execute_operator(&op_str).map_err(|e| {
                        PdfError::Other(
                            format!("operator {op_str} at {}: {e}", self.op_index).into(),
                        )
                    })?;
                }
                _ => {
                    let obj = parser.parse_object()?;
                    self.stack.push(obj);
                }
            }
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
                fepdf_model::graphics::Color::Lab(l, a, b) => {
                    self.stack.push(Object::Real(*l));
                    self.stack.push(Object::Real(*a));
                    self.stack.push(Object::Real(*b));
                    log::warn!(
                        "[SDK] Lab color in Command::SetFillColor not directly mappable to operator"
                    );
                    Ok(())
                }
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
                fepdf_model::graphics::Color::Lab(l, a, b) => {
                    self.stack.push(Object::Real(*l));
                    self.stack.push(Object::Real(*a));
                    self.stack.push(Object::Real(*b));
                    log::warn!(
                        "[SDK] Lab color in Command::SetStrokeColor not directly mappable to operator"
                    );
                    Ok(())
                }
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
            Command::BeginMarkedContent { .. } | Command::EndMarkedContent => Ok(()),
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
            "BMC" | "BDC" | "EMC" | "MP" | "DP" => self.handle_marked_content_operator(op),
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
            "J" | "j" | "w" | "M" | "d" | "i" => self.handle_state_operator(op),
            "SCN" | "scn" | "sc" | "SC" => self.handle_color_operator(op),
            "sh" => self.handle_shading_operator(),
            _ => {
                if !op.is_empty() {
                    log::warn!("Unknown or unhandled operator: {op}");
                }
                Ok(())
            }
        }
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
