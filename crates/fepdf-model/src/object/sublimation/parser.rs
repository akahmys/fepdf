//! Sublimation parser for PDF Content Streams.

use super::{Command, IrObject};
use crate::font::FontResource;
use crate::graphics::{Color, StrokeStyle, TextRenderingMode, WindingRule};
use crate::lexer::{Lexer, Token};
use crate::object::PdfName;
use kurbo::{Affine, Point, Rect};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A stateful sublimator for converting raw content stream tokens into structured IR.
pub struct Sublimator<'a> {
    fonts: &'a BTreeMap<String, Arc<FontResource>>,
    stack: Vec<IrObject>,
    current_font: Option<Arc<FontResource>>,
    fill_color_space: crate::graphics::ColorSpaceKind,
    stroke_color_space: crate::graphics::ColorSpaceKind,
    current_stroke_style: crate::graphics::StrokeStyle,
    decisions: Vec<crate::interpretation::Decision>,
}

impl<'a> Sublimator<'a> {
    /// Prepares a sublimator that resolves fonts through `fonts`.
    pub fn new(fonts: &'a BTreeMap<String, Arc<FontResource>>) -> Self {
        Self {
            fonts,
            stack: Vec::new(),
            current_font: None,
            decisions: Vec::new(),
            fill_color_space: crate::graphics::ColorSpaceKind::DeviceGray,
            stroke_color_space: crate::graphics::ColorSpaceKind::DeviceGray,
            current_stroke_style: crate::graphics::StrokeStyle {
                width: 1.0,
                cap: crate::graphics::LineCap::Butt,
                join: crate::graphics::LineJoin::Miter,
                miter_limit: 10.0,
                dash_pattern: None,
            },
        }
    }

    fn parse_inline_image(&self, lexer: &mut Lexer) -> Command {
        // Inline Image Handling (ISO 32000-2:2020 Clause 8.9.7)
        let mut dict = BTreeMap::new();
        while let Ok(token) = lexer.next_token() {
            if token == Token::Keyword("ID".to_string()) {
                break;
            }
            let key = match token {
                Token::Name(b) => crate::refine::text::recover_string(&b),
                _ => continue,
            };
            let val = match lexer.next_token() {
                Ok(Token::LeftArray) => self.parse_ir_array(lexer),
                Ok(Token::LeftDict) => self.parse_ir_dict(lexer),
                Ok(v) => token_to_ir_object(v).unwrap_or(IrObject::Null),
                Err(_) => IrObject::Null,
            };
            dict.insert(key, val);
        }

        // Now skip binary data until EI.
        // EI must be preceded by whitespace and followed by whitespace/EOF.
        let start_pos = lexer.pos();
        let data = lexer.get_data();
        let mut end_pos = start_pos;
        while end_pos + 3 <= data.len() {
            if &data[end_pos..end_pos + 3] == b" EI"
                || &data[end_pos..end_pos + 3] == b"\nEI"
                || &data[end_pos..end_pos + 3] == b"\rEI"
            {
                break;
            }
            end_pos += 1;
        }
        let img_data = data[start_pos..end_pos].to_vec();
        lexer.set_pos(end_pos + 3);

        Command::DrawInlineImage {
            width: dict.get("W").and_then(|v| v.as_i64()).unwrap_or(0) as u32,
            height: dict.get("H").and_then(|v| v.as_i64()).unwrap_or(0) as u32,
            format: crate::graphics::PixelFormat::Rgb8, // Placeholder
            data: img_data,
        }
    }

    /// Parses a content stream into drawing commands.
    /// The interpretation decisions taken while sublimating, and clears them.
    pub fn take_decisions(&mut self) -> Vec<crate::interpretation::Decision> {
        std::mem::take(&mut self.decisions)
    }

