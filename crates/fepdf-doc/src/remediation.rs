#![allow(clippy::cast_sign_loss, clippy::cast_possible_truncation, clippy::cast_possible_wrap)]

use fepdf_content::Interpreter;
use fepdf_content::{FallbackFontType, RenderBackend, TextGlyph, TextState};
use fepdf_model::graphics::{BlendMode, Color, PixelFormat, StrokeStyle, WindingRule};
use fepdf_model::{Document, Handle, Object, PdfResult};
use kurbo::{Affine, BezPath};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A single span of text with its associated styling and positioning.
#[derive(Debug, Clone)]
pub struct TextSpan {
    /// The textual content of the span.
    pub text: String,
    /// Effective font size in points.
    pub font_size: f64,
    /// True if the font style is bold.
    pub is_bold: bool,
    /// X coordinate in page space.
    pub x: f64,
    /// Y coordinate in page space.
    pub y: f64,
    /// Total advance width of the span.
    pub width: f64,
    /// Index of the operation in the content stream.
    pub op_index: usize,
}

/// A backend that extracts text content from a page.
pub struct TextExtractionBackend {
    output: String,
    last_y: f64,
    /// Glyphs handed to this backend, and how many of them carried no character.
    ///
    /// A glyph the interpreter drew but could not name in Unicode contributes nothing to
    /// the output, so a page of them extracts as an empty string — **the same value a
    /// blank page extracts as.** Measured before this counted: 192 of
    /// `bokutokitan.pdf`'s 195 pages, 64,556 glyphs, indistinguishable from blank.
    glyphs_seen: usize,
    /// How many of `glyphs_seen` had an empty `unicode`.
    glyphs_unmapped: usize,
    /// Which route was reached, and failed, for each of them.
    ///
    /// A count says how much was lost; this says where to go for it. Measured on
    /// `bokutokitan.pdf`, "213 of 360" was the whole of what could be reported and it
    /// named no work.
    unmapped_by_source: std::collections::BTreeMap<&'static str, usize>,
}

impl Default for TextExtractionBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TextExtractionBackend {
    /// Creates a new empty text aggregator.
    pub fn new() -> Self {
        Self {
            output: String::new(),
            last_y: 0.0,
            glyphs_seen: 0,
            glyphs_unmapped: 0,
            unmapped_by_source: std::collections::BTreeMap::new(),
        }
    }
    /// Finalizes the aggregation and returns the accumulated string.
    pub fn finish(self) -> String {
        self.output
    }
}

impl RenderBackend for TextExtractionBackend {
    fn transform(&mut self, _affine: Affine) {}
    fn set_transform(&mut self, _affine: Affine) {}
    fn push_state(&mut self) {}
    fn pop_state(&mut self) {}
    fn fill_path(&mut self, _path: &BezPath, _color: &Color, _rule: WindingRule) {}
    fn stroke_path(&mut self, _path: &BezPath, _color: &Color, _style: &StrokeStyle) {}
    fn push_clip(&mut self, _path: &BezPath, _rule: WindingRule) {}
    fn pop_clip(&mut self) {}
    fn draw_image(
        &mut self,
        _data: &[u8],
        _width: u32,
        _height: u32,
        _format: PixelFormat,
        _smask: Option<fepdf_content::SMaskData>,
    ) {
    }
    fn set_fill_alpha(&mut self, _alpha: f64) {}
    fn set_stroke_alpha(&mut self, _alpha: f64) {}
    fn set_fill_color(&mut self, _color: Color) {}
    fn set_stroke_color(&mut self, _color: Color) {}
    fn set_blend_mode(&mut self, _mode: BlendMode) {}
    fn define_font(
        &mut self,
        _name: &str,
        _base: Option<&str>,
        _data: Option<Arc<Vec<u8>>>,
        _index: Option<usize>,
        _cid_map: Option<BTreeMap<u32, u32>>,
        _fallback: FallbackFontType,
        _is_cid: bool,
    ) {
    }
    fn set_font(&mut self, _name: &str) {}
    fn show_text(
        &mut self,
        glyphs: &[TextGlyph],
        _size: f64,
        transform: Affine,
        _state: TextState,
        _op_index: usize,
    ) {
        let coeffs = transform.as_coeffs();
        let y = coeffs[5];
        if (y - self.last_y).abs() > 5.0 && !self.output.is_empty() {
            self.output.push('\n');
        }
        for glyph in glyphs {
            self.glyphs_seen += 1;
            if glyph.unicode.is_empty() {
                self.glyphs_unmapped += 1;
                *self.unmapped_by_source.entry(glyph.source.name()).or_default() += 1;
            }
            self.output.push_str(&glyph.unicode);
        }
        self.last_y = y;
    }

    /// What this extraction could not say, for `render_page` to record.
    ///
    /// **The count is the whole point.** "No text on this page" is a legitimate answer
    /// for a blank page and for one drawn entirely in vector paths — measured, 24 of
    /// `fugaku.pdf`'s 25 pages are the second — and it is a defect when glyphs were drawn
    /// and none of them could be named. Only the caller can tell those apart, and only if
    /// it is told which happened.
    ///
    /// The missing-resource case is named separately because it is the one a reader can
    /// act on: the CMap collections are not carried by this crate, and a machine without
    /// them cannot map an Adobe-Japan1 CID to a character no matter what the file says.
    fn take_decisions(&mut self) -> Vec<fepdf_model::interpretation::Decision> {
        if self.glyphs_unmapped == 0 {
            return Vec::new();
        }
        let unmapped = self.glyphs_unmapped;
        let seen = self.glyphs_seen;
        let action = if fepdf_model::resources::locate(fepdf_model::resources::Resource::Cmaps)
            .is_none()
        {
            format!(
                "left them out of the extracted text; {}",
                fepdf_model::resources::not_found_message(fepdf_model::resources::Resource::Cmaps)
            )
        } else {
            "left them out of the extracted text; the font supplied no /ToUnicode and no              CMap collection this engine carries names these codes"
                .to_string()
        };
        let routes = self
            .unmapped_by_source
            .iter()
            .map(|(route, count)| format!("{count} reached {route}"))
            .collect::<Vec<_>>()
            .join(", ");
        vec![fepdf_model::interpretation::Decision::violation(
            "9.10.2",
            format!(
                "{unmapped} of {seen} glyphs drawn on this page have no Unicode value ({routes})"
            ),
            action,
        )]
    }

    fn set_text_render_mode(&mut self, _mode: fepdf_model::graphics::TextRenderingMode) {}
    fn set_char_spacing(&mut self, _spacing: f64) {}
    fn set_word_spacing(&mut self, _spacing: f64) {}
}

/// A backend that collects structured text spans with styling and coordinates.
pub struct CollectorBackend {
    /// The list of collected text spans.
    pub spans: Vec<TextSpan>,
    /// Mappings from local font names to base font family names.
    pub fonts: BTreeMap<String, String>,
    /// The name of the font currently active in the text state.
    pub current_font: Option<String>,
    /// The current transformation matrix from graphics state (CTM).
    pub current_transform: Affine,
    /// Stack of transformation matrices for push/pop state.
    pub transform_stack: Vec<Affine>,
}