    /// Turns a content stream's bytes into drawing commands.
    pub fn sublimate(&mut self, data: &[u8]) -> Vec<Command> {
        // DETECT CORRUPTION: If the stream looks like Rust debug output, attempt resurrection (ISO 32000-2:2020 Clause 7.8.2 Fallback)
        if (data.starts_with(b"PushState") || data.starts_with(b"RawOperator"))
            && let Some(cmds) = super::resurrection::resurrect_commands(data)
        {
            return cmds;
        }

        let mut commands = Vec::new();
        let mut lexer = Lexer::new(bytes::Bytes::copy_from_slice(data));

        while let Ok(token) = lexer.next_token() {
            if token == Token::EOF {
                break;
            }

            match token {
                Token::Keyword(kw) => {
                    if kw == "BI" {
                        let cmd = self.parse_inline_image(&mut lexer);
                        commands.push(cmd);
                    } else {
                        let mut cmds = self.handle_operator(&kw, &commands);
                        commands.append(&mut cmds);
                    }
                }
                Token::LeftArray => {
                    let arr = self.parse_ir_array(&mut lexer);
                    self.stack.push(arr);
                }
                Token::LeftDict => {
                    let dict = self.parse_ir_dict(&mut lexer);
                    self.stack.push(dict);
                }
                _ => {
                    if let Some(ir) = token_to_ir_object(token) {
                        self.stack.push(ir);
                    }
                }
            }
        }

        commands
    }

    fn handle_operator(&mut self, op: &str, prev_commands: &[Command]) -> Vec<Command> {
        match op {
            "q" | "Q" | "cm" | "m" | "l" | "c" | "v" | "y" | "h" | "n" | "re" | "f" | "F"
            | "f*" | "S" | "s" | "B" | "B*" | "b" | "b*" | "W" | "W*" | "Do" | "w" | "J" | "j"
            | "M" | "d" | "i" | "gs" | "sh" | "ri" => self.handle_graphics_op(op, prev_commands),
            "BT" | "ET" | "Tf" | "Tj" | "'" | "\"" | "TJ" | "Td" | "TD" | "Tm" | "Tc" | "Tw"
            | "Tz" | "Tr" | "Ts" | "TL" | "T*" => self.handle_text_op(op),
            "rg" | "RG" | "k" | "K" | "g" | "G" | "cs" | "CS" | "scn" | "SCN" | "sc" | "SC" => {
                self.handle_color_op(op)
            }
            "BMC" | "BDC" | "EMC" | "MP" | "DP" => self.handle_marked_content_op(op),
            "BX" | "EX" => Vec::new(), // Strip compatibility operators
            "d0" | "d1" => self.handle_type3_op(op),
            _ => {
                self.decisions.push(crate::interpretation::Decision::ambiguity(
                    "7.8.2",
                    format!("unknown content operator {op}"),
                    "kept it verbatim so a later writer can reproduce it",
                ));
                let mut operands = Vec::new();
                while let Some(obj) = self.stack.pop() {
                    operands.insert(0, obj);
                }
                vec![Command::RawOperator { name: op.to_string(), operands }]
            }
        }
    }