impl RenderBackend for CollectorBackend {
    fn transform(&mut self, matrix: Affine) {
        self.current_transform *= matrix;
    }
    fn set_transform(&mut self, matrix: Affine) {
        self.current_transform = matrix;
    }
    fn push_state(&mut self) {
        self.transform_stack.push(self.current_transform);
    }
    fn pop_state(&mut self) {
        if let Some(t) = self.transform_stack.pop() {
            self.current_transform = t;
        }
    }
    fn push_clip(&mut self, _path: &BezPath, _rule: WindingRule) {}
    fn pop_clip(&mut self) {}
    fn draw_image(
        &mut self,
        _data: &[u8],
        _w: u32,
        _h: u32,
        _fmt: PixelFormat,
        _smask: Option<fepdf_content::SMaskData>,
    ) {
    }
    fn fill_path(&mut self, _path: &BezPath, _color: &Color, _rule: WindingRule) {}
    fn stroke_path(&mut self, _path: &BezPath, _color: &Color, _style: &StrokeStyle) {}
    fn set_fill_alpha(&mut self, _alpha: f64) {}
    fn set_stroke_alpha(&mut self, _alpha: f64) {}
    fn set_fill_color(&mut self, _color: Color) {}
    fn set_stroke_color(&mut self, _color: Color) {}
    fn set_blend_mode(&mut self, _mode: BlendMode) {}
    fn set_font(&mut self, name: &str) {
        self.current_font = Some(name.to_string());
    }
    fn show_text(
        &mut self,
        glyphs: &[TextGlyph],
        size: f64,
        transform: Affine,
        _state: TextState,
        op_index: usize,
    ) {
        let total_transform = self.current_transform * transform;
        let coeffs = total_transform.as_coeffs();
        let mut is_bold = false;
        let mut width = 0.0;
        let mut text = String::new();
        if let Some(font_name) = &self.current_font
            && let Some(base_font) = self.fonts.get(font_name)
        {
            let name_lower = base_font.to_lowercase();
            is_bold = name_lower.contains("bold")
                || name_lower.contains("heavy")
                || name_lower.contains("black");
        }
        for glyph in glyphs {
            let raw_width = if glyph.width.abs() < f32::EPSILON {
                let is_cjk = glyph.unicode.chars().any(|c| (c as u32) >= 0x2E80);
                if is_cjk { 1000.0 } else { 500.0 }
            } else {
                glyph.width
            };
            let adv_scaled = (f64::from(raw_width) * size) / 1000.0;
            width += adv_scaled;
            text.push_str(&glyph.unicode);
        }
        self.spans.push(TextSpan {
            text,
            font_size: size * coeffs[3],
            is_bold,
            x: coeffs[4],
            y: coeffs[5],
            width,
            op_index,
        });
    }
    fn define_font(
        &mut self,
        name: &str,
        base: Option<&str>,
        _data: Option<Arc<Vec<u8>>>,
        _index: Option<usize>,
        _cid_map: Option<BTreeMap<u32, u32>>,
        _fallback: FallbackFontType,
        _is_cid: bool,
    ) {
        if let Some(base_name) = base {
            self.fonts.insert(name.to_string(), base_name.to_string());
        }
    }
    fn set_text_render_mode(&mut self, _mode: fepdf_model::graphics::TextRenderingMode) {}
    fn set_char_spacing(&mut self, _spacing: f64) {}
    fn set_word_spacing(&mut self, _spacing: f64) {}
}

impl Default for CollectorBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CollectorBackend {
    /// Creates a new empty CollectorBackend.
    pub fn new() -> Self {
        Self {
            spans: Vec::new(),
            fonts: BTreeMap::new(),
            current_font: None,
            current_transform: Affine::IDENTITY,
            transform_stack: Vec::new(),
        }
    }
}

/// Heuristic engine for inferring logical structure from flat PDF content.
pub struct HeuristicEngine;

impl Default for HeuristicEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl HeuristicEngine {
    /// Creates a new heuristic engine.
    pub fn new() -> Self {
        Self
    }
    /// Analyzes the document to identify potential structural improvements.
    pub fn infer_structure(&self, doc: &Document) -> PdfResult<Vec<RemediationCandidate>> {
        let mut candidates = Vec::new();
        let page_count = doc.page_count()?;
        for i in 0..page_count {
            let page = doc.get_page(i)?;
            let mut collector = CollectorBackend::new();
            let res_dh = page.resources_handle();
            let mut interpreter =
                Interpreter::new(&mut collector, doc, res_dh, kurbo::Affine::IDENTITY);
            if let Some(contents) = page.resolve_attribute("Contents") {
                let data = doc.decode_stream(&contents)?;
                let _ = interpreter.execute_raw(&data);
            }
            candidates.extend(self.detect_headings(i, &collector.spans)?);
            candidates.extend(self.detect_tables(i, &collector.spans)?);
            candidates.extend(self.cluster_paragraphs(i, &collector.spans)?);
        }
        Ok(candidates)
    }

    fn detect_headings(
        &self,
        page_index: usize,
        spans: &[TextSpan],
    ) -> PdfResult<Vec<RemediationCandidate>> {
        if spans.is_empty() {
            return Ok(Vec::new());
        }
        let (body_size, heading_sizes) = self.calculate_font_size_stats(spans);
        let mut candidates = Vec::new();
        for (i, span) in spans.iter().enumerate() {
            if let Some(cand) = self.infer_heading(page_index, i, span, body_size, &heading_sizes) {
                candidates.push(cand);
            }
        }
        Ok(candidates)
    }

    fn calculate_font_size_stats(&self, spans: &[TextSpan]) -> (i32, Vec<i32>) {
        let mut size_counts: BTreeMap<i32, usize> = BTreeMap::new();
        for span in spans {
            let s = (span.font_size * 10.0) as i32;
            *size_counts.entry(s).or_default() += 1;
        }
        let body_size = size_counts.iter().max_by_key(|&(_, count)| count).map_or(120, |(&s, _)| s);
        let mut heading_sizes: Vec<i32> =
            size_counts.keys().filter(|&&s| s > body_size + 15).copied().collect();
        heading_sizes.sort_by(|a, b| b.cmp(a));
        (body_size, heading_sizes)
    }

    fn infer_heading(
        &self,
        page_index: usize,
        span_index: usize,
        span: &TextSpan,
        body_size: i32,
        heading_sizes: &[i32],
    ) -> Option<RemediationCandidate> {
        let s = (span.font_size * 10.0) as i32;
        if let Some(rank) = heading_sizes.iter().position(|&hs| hs == s) {
            let level = (rank + 1).min(6) as u8;
            return Some(RemediationCandidate {
                description: format!(
                    "Set '{}' as Heading Level {} ({}pt)",
                    span.text.trim(),
                    level,
                    span.font_size
                ),
                page_index,
                action_type: RemediationActionType::SetHeading {
                    text: span.text.clone(),
                    level,
                    span_indices: vec![span_index],
                },
            });
        }
        if span.is_bold && s >= body_size - 5 {
            return Some(RemediationCandidate {
                description: format!("Set '{}' as Heading Level 6 (Bold Body)", span.text.trim()),
                page_index,
                action_type: RemediationActionType::SetHeading {
                    text: span.text.clone(),
                    level: 6,
                    span_indices: vec![span_index],
                },
            });
        }
        None
    }

    fn detect_tables(
        &self,
        page_index: usize,
        spans: &[TextSpan],
    ) -> PdfResult<Vec<RemediationCandidate>> {
        let mut candidates = Vec::new();
        if spans.len() < 4 {
            return Ok(candidates);
        }
        let mut lines: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
        for (i, span) in spans.iter().enumerate() {
            let y = (span.y * 10.0) as i32;
            lines.entry(y).or_default().push(i);
        }
        let mut potential_table_rows = 0;
        let mut prev_cols = Vec::new();
        let mut all_span_indices = Vec::new();
        for (_y, line_spans) in lines {
            let mut current_cols: Vec<i32> =
                line_spans.iter().map(|&idx| (spans[idx].x * 10.0) as i32).collect();
            current_cols.sort_unstable();
            if current_cols.len() > 1 && !prev_cols.is_empty() {
                let mut matches = 0;
                for &c in &current_cols {
                    for &pc in &prev_cols {
                        if i32::abs(pc - c) < 50 {
                            matches += 1;
                            break;
                        }
                    }
                }
                if matches > 1 {
                    potential_table_rows += 1;
                    all_span_indices.extend(line_spans);
                }
            }
            prev_cols = current_cols;
        }
        if potential_table_rows > 2 {
            candidates.push(RemediationCandidate {
                description: format!("Form table from {potential_table_rows} aligned rows"),
                page_index,
                action_type: RemediationActionType::CreateTable {
                    rows: potential_table_rows as usize,
                    span_indices: all_span_indices,
                },
            });
        }
        Ok(candidates)
    }