    #[allow(clippy::collapsible_if)]
    fn handle_graphics_op(&mut self, op: &str, prev_commands: &[Command]) -> Vec<Command> {
        // RR-15 Limit: Dispatcher - Flat non-nested PDF graphics instruction parsing dispatcher routing ops to Commands
        match op {
            "q" => vec![Command::PushState],
            "Q" => vec![Command::PopState],
            "cm" => self.pop_affine().map(Command::Transform).into_iter().collect(),
            "m" => self.pop_point().map(Command::MoveTo).into_iter().collect(),
            "l" => self.pop_point().map(Command::LineTo).into_iter().collect(),
            "c" => self
                .pop_three_points()
                .map(|(p1, p2, p3)| Command::CurveTo(p1, p2, p3))
                .into_iter()
                .collect(),
            "v" | "y" | "n" => {
                let mut operands = Vec::new();
                if op == "v" || op == "y" {
                    // Both v and y require 4 operands (2 points)
                    for _ in 0..4 {
                        if let Some(ir) = self.stack.pop() {
                            operands.push(ir);
                        }
                    }
                }
                operands.reverse();
                vec![Command::RawOperator { name: op.to_string(), operands }]
            }
            "h" => vec![Command::ClosePath],
            "re" => self.pop_rect().map(Command::Rect).into_iter().collect(),
            // A heuristic stood here that replaced `f` with `n` — deleting the fill —
            // whenever the preceding rectangle had `y1 > 700`, height under 15 and
            // width over 500. Its comment named the file it was written for: "In Intel
            // SDM, pages 2-4 have `re f` at the top where other pages have `re W n`".
            //
            // It fired 1,738 times on that file and 902 times on `samples/volvo_xc90.pdf`,
            // where the rectangles it deleted were one point high and 681 wide, at
            // y-coordinates above 9,000 — rules, not header bars. The `y1 > 700` test
            // assumed a user space it had no way to check.
            //
            // Removing it changed no text on any corpus file, so it helped none and
            // deleted marks on two. It also reported only through `log::info!`, which
            // ARCHITECTURE.md §5.3 calls a silent acceptance and therefore a defect.
            // Deciding a producer's fill is a bug, from its geometry, is not something
            // this engine can know.
            "f" | "F" | "f*" | "S" | "s" | "B" | "B*" | "b" | "b*" => match op {
                "f" | "F" => vec![Command::Fill(WindingRule::NonZero)],
                "f*" => vec![Command::Fill(WindingRule::EvenOdd)],
                "S" => self.create_stroke().map(Command::Stroke).into_iter().collect(),
                "s" => {
                    let mut cmds = self.handle_operator("h", prev_commands);
                    if let Some(s) = self.create_stroke() {
                        cmds.push(Command::Stroke(s));
                    }
                    cmds
                }
                "B" => self
                    .create_stroke()
                    .map(|s| Command::FillStroke(WindingRule::NonZero, s))
                    .into_iter()
                    .collect(),
                "B*" => self
                    .create_stroke()
                    .map(|s| Command::FillStroke(WindingRule::EvenOdd, s))
                    .into_iter()
                    .collect(),
                "b" => {
                    let mut cmds = self.handle_operator("h", prev_commands);
                    if let Some(s) = self.create_stroke() {
                        cmds.push(Command::FillStroke(WindingRule::NonZero, s));
                    }
                    cmds
                }
                "b*" => {
                    let mut cmds = self.handle_operator("h", prev_commands);
                    if let Some(s) = self.create_stroke() {
                        cmds.push(Command::FillStroke(WindingRule::EvenOdd, s));
                    }
                    cmds
                }
                _ => Vec::new(),
            },
            "W" => vec![Command::Clip(WindingRule::NonZero)],
            "W*" => vec![Command::Clip(WindingRule::EvenOdd)],
            "Do" => self
                .pop_name()
                .map(|n| Command::DrawXObject(n.as_str().to_string()))
                .into_iter()
                .collect(),
            "w" => {
                if let Some(w) = self.pop_f64() {
                    self.current_stroke_style.width = w;
                    vec![Command::SetLineWidth(w)]
                } else {
                    Vec::new()
                }
            }
            "J" => {
                if let Some(i) = self.pop_i64() {
                    let cap = crate::graphics::LineCap::from_i64(i);
                    self.current_stroke_style.cap = cap;
                    vec![Command::SetLineCap(cap)]
                } else {
                    Vec::new()
                }
            }
            "j" => {
                if let Some(i) = self.pop_i64() {
                    let join = crate::graphics::LineJoin::from_i64(i);
                    self.current_stroke_style.join = join;
                    vec![Command::SetLineJoin(join)]
                } else {
                    Vec::new()
                }
            }
            "M" => {
                if let Some(m) = self.pop_f64() {
                    self.current_stroke_style.miter_limit = m;
                    vec![Command::SetMiterLimit(m)]
                } else {
                    Vec::new()
                }
            }
            "d" => {
                let dash_phase = self.pop_f64();
                let dash_array = self.pop_f64_array();
                if let (Some(arr), Some(phase)) = (dash_array, dash_phase) {
                    self.current_stroke_style.dash_pattern = Some((arr.clone(), phase));
                    vec![Command::SetDashPattern(arr, phase)]
                } else {
                    Vec::new()
                }
            }
            "i" | "gs" | "sh" | "ri" => {
                let mut operands = Vec::new();
                if let Some(op1) = self.stack.pop() {
                    operands.push(op1);
                }
                vec![Command::RawOperator { name: op.to_string(), operands }]
            }
            _ => Vec::new(),
        }
    }