    fn cluster_paragraphs(
        &self,
        page_index: usize,
        spans: &[TextSpan],
    ) -> PdfResult<Vec<RemediationCandidate>> {
        let mut candidates = Vec::new();
        let mut current_indices = Vec::new();
        let mut paragraphs = 0;
        let mut last_y = 0.0;
        for (i, span) in spans.iter().enumerate() {
            if (span.y - last_y).abs() > 20.0 && !current_indices.is_empty() {
                paragraphs += 1;
                candidates.push(RemediationCandidate {
                    description: format!(
                        "Cluster paragraph {} with {} spans",
                        paragraphs,
                        current_indices.len()
                    ),
                    page_index,
                    action_type: RemediationActionType::ClusterParagraphs {
                        count: current_indices.len(),
                        span_indices: current_indices.clone(),
                    },
                });
                current_indices.clear();
            }
            current_indices.push(i);
            last_y = span.y;
        }
        Ok(candidates)
    }

    /// Applies a set of identified structural remediations to the document.
    pub fn apply_remediations(
        &self,
        doc: &Document,
        candidates: Vec<RemediationCandidate>,
    ) -> PdfResult<()> {
        let mut page_groups: BTreeMap<usize, Vec<RemediationCandidate>> = BTreeMap::new();
        for c in candidates {
            page_groups.entry(c.page_index).or_default().push(c);
        }
        let mut struct_elements = Vec::new();
        for (page_idx, page_candidates) in page_groups {
            struct_elements.extend(self.apply_page_remediations(doc, page_idx, page_candidates)?);
        }
        self.finalize_struct_tree(doc, struct_elements)
    }

    fn apply_page_remediations(
        &self,
        doc: &Document,
        page_idx: usize,
        candidates: Vec<RemediationCandidate>,
    ) -> PdfResult<Vec<Object>> {
        let page = doc.get_page(page_idx)?;
        let arena = doc.arena();
        let mut collector = CollectorBackend::new();
        let res_dh = page.resources_handle();
        let mut interpreter =
            Interpreter::new(&mut collector, doc, res_dh, kurbo::Affine::IDENTITY);
        if let Some(contents) = page.resolve_attribute("Contents") {
            interpreter.execute_raw(&doc.decode_stream(&contents)?)?;
        }
        let mut op_to_mcid = BTreeMap::new();
        let mut page_struct_elements = Vec::new();
        for (mcid, cand) in candidates.into_iter().enumerate() {
            let tag = match &cand.action_type {
                RemediationActionType::SetHeading { level, .. } => format!("H{level}"),
                RemediationActionType::CreateTable { .. } => "Table".to_string(),
                RemediationActionType::ClusterParagraphs { .. } => "P".to_string(),
            };
            let span_indices = match &cand.action_type {
                RemediationActionType::SetHeading { span_indices, .. }
                | RemediationActionType::CreateTable { span_indices, .. }
                | RemediationActionType::ClusterParagraphs { span_indices, .. } => span_indices,
            };
            for &idx in span_indices {
                if let Some(span) = collector.spans.get(idx) {
                    op_to_mcid.insert(span.op_index, (tag.clone(), mcid as i32));
                }
            }
            let page_ref = page.obj_handle();
            let mut elem_dict = BTreeMap::new();
            elem_dict.insert(arena.name("Type"), Object::Name(arena.name("StructElem")));
            elem_dict.insert(arena.name("S"), Object::Name(arena.name(&tag)));
            elem_dict.insert(arena.name("Pg"), Object::Reference(page_ref));
            elem_dict.insert(arena.name("K"), Object::Integer(mcid as i64));
            let elem_h = arena.alloc_dict(elem_dict);
            page_struct_elements
                .push(Object::Reference(arena.alloc_object(Object::Dictionary(elem_h))));
        }
        self.update_page_contents(doc, page.obj_handle(), op_to_mcid)?;
        Ok(page_struct_elements)
    }