    fn handle_text_op(&mut self, op: &str) -> Vec<Command> {
        match op {
            "BT" => vec![Command::BeginText],
            "ET" => vec![Command::EndText],
            "Tf" => self.handle_font_selection(),
            "Tj" => self.handle_show_text().into_iter().collect(),
            "'" => self.handle_quote_op(),
            "\"" => self.handle_double_quote_op(),
            "TJ" => self.handle_show_text_array().into_iter().collect(),
            "Td" => self.pop_point().map(Command::MoveText).into_iter().collect(),
            "TD" => self.handle_td_op(),
            "Tm" => self.pop_affine().map(Command::SetTextMatrix).into_iter().collect(),
            "Tc" => self.pop_f64().map(Command::SetCharSpacing).into_iter().collect(),
            "Tw" => self.pop_f64().map(Command::SetWordSpacing).into_iter().collect(),
            "Tz" => self.pop_f64().map(Command::SetHorizontalScaling).into_iter().collect(),
            "Tr" => self
                .pop_i64()
                .map(|i| Command::SetTextRenderMode(TextRenderingMode::from(i)))
                .into_iter()
                .collect(),
            "Ts" => self.pop_f64().map(Command::SetTextRise).into_iter().collect(),
            "TL" => self.pop_f64().map(Command::SetTextLeading).into_iter().collect(),
            "T*" => vec![Command::MoveToNextLine],
            _ => Vec::new(),
        }
    }

    fn handle_quote_op(&mut self) -> Vec<Command> {
        let mut cmds = vec![Command::MoveToNextLine];
        if let Some(text) = self.handle_show_text() {
            cmds.push(text);
        }
        cmds
    }

    fn handle_double_quote_op(&mut self) -> Vec<Command> {
        let mut cmds = Vec::new();
        let string = self.stack.pop();
        let char_spacing = self.pop_f64();
        let word_spacing = self.pop_f64();

        if let Some(w) = word_spacing {
            cmds.push(Command::SetWordSpacing(w));
        }
        if let Some(c) = char_spacing {
            cmds.push(Command::SetCharSpacing(c));
        }
        cmds.push(Command::MoveToNextLine);
        if let Some(token) = string {
            self.stack.push(token);
            if let Some(text) = self.handle_show_text() {
                cmds.push(text);
            }
        }
        cmds
    }

    fn handle_td_op(&mut self) -> Vec<Command> {
        if let Some(p) = self.pop_point() {
            vec![Command::SetTextLeading(-p.y), Command::MoveText(p)]
        } else {
            Vec::new()
        }
    }

    #[allow(clippy::collapsible_if)]
    fn handle_color_op(&mut self, op: &str) -> Vec<Command> {
        match op {
            "rg" => self.pop_rgb().map(Command::SetFillColor).into_iter().collect(),
            "RG" => self.pop_rgb().map(Command::SetStrokeColor).into_iter().collect(),
            "k" => self.pop_cmyk().map(Command::SetFillColor).into_iter().collect(),
            "K" => self.pop_cmyk().map(Command::SetStrokeColor).into_iter().collect(),
            "g" => {
                self.pop_f64().map(|g| Command::SetFillColor(Color::Gray(g))).into_iter().collect()
            }
            "G" => self
                .pop_f64()
                .map(|g| Command::SetStrokeColor(Color::Gray(g)))
                .into_iter()
                .collect(),
            "cs" | "CS" => self.sublimate_cs(op),
            "scn" | "SCN" | "sc" | "SC" => self.sublimate_sc(op),
            _ => {
                let mut operands = Vec::new();
                // Pop name if present (for Pattern or Separation)
                if let Some(IrObject::Name(_)) = self.stack.last() {
                    if let Some(obj) = self.stack.pop() {
                        operands.push(obj);
                    }
                }
                // Pop all preceding numbers
                while let Some(obj) = self.stack.last() {
                    if matches!(obj, IrObject::Integer(_) | IrObject::Real(_)) {
                        if let Some(obj) = self.stack.pop() {
                            operands.push(obj);
                        }
                    } else {
                        break;
                    }
                }
                operands.reverse();
                vec![Command::RawOperator { name: op.to_string(), operands }]
            }
        }
    }