    fn update_page_contents(
        &self,
        doc: &Document,
        page_obj_h: Handle<Object>,
        op_to_mcid: BTreeMap<usize, (String, i32)>,
    ) -> PdfResult<()> {
        let arena = doc.arena();
        let page_dh = doc.resolve_to_dict(page_obj_h)?;
        let page_dict = arena.get_dict(page_dh).unwrap_or_default();
        if let Some(contents) = page_dict.get(&arena.name("Contents")) {
            let data = decode_page_contents(doc, contents)?;
            let rewriter = fepdf_model::content::ContentRewriter::new(arena, data);
            let mut mapping_refs = BTreeMap::new();
            for (k, (v1, v2)) in &op_to_mcid {
                mapping_refs.insert(*k, (v1.as_str(), *v2));
            }
            let new_data = rewriter.insert_mcids(mapping_refs)?;
            let mut stream_dict = BTreeMap::new();
            stream_dict.insert(arena.name("Length"), Object::Integer(new_data.len() as i64));
            let new_contents = arena.alloc_object(Object::Stream(
                arena.alloc_dict(stream_dict),
                std::sync::Arc::new(fepdf_model::object::SublimatedData::Raw(bytes::Bytes::from(
                    new_data,
                ))),
            ));
            let mut updated_dict = arena.get_dict(page_dh).unwrap_or_default();
            updated_dict.insert(arena.name("Contents"), Object::Reference(new_contents));
            updated_dict.insert(arena.name("Tabs"), Object::Name(arena.name("S")));
            arena.set_dict(page_dh, updated_dict);
        }
        Ok(())
    }

    fn finalize_struct_tree(&self, doc: &Document, struct_elements: Vec<Object>) -> PdfResult<()> {
        let arena = doc.arena();
        let mut root_dict = BTreeMap::new();
        root_dict.insert(arena.name("Type"), Object::Name(arena.name("StructTreeRoot")));
        root_dict.insert(arena.name("K"), Object::Array(arena.alloc_array(struct_elements)));
        let root_ref = arena.alloc_object(Object::Dictionary(arena.alloc_dict(root_dict)));
        if let Some(cah) = doc.catalog_handle() {
            let cadh = doc.resolve_to_dict(cah)?;
            let mut catalog = arena.get_dict(cadh).unwrap_or_default();
            catalog.insert(arena.name("StructTreeRoot"), Object::Reference(root_ref));
            let mut mark_info = BTreeMap::new();
            mark_info.insert(arena.name("Marked"), Object::Boolean(true));
            catalog.insert(arena.name("MarkInfo"), Object::Dictionary(arena.alloc_dict(mark_info)));
            arena.set_dict(cadh, catalog);
        }
        Ok(())
    }
}

/// A proposed structural remediation for a specific page.
#[derive(Debug, Clone)]
pub struct RemediationCandidate {
    /// Human-readable description of the change.
    pub description: String,
    /// Index of the page to apply to.
    pub page_index: usize,
    /// The specific action to perform.
    pub action_type: RemediationActionType,
}

/// Types of structural modifications supported by the heuristic engine.
#[derive(Debug, Clone)]
pub enum RemediationActionType {
    /// Convert text spans into a logical heading.
    SetHeading {
        /// Normalized heading text.
        text: String,
        /// Heading level (1-6).
        level: u8,
        /// Indices of the source spans.
        span_indices: Vec<usize>,
    },
    /// Group text spans into a logical table structure.
    CreateTable {
        /// Number of rows detected.
        rows: usize,
        /// Indices of the source spans.
        span_indices: Vec<usize>,
    },
    /// Cluster disconnected spans into a single paragraph.
    ClusterParagraphs {
        /// Number of paragraphs detected.
        count: usize,
        /// Indices of the source spans.
        span_indices: Vec<usize>,
    },
}

/// Automatically infers and applies structural tags to a document.
pub fn retag(doc: &mut Document) -> PdfResult<()> {
    let engine = HeuristicEngine::new();
    let candidates = engine.infer_structure(doc)?;
    engine.apply_remediations(doc, candidates)?;
    Ok(())
}

impl HeuristicEngine {
    fn check_span_intersection(span: &TextSpan, rect: &[f32; 4]) -> bool {
        let span_min_x = span.x;
        let span_max_x = span.x + span.width;
        let span_min_y = span.y;
        let span_max_y = span.y + span.font_size; // approximate height

        let r_min_x = f64::from(rect[0]);
        let r_min_y = f64::from(rect[1]);
        let r_max_x = f64::from(rect[2]);
        let r_max_y = f64::from(rect[3]);

        span_min_x < r_max_x && span_max_x > r_min_x && span_min_y < r_max_y && span_max_y > r_min_y
    }

    fn collect_redacted_op_indices(
        spans: &[TextSpan],
        redacted_rects: &[[f32; 4]],
    ) -> std::collections::BTreeSet<usize> {
        let mut redacted_op_indices = std::collections::BTreeSet::new();
        for span in spans {
            for rect in redacted_rects {
                if Self::check_span_intersection(span, rect) {
                    redacted_op_indices.insert(span.op_index);
                }
            }
        }
        redacted_op_indices
    }

    fn rewrite_redacted_tokens(
        data: bytes::Bytes,
        redacted_op_indices: &std::collections::BTreeSet<usize>,
    ) -> Vec<u8> {
        use fepdf_model::lexer::{Lexer, Token};
        let mut lexer = Lexer::new(data);
        let mut output = Vec::new();
        let mut op_index = 0;

        while let Ok(token) = lexer.next_token() {
            if token == Token::EOF {
                break;
            }

            if let Token::Keyword(_) = &token {
                token.write_to(&mut output);
                op_index += 1;
            } else if matches!(token, Token::String(_) | Token::Hex(_)) {
                if redacted_op_indices.contains(&op_index) {
                    let redacted_tok = Token::String(bytes::Bytes::from("[REDACTED]"));
                    redacted_tok.write_to(&mut output);
                } else {
                    token.write_to(&mut output);
                }
                op_index += 1;
            } else {
                token.write_to(&mut output);
            }
        }
        output
    }
}

/// Physically scrubs content streams inside specified redaction rectangles on a page (Atomic Redaction).
pub fn apply_physical_redaction_to_page(
    doc: &Document,
    page_index: usize,
    redacted_rects: &[[f32; 4]],
) -> PdfResult<()> {
    if redacted_rects.is_empty() {
        return Ok(());
    }

    let page = doc.get_page(page_index)?;
    let arena = doc.arena();
    let page_dh = doc.resolve_to_dict(page.obj_handle())?;
    let page_dict = arena.get_dict(page_dh).unwrap_or_default();

    if let Some(contents) = page_dict.get(&arena.name("Contents")) {
        let data = decode_page_contents(doc, contents)?;

        // 1. Collect text spans with op_indices
        let mut collector = CollectorBackend::new();
        let res_dh = page.resources_handle();
        let mut interpreter =
            Interpreter::new(&mut collector, doc, res_dh, kurbo::Affine::IDENTITY);
        let _ = interpreter.execute_raw(&data);

        // 2. Identify which op_indices intersect the redacted rectangles
        let redacted_op_indices =
            HeuristicEngine::collect_redacted_op_indices(&collector.spans, redacted_rects);

        if redacted_op_indices.is_empty() {
            return Ok(());
        }

        // 3. Rewrite content stream tokens to scrub redacted string values
        let output = HeuristicEngine::rewrite_redacted_tokens(data, &redacted_op_indices);

        // 4. Write modified contents stream back into PdfArena
        let mut stream_dict = std::collections::BTreeMap::new();
        stream_dict.insert(arena.name("Length"), Object::Integer(output.len() as i64));
        let new_contents = arena.alloc_object(Object::Stream(
            arena.alloc_dict(stream_dict),
            std::sync::Arc::new(fepdf_model::object::SublimatedData::Raw(bytes::Bytes::from(
                output,
            ))),
        ));

        let mut updated_dict = arena.get_dict(page_dh).unwrap_or_default();
        updated_dict.insert(arena.name("Contents"), Object::Reference(new_contents));
        arena.set_dict(page_dh, updated_dict);
    }

    Ok(())
}

fn decode_page_contents(doc: &Document, contents: &Object) -> PdfResult<bytes::Bytes> {
    let arena = doc.arena();
    let resolved = match contents {
        Object::Reference(h) => doc.resolve(h)?,
        _ => contents.clone(),
    };
    match resolved {
        Object::Array(ah) => {
            let mut merged = Vec::new();
            if let Some(arr) = arena.get_array(ah) {
                for item in arr {
                    let resolved_item = match item {
                        Object::Reference(h) => doc.resolve(&h)?,
                        _ => item,
                    };
                    let part = doc.decode_stream(&resolved_item)?;
                    merged.extend_from_slice(&part);
                    merged.push(b'\n');
                }
            }
            Ok(bytes::Bytes::from(merged))
        }
        _ => doc.decode_stream(&resolved),
    }
}