    fn handle_marked_content_op(&mut self, op: &str) -> Vec<Command> {
        match op {
            "BMC" => self
                .pop_name()
                .map(|n| Command::BeginMarkedContent { tag: n, properties: None })
                .into_iter()
                .collect(),
            "BDC" => self.handle_bdc(),
            "EMC" => vec![Command::EndMarkedContent],
            "MP" | "DP" => {
                let mut operands = Vec::new();
                if let Some(op1) = self.stack.pop() {
                    operands.push(op1);
                }
                vec![Command::RawOperator { name: op.to_string(), operands }]
            }
            _ => Vec::new(),
        }
    }

    fn handle_bdc(&mut self) -> Vec<Command> {
        let props = self.stack.pop();
        let tag = self.pop_name();
        if let Some(t) = tag {
            vec![Command::BeginMarkedContent { tag: t, properties: props }]
        } else {
            Vec::new()
        }
    }

    fn handle_type3_op(&mut self, op: &str) -> Vec<Command> {
        match op {
            "d0" => {
                let Some(wy) = self.pop_f64() else { return Vec::new() };
                let Some(wx) = self.pop_f64() else { return Vec::new() };
                vec![Command::Type3SetMetrics { wx, wy, bbox: None }]
            }
            "d1" => {
                let Some(ury) = self.pop_f64() else { return Vec::new() };
                let Some(urx) = self.pop_f64() else { return Vec::new() };
                let Some(lly) = self.pop_f64() else { return Vec::new() };
                let Some(llx) = self.pop_f64() else { return Vec::new() };
                let Some(wy) = self.pop_f64() else { return Vec::new() };
                let Some(wx) = self.pop_f64() else { return Vec::new() };
                vec![Command::Type3SetMetrics {
                    wx,
                    wy,
                    bbox: Some(kurbo::Rect::new(llx, lly, urx, ury)),
                }]
            }
            _ => Vec::new(),
        }
    }

    // --- Helpers for popping operands ---

    fn sublimate_cs(&mut self, op: &str) -> Vec<Command> {
        use crate::graphics::ColorSpaceKind;
        let is_fill = op == "cs";
        let name = self
            .pop_name()
            .map(|n| n.as_str().to_string())
            .unwrap_or_else(|| "DeviceGray".to_string());
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
            self.fill_color_space = cs;
            vec![Command::SetFillColorSpace(name)]
        } else {
            self.stroke_color_space = cs;
            vec![Command::SetStrokeColorSpace(name)]
        }
    }

    fn sublimate_sc(&mut self, op: &str) -> Vec<Command> {
        let is_fill = op == "sc" || op == "scn";
        let current_cs = if is_fill { self.fill_color_space } else { self.stroke_color_space };

        let count = self
            .stack
            .iter()
            .rev()
            .take_while(|o| matches!(o, IrObject::Integer(_) | IrObject::Real(_)))
            .count();

        let color = self.pop_color_helper(current_cs, count);

        if let Some(c) = color {
            if is_fill { vec![Command::SetFillColor(c)] } else { vec![Command::SetStrokeColor(c)] }
        } else {
            let mut operands = Vec::new();
            while let Some(o) = self.stack.pop() {
                operands.insert(0, o);
            }
            vec![Command::RawOperator { name: op.to_string(), operands }]
        }
    }

    fn pop_color_helper(
        &mut self,
        cs: crate::graphics::ColorSpaceKind,
        count: usize,
    ) -> Option<Color> {
        use crate::graphics::ColorSpaceKind;
        match cs {
            ColorSpaceKind::DeviceGray => {
                if count >= 1 {
                    self.pop_f64().map(Color::Gray)
                } else {
                    None
                }
            }
            ColorSpaceKind::DeviceRGB => {
                if count >= 3 {
                    self.pop_rgb()
                } else {
                    None
                }
            }
            ColorSpaceKind::DeviceCMYK => {
                if count >= 4 {
                    self.pop_cmyk()
                } else {
                    None
                }
            }
            // Non-device spaces carry no directly poppable component tuple.
            ColorSpaceKind::CalGray
            | ColorSpaceKind::CalRGB
            | ColorSpaceKind::Lab
            | ColorSpaceKind::ICCBased
            | ColorSpaceKind::Pattern
            | ColorSpaceKind::Indexed
            | ColorSpaceKind::Separation
            | ColorSpaceKind::DeviceN
            | ColorSpaceKind::Unknown => None,
        }
    }

    fn pop_f64(&mut self) -> Option<f64> {
        match self.stack.pop() {
            Some(IrObject::Real(f)) => Some(f),
            Some(IrObject::Integer(i)) => Some(i as f64),
            _ => None,
        }
    }

    fn pop_i64(&mut self) -> Option<i64> {
        match self.stack.pop() {
            Some(IrObject::Integer(i)) => Some(i),
            Some(IrObject::Real(f)) => Some(f as i64),
            _ => None,
        }
    }

    fn pop_point(&mut self) -> Option<Point> {
        let y = self.pop_f64()?;
        let x = self.pop_f64()?;
        Some(Point::new(x, y))
    }

    fn pop_three_points(&mut self) -> Option<(Point, Point, Point)> {
        let p3 = self.pop_point()?;
        let p2 = self.pop_point()?;
        let p1 = self.pop_point()?;
        Some((p1, p2, p3))
    }

    fn pop_rect(&mut self) -> Option<Rect> {
        let h = self.pop_f64()?;
        let w = self.pop_f64()?;
        let y = self.pop_f64()?;
        let x = self.pop_f64()?;
        Some(Rect::from_origin_size(Point::new(x, y), kurbo::Size::new(w, h)))
    }

    fn pop_affine(&mut self) -> Option<Affine> {
        let f = self.pop_f64()?;
        let e = self.pop_f64()?;
        let d = self.pop_f64()?;
        let c = self.pop_f64()?;
        let b = self.pop_f64()?;
        let a = self.pop_f64()?;
        Some(Affine::new([a, b, c, d, e, f]))
    }

    fn pop_rgb(&mut self) -> Option<Color> {
        let b = self.pop_f64()?;
        let g = self.pop_f64()?;
        let r = self.pop_f64()?;
        Some(Color::Rgb(r, g, b))
    }

    fn pop_cmyk(&mut self) -> Option<Color> {
        let k = self.pop_f64()?;
        let y = self.pop_f64()?;
        let m = self.pop_f64()?;
        let c = self.pop_f64()?;
        Some(Color::Cmyk(c, m, y, k))
    }

    fn pop_name(&mut self) -> Option<PdfName> {
        match self.stack.pop() {
            Some(IrObject::Name(s)) => Some(PdfName::new(&s)),
            _ => None,
        }
    }

    fn pop_f64_array(&mut self) -> Option<Vec<f64>> {
        match self.stack.pop()? {
            IrObject::Array(arr) => {
                let mut vals = Vec::new();
                for item in arr {
                    if let Some(f) = item.as_f64() {
                        vals.push(f);
                    }
                }
                Some(vals)
            }
            _ => None,
        }
    }

    fn create_stroke(&self) -> Option<StrokeStyle> {
        Some(self.current_stroke_style.clone())
    }

    fn handle_font_selection(&mut self) -> Vec<Command> {
        let Some(size) = self.pop_f64() else { return Vec::new() };
        let Some(name) = self.pop_name() else { return Vec::new() };
        let name_str = name.as_str();

        if let Some(font_res) = self.fonts.get(name_str) {
            self.current_font = Some(font_res.clone());
            vec![
                Command::SetFont { font: name_str.to_string(), size },
                Command::SetWritingMode(font_res.wmode),
            ]
        } else {
            self.decisions.push(crate::interpretation::Decision::repaired(
                "9.6.2",
                format!(
                    "the content stream selects /{name_str}, which its resources do not define"
                ),
                "substituted a fallback font so the text still renders",
            ));
            // Insert a SetFont command anyway, but mark it for fallback resolution in SDK
            vec![Command::SetFont { font: "Fallback-Sans".to_string(), size }]
        }
    }

    fn handle_show_text(&mut self) -> Option<Command> {
        let obj = self.stack.pop()?;
        match obj {
            IrObject::String(b) | IrObject::Hex(b) => Some(Command::ShowText(b)),
            _ => None,
        }
    }

    fn handle_show_text_array(&mut self) -> Option<Command> {
        let obj = self.stack.pop()?;
        let items = match obj {
            IrObject::Array(arr) => arr,
            _ => return None,
        };

        let mut array_items = Vec::new();
        for t in items {
            match t {
                IrObject::String(b) | IrObject::Hex(b) => {
                    array_items.push(super::TextArrayItem::Text(b));
                }
                IrObject::Integer(i) => {
                    array_items.push(super::TextArrayItem::Offset(i as f64));
                }
                IrObject::Real(f) => {
                    array_items.push(super::TextArrayItem::Offset(f));
                }
                _ => {}
            }
        }
        Some(Command::ShowTextArray(array_items))
    }

    fn parse_ir_array(&self, lexer: &mut Lexer) -> IrObject {
        let mut elements = Vec::new();
        while let Ok(token) = lexer.peek() {
            if token == Token::RightArray || token == Token::EOF {
                break;
            }
            let _ = lexer.next_token();
            match token {
                Token::LeftArray => elements.push(self.parse_ir_array(lexer)),
                Token::LeftDict => elements.push(self.parse_ir_dict(lexer)),
                _ => {
                    if let Some(ir) = token_to_ir_object(token) {
                        elements.push(ir);
                    }
                }
            }
        }
        let _ = lexer.next_token(); // consume ']'
        IrObject::Array(elements)
    }

    fn parse_ir_dict(&self, lexer: &mut Lexer) -> IrObject {
        let mut dict = BTreeMap::new();
        while let Ok(token) = lexer.peek() {
            if token == Token::RightDict || token == Token::EOF {
                break;
            }
            let Ok(key_token) = lexer.next_token() else {
                break;
            };
            let key = match key_token {
                Token::Name(b) => crate::refine::text::recover_string(&b),
                _ => continue, // Should be an error but let's be robust
            };

            let Ok(val_token) = lexer.next_token() else {
                break;
            };
            let val = match val_token {
                Token::LeftArray => self.parse_ir_array(lexer),
                Token::LeftDict => self.parse_ir_dict(lexer),
                _ => token_to_ir_object(val_token).unwrap_or(IrObject::Null),
            };
            dict.insert(key, val);
        }
        let _ = lexer.next_token(); // consume '>>'
        IrObject::Dictionary(dict)
    }
}

fn token_to_ir_object(token: Token) -> Option<IrObject> {
    match token {
        Token::Boolean(b) => Some(IrObject::Boolean(b)),
        Token::Integer(i) => Some(IrObject::Integer(i)),
        Token::Real(f) => Some(IrObject::Real(f)),
        Token::String(s) => Some(IrObject::String(s)),
        Token::Hex(s) => Some(IrObject::Hex(s)),
        Token::Name(n) => Some(IrObject::Name(crate::refine::text::recover_string(&n))),
        Token::Null => Some(IrObject::Null),
        _ => None,
    }
}
