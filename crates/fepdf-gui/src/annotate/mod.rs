//! Putting an annotation on the page with the pointer (ROADMAP W-13).
//!
//! **The engine made all twelve kinds and nothing in this window asked it to.** A reader
//! picks a pen in the drawer and draws with it on the page; what comes out is one
//! `AddAnnotation`, which is all a frontend may send (Rule D).
//!
//! **The gesture is the one each kind is drawn with elsewhere**: a drag covers what a
//! highlight marks or a box holds, a drag from a point to where the words go is a callout,
//! the path of a drag is an ink stroke, and a click is enough for a note or typed words.

mod more;

use crate::interaction::SelectionManager;
use fepdf::{AnnotationKind, AnnotationSpec, ShapeForm};
pub use more::More;

/// What a reader draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pen {
    /// A note: an icon, and the words it opens to (12.5.6.4).
    Note,
    /// Words on the page with no box round them.
    Typewriter,
    /// Words in a box.
    TextBox,
    /// Words in a box with a line to what they are about.
    Callout,
    /// A highlight over text.
    Highlight,
    /// A line under text.
    Underline,
    /// A line through text.
    StrikeOut,
    /// A wavy line under text.
    Squiggly,
    /// A freehand stroke.
    Ink,
    /// A rectangle's outline.
    Rectangle,
    /// An ellipse's outline.
    Ellipse,
    /// A straight line.
    Line,
    /// A picture.
    Stamp,
    /// A region that goes somewhere when clicked.
    Link,
    /// A closed shape through points clicked in turn (12.5.6.9).
    Polygon,
    /// An open line through points clicked in turn (12.5.6.9).
    PolyLine,
    /// A mark where words are to go in (12.5.6.11).
    Caret,
    /// A file carried on the page (12.5.6.15).
    Attachment,
    /// A region media plays in (12.5.6.18).
    Screen,
    /// The window a comment's words open in (12.5.6.14).
    Popup,
    /// A registration target or a colour bar (14.11.3).
    PrinterMark,
    /// Words across the page that print as they are shown (12.5.6.22).
    Watermark,
    /// A region marked to be redacted, and not yet removed (12.5.6.23).
    Redact,
    /// A comment on a measurement (12.5.6.24).
    Projection,
}

impl Pen {
    /// Every pen, in the order the drawer lists them.
    pub const ALL: [Self; 24] = [
        Self::Note,
        Self::Typewriter,
        Self::TextBox,
        Self::Callout,
        Self::Highlight,
        Self::Underline,
        Self::StrikeOut,
        Self::Squiggly,
        Self::Ink,
        Self::Rectangle,
        Self::Ellipse,
        Self::Line,
        Self::Stamp,
        Self::Link,
        Self::Polygon,
        Self::PolyLine,
        Self::Caret,
        Self::Attachment,
        Self::Screen,
        Self::Popup,
        Self::PrinterMark,
        Self::Watermark,
        Self::Redact,
        Self::Projection,
    ];

    /// The locale keys naming it and saying how it is drawn.
    ///
    /// No wildcard arm, so another pen does not compile until it has both (Rule 5).
    pub const fn keys(self) -> (&'static str, &'static str) {
        match self {
            Self::Note => ("annotate_note", "annotate_how_click"),
            Self::Typewriter => ("annotate_typewriter", "annotate_how_click"),
            Self::TextBox => ("annotate_text_box", "annotate_how_box"),
            Self::Callout => ("annotate_callout", "annotate_how_callout"),
            Self::Highlight => ("annotate_highlight", "annotate_how_mark"),
            Self::Underline => ("annotate_underline", "annotate_how_mark"),
            Self::StrikeOut => ("annotate_strike_out", "annotate_how_mark"),
            Self::Squiggly => ("annotate_squiggly", "annotate_how_mark"),
            Self::Ink => ("annotate_ink", "annotate_how_ink"),
            Self::Rectangle => ("annotate_rectangle", "annotate_how_box"),
            Self::Ellipse => ("annotate_ellipse", "annotate_how_box"),
            Self::Line => ("annotate_line", "annotate_how_line"),
            Self::Stamp => ("annotate_stamp", "annotate_how_box"),
            Self::Link => ("annotate_link", "annotate_how_box"),
            Self::Polygon => ("annotate_polygon", "annotate_how_points"),
            Self::PolyLine => ("annotate_polyline", "annotate_how_points"),
            Self::Caret => ("annotate_caret", "annotate_how_click"),
            Self::Attachment => ("annotate_attachment", "annotate_how_click"),
            Self::Screen => ("annotate_screen", "annotate_how_box"),
            Self::Popup => ("annotate_popup", "annotate_how_popup"),
            Self::PrinterMark => ("annotate_printer_mark", "annotate_how_box"),
            Self::Watermark => ("annotate_watermark", "annotate_how_box"),
            Self::Redact => ("annotate_redact", "annotate_how_box"),
            Self::Projection => ("annotate_projection", "annotate_how_box"),
        }
    }

    /// Whether it carries words the reader types: needed by a note, typed words, a box,
    /// a callout, a watermark and a projection, and taken where given by a caret, an
    /// attachment (what it is), a screen (its title) and a redaction (what is put over it).
    pub const fn takes_words(self) -> bool {
        matches!(
            self,
            Self::Note
                | Self::Typewriter
                | Self::TextBox
                | Self::Callout
                | Self::Caret
                | Self::Attachment
                | Self::Screen
                | Self::Watermark
                | Self::Redact
                | Self::Projection
        )
    }

    /// Whether the words are set on the page at a size, rather than kept behind an icon.
    pub const fn takes_size(self) -> bool {
        matches!(self, Self::Typewriter | Self::TextBox | Self::Callout)
    }

    /// Whether it is drawn through points clicked one at a time.
    pub const fn takes_points(self) -> bool {
        matches!(self, Self::Polygon | Self::PolyLine)
    }

    /// Whether it is drawn in a colour the reader chooses.
    pub const fn takes_colour(self) -> bool {
        matches!(
            self,
            Self::Highlight
                | Self::Underline
                | Self::StrikeOut
                | Self::Squiggly
                | Self::Ink
                | Self::Rectangle
                | Self::Ellipse
                | Self::Line
                | Self::Polygon
                | Self::PolyLine
                | Self::Caret
                | Self::Redact
        )
    }

    /// Whether it is a line of a width the reader chooses.
    pub const fn takes_width(self) -> bool {
        matches!(
            self,
            Self::Ink
                | Self::Rectangle
                | Self::Ellipse
                | Self::Line
                | Self::Polygon
                | Self::PolyLine
        )
    }
}

/// How a pen's gesture is read, and which of its group it is.
///
/// One exhaustive match decides the group, so each placing function matches its own pens
/// and nothing else (Rule 5) — rather than each listing every other pen as refused.
enum Gesture {
    Written(Written),
    Drawn(Drawn),
    /// A stamp or a link, in a box.
    Pointing,
    More(more::Kind),
}

/// Pens that carry words a reader typed.
#[derive(Clone, Copy)]
enum Written {
    Note,
    Typewriter,
    TextBox,
    Callout,
}

/// Pens that draw in a colour: marks, strokes and shapes.
#[derive(Clone, Copy)]
enum Drawn {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
    Ink,
    Rectangle,
    Ellipse,
    Line,
}

impl Pen {
    const fn gesture(self) -> Gesture {
        use more::Kind;
        match self {
            Self::Note => Gesture::Written(Written::Note),
            Self::Typewriter => Gesture::Written(Written::Typewriter),
            Self::TextBox => Gesture::Written(Written::TextBox),
            Self::Callout => Gesture::Written(Written::Callout),
            Self::Highlight => Gesture::Drawn(Drawn::Highlight),
            Self::Underline => Gesture::Drawn(Drawn::Underline),
            Self::StrikeOut => Gesture::Drawn(Drawn::StrikeOut),
            Self::Squiggly => Gesture::Drawn(Drawn::Squiggly),
            Self::Ink => Gesture::Drawn(Drawn::Ink),
            Self::Rectangle => Gesture::Drawn(Drawn::Rectangle),
            Self::Ellipse => Gesture::Drawn(Drawn::Ellipse),
            Self::Line => Gesture::Drawn(Drawn::Line),
            Self::Stamp | Self::Link => Gesture::Pointing,
            Self::Polygon | Self::PolyLine => Gesture::More(Kind::Points),
            Self::Caret => Gesture::More(Kind::Caret),
            Self::Attachment => Gesture::More(Kind::Attachment),
            Self::Screen => Gesture::More(Kind::Screen),
            Self::Popup => Gesture::More(Kind::Popup),
            Self::PrinterMark => Gesture::More(Kind::PrinterMark),
            Self::Watermark => Gesture::More(Kind::Watermark),
            Self::Redact => Gesture::More(Kind::Redact),
            Self::Projection => Gesture::More(Kind::Projection),
        }
    }
}

/// The colours a pen can draw in, by the locale key naming each.
pub const COLOURS: [(&str, [f32; 3]); 5] = [
    ("annotate_yellow", [1.0, 0.85, 0.0]),
    ("annotate_red", [0.85, 0.1, 0.1]),
    ("annotate_blue", [0.1, 0.3, 0.85]),
    ("annotate_green", [0.1, 0.55, 0.2]),
    ("annotate_black", [0.0, 0.0, 0.0]),
];

/// The drawer: which pen, and what it draws with.
///
/// **The drawer holds no gesture**, as the snapshot's does not: what is drawn is drawn on
/// the page, where the reader can see what it covers.
pub fn show(
    tool: &mut AnnotateTool,
    ui: &mut egui::Ui,
    tr: &dyn Fn(&str) -> String,
    comments: &[fepdf::comments::Comment],
) {
    use crate::app::theme::space;
    ui.horizontal_wrapped(|ui| {
        for pen in Pen::ALL {
            ui.selectable_value(&mut tool.pen, pen, tr(pen.keys().0));
        }
    });
    ui.add_space(space::ITEM);
    ui.label(tr(tool.pen.keys().1));
    ui.add_space(space::ITEM);
    if tool.pen.takes_words() {
        ui.label(tr("annotate_words"));
        ui.add(egui::TextEdit::multiline(&mut tool.words).desired_rows(3));
    }
    if tool.pen.takes_size() {
        ui.horizontal(|ui| {
            ui.label(tr("annotate_size"));
            ui.add(egui::DragValue::new(&mut tool.font_size).range(4.0..=72.0).suffix(" pt"));
        });
    }
    if tool.pen.takes_colour() {
        ui.horizontal_wrapped(|ui| {
            for (index, (key, _)) in COLOURS.iter().enumerate() {
                ui.selectable_value(&mut tool.colour, index, tr(key));
            }
        });
    }
    if tool.pen.takes_width() {
        ui.horizontal(|ui| {
            ui.label(tr("annotate_width"));
            ui.add(egui::DragValue::new(&mut tool.width).range(0.25..=24.0).suffix(" pt"));
        });
    }
    if tool.pen == Pen::Link {
        ui.label(tr("annotate_target"));
        ui.text_edit_singleline(&mut tool.target);
    }
    if tool.pen == Pen::Stamp {
        choose_picture(tool, ui, tr);
    }
    more::show(tool, ui, tr, comments);
}

/// The stamp's picture: a JPEG, which the engine carries into the file as it is.
fn choose_picture(tool: &mut AnnotateTool, ui: &mut egui::Ui, tr: &dyn Fn(&str) -> String) {
    if let Some((name, _)) = &tool.picture {
        ui.label(name.as_str());
    }
    if ui.button(tr("annotate_choose_picture")).clicked()
        && let Some(path) = rfd::FileDialog::new().add_filter("JPEG", &["jpg", "jpeg"]).pick_file()
    {
        tool.picture = std::fs::read(&path).ok().map(|bytes| {
            (path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()), bytes)
        });
    }
}

/// How big a note's icon is, in points.
const NOTE: f32 = 20.0;

/// What the annotation tool holds between frames.
pub struct AnnotateTool {
    /// Whether it is on.
    pub is_active: bool,
    /// What it draws.
    pub pen: Pen,
    /// The words a note, a box, typed words or a callout carries.
    pub words: String,
    /// The size they are set at, in points.
    pub font_size: f32,
    /// Which of [`COLOURS`].
    pub colour: usize,
    /// The width of a stroke or an outline, in points.
    pub width: f32,
    /// Where a link goes: a page number, or anything else as an address.
    pub target: String,
    /// A stamp's picture: its file name, and the JPEG.
    pub picture: Option<(String, Vec<u8>)>,
    /// What the pens of AA-4g hold: points clicked, a file, a comment, a mark.
    pub more: More,
    /// The drag under way, in the page's own space.
    drag: Vec<egui::Pos2>,
}

impl Default for AnnotateTool {
    fn default() -> Self {
        Self {
            is_active: false,
            pen: Pen::Highlight,
            words: String::new(),
            font_size: 12.0,
            colour: 0,
            width: 2.0,
            target: String::new(),
            picture: None,
            more: More::default(),
            drag: Vec::new(),
        }
    }
}

/// Why a gesture made no annotation, as the locale key that says so.
pub type Refusal = &'static str;

impl AnnotateTool {
    /// Follows the gesture, and answers what it drew when it ends.
    pub fn interaction(
        &mut self,
        ui: &mut egui::Ui,
        page: usize,
        page_rect: egui::Rect,
        frame: crate::interaction::PageFrame,
        zoom: f32,
    ) -> Option<Result<AnnotationSpec, Refusal>> {
        if !self.is_active {
            return None;
        }
        let response = ui.allocate_rect(page_rect, egui::Sense::click_and_drag());
        let at = |pos| SelectionManager::screen_to_pdf(page_rect, zoom, frame, pos);
        let (pointer, origin) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.press_origin()));
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if self.pen.takes_points() {
            let finished = self.follow_points(ui, &response, page, &at, pointer);
            self.paint_points(ui, page, page_rect, frame, (zoom, pointer));
            return finished.map(|points| self.placed(page, &points));
        }
        if response.drag_started() {
            self.drag = origin.or(pointer).map(at).into_iter().collect();
        }
        if response.dragged()
            && let Some(pos) = pointer.map(at)
            && self.drag.last() != Some(&pos)
        {
            self.drag.push(pos);
        }
        self.paint_drag(ui, page_rect, frame, zoom);
        if response.clicked() {
            return pointer.map(|pos| self.placed(page, &[at(pos)]));
        }
        if !response.drag_stopped() {
            return None;
        }
        let points = std::mem::take(&mut self.drag);
        Some(self.placed(page, &points))
    }

    /// Draws the drag under way over the page: the path for a stroke or a line, the
    /// rectangle for everything else.
    fn paint_drag(
        &self,
        ui: &egui::Ui,
        page_rect: egui::Rect,
        frame: crate::interaction::PageFrame,
        zoom: f32,
    ) {
        let (Some(&start), Some(&end)) = (self.drag.first(), self.drag.last()) else { return };
        let to = |pos| SelectionManager::pdf_to_screen(page_rect, zoom, frame, pos);
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("annotate_drag"));
        let painter = ui.ctx().layer_painter(layer).with_clip_rect(page_rect);
        let stroke = egui::Stroke::new(1.5_f32, crate::app::theme::colors::rust::ACCENT);
        match self.pen {
            Pen::Ink => {
                painter.add(egui::Shape::line(self.drag.iter().map(|p| to(*p)).collect(), stroke));
            }
            Pen::Line | Pen::Callout => {
                painter.line_segment([to(start), to(end)], stroke);
            }
            Pen::Note
            | Pen::Typewriter
            | Pen::TextBox
            | Pen::Highlight
            | Pen::Underline
            | Pen::StrikeOut
            | Pen::Squiggly
            | Pen::Rectangle
            | Pen::Ellipse
            | Pen::Stamp
            | Pen::Link
            | Pen::Polygon
            | Pen::PolyLine
            | Pen::Caret
            | Pen::Attachment
            | Pen::Screen
            | Pen::Popup
            | Pen::PrinterMark
            | Pen::Watermark
            | Pen::Redact
            | Pen::Projection => {
                let rect = egui::Rect::from_two_pos(to(start), to(end));
                painter.rect_stroke(
                    rect,
                    crate::app::theme::radius::FLAT,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
    }

    /// The annotation a gesture through `points` draws on `page`, or why it draws none.
    ///
    /// **A drag that went nowhere is not a box**: a click with a box pen would otherwise
    /// ask for an annotation of no area, which the engine refuses in words a reader did
    /// not choose. It is refused here, saying what the pen wants instead.
    ///
    /// # Errors
    /// The locale key of what is missing: words, a picture, a target, or a drag.
    pub fn placed(&self, page: usize, points: &[egui::Pos2]) -> Result<AnnotationSpec, Refusal> {
        let (Some(&start), Some(&end)) = (points.first(), points.last()) else {
            return Err("annotate_needs_drag");
        };
        let dragged = egui::Rect::from_two_pos(start, end);
        let has_area = dragged.width() >= 1.0 && dragged.height() >= 1.0;
        let (rect, kind) = match self.pen.gesture() {
            Gesture::Written(pen) => self.written(pen, start, end)?,
            Gesture::Drawn(pen) => self.drawn(pen, dragged, has_area, points)?,
            Gesture::Pointing if has_area => (dragged, self.pointing()?),
            Gesture::Pointing => return Err("annotate_needs_drag"),
            Gesture::More(kind) => {
                self.more_placed(kind, page, (start, dragged, has_area), points)?
            }
        };
        Ok(AnnotationSpec {
            page,
            rect: [rect.min.x, rect.min.y, rect.max.x, rect.max.y],
            kind,
            by: fepdf::Authorship::default(),
        })
    }

    /// A note, typed words, a text box or a callout.
    fn written(
        &self,
        pen: Written,
        start: egui::Pos2,
        end: egui::Pos2,
    ) -> Result<(egui::Rect, AnnotationKind), Refusal> {
        let contents = self.words.trim_end().to_owned();
        if contents.trim().is_empty() {
            return Err("annotate_needs_words");
        }
        let font_size = self.font_size;
        let fits = words_size(&contents, font_size);
        Ok(match pen {
            Written::Note => (
                egui::Rect::from_min_max(
                    egui::pos2(start.x, start.y - NOTE),
                    egui::pos2(start.x + NOTE, start.y),
                ),
                AnnotationKind::TextComment { contents },
            ),
            // Typed words start where the pointer was and take the room they need.
            Written::Typewriter => (
                egui::Rect::from_min_max(
                    egui::pos2(start.x, start.y - fits.y),
                    egui::pos2(start.x + fits.x, start.y),
                ),
                AnnotationKind::Typewriter { contents, font_size },
            ),
            Written::TextBox => {
                let rect = egui::Rect::from_two_pos(start, end);
                if rect.width() < 1.0 || rect.height() < 1.0 {
                    return Err("annotate_needs_drag");
                }
                (rect, AnnotationKind::TextBox { contents, font_size })
            }
            Written::Callout => (
                callout_box(start, end, fits)?,
                AnnotationKind::Callout { contents, font_size, points_at: [start.x, start.y] },
            ),
        })
    }

    /// A text markup, a stroke or a shape, in the chosen colour.
    fn drawn(
        &self,
        pen: Drawn,
        dragged: egui::Rect,
        has_area: bool,
        points: &[egui::Pos2],
    ) -> Result<(egui::Rect, AnnotationKind), Refusal> {
        let color_rgb = self.colour_rgb();
        let width = self.width;
        let boxed = |kind| if has_area { Ok((dragged, kind)) } else { Err("annotate_needs_drag") };
        match pen {
            Drawn::Highlight => boxed(AnnotationKind::Highlight { color_rgb }),
            Drawn::Underline => boxed(AnnotationKind::Underline { color_rgb }),
            Drawn::StrikeOut => boxed(AnnotationKind::StrikeOut { color_rgb }),
            Drawn::Squiggly => boxed(AnnotationKind::Squiggly { color_rgb }),
            Drawn::Rectangle => {
                boxed(AnnotationKind::Shape { form: ShapeForm::Rectangle, color_rgb, width })
            }
            Drawn::Ellipse => {
                boxed(AnnotationKind::Shape { form: ShapeForm::Ellipse, color_rgb, width })
            }
            // A stroke or a line is drawn along its points, and the engine grows the
            // rectangle to take them in.
            Drawn::Ink if points.len() >= 2 => {
                let stroke = points.iter().map(|p| [p.x, p.y]).collect();
                Ok((dragged, AnnotationKind::Ink { strokes: vec![stroke], color_rgb, width }))
            }
            Drawn::Line if dragged.size().length() >= 1.0 => {
                let (Some(from), Some(to)) = (points.first(), points.last()) else {
                    return Err("annotate_needs_drag");
                };
                let form = ShapeForm::Line { from: [from.x, from.y], to: [to.x, to.y] };
                Ok((dragged, AnnotationKind::Shape { form, color_rgb, width }))
            }
            Drawn::Ink | Drawn::Line => Err("annotate_needs_drag"),
        }
    }

    /// A stamp or a link: what the picture is, or where the link goes.
    fn pointing(&self) -> Result<AnnotationKind, Refusal> {
        if self.pen == Pen::Stamp {
            let Some((_, jpeg)) = &self.picture else { return Err("annotate_needs_picture") };
            return Ok(AnnotationKind::Stamp { stamp_image_bytes: jpeg.clone() });
        }
        let target = self.target.trim();
        if target.is_empty() {
            return Err("annotate_needs_target");
        }
        // A number is a page of this document, counted from one as a reader counts;
        // anything else is an address.
        Ok(match target.parse::<usize>() {
            Ok(page) if page >= 1 => AnnotationKind::Link { destination_page: page - 1, url: None },
            _ => AnnotationKind::Link { destination_page: 0, url: Some(target.to_owned()) },
        })
    }
}

/// Where a callout's box goes: from the drag's end, growing away from where it began —
/// the point the line is drawn to — so the line never crosses the words.
fn callout_box(
    start: egui::Pos2,
    end: egui::Pos2,
    fits: egui::Vec2,
) -> Result<egui::Rect, Refusal> {
    if start.distance(end) < 1.0 {
        return Err("annotate_needs_drag");
    }
    let far = egui::pos2(
        if end.x >= start.x { end.x + fits.x } else { end.x - fits.x },
        if end.y >= start.y { end.y + fits.y } else { end.y - fits.y },
    );
    Ok(egui::Rect::from_two_pos(end, far))
}

/// The room `words` take at `size`, as `set_lines` in the engine sets them: two points
/// of padding, a line every 1.2 sizes, and each character taken at a full size — which is
/// right for kanji and generous for Latin letters, and a box a little wide is a box whose
/// words stay inside it.
fn words_size(words: &str, size: f32) -> egui::Vec2 {
    let lines: Vec<&str> = words.lines().collect();
    #[allow(clippy::cast_precision_loss)] // a line of text is nowhere near 2^24 characters
    let (longest, count) = (
        lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32,
        lines.len().max(1) as f32,
    );
    egui::vec2(longest.mul_add(size, 4.0), (count * 1.2).mul_add(size, 4.0))
}

/// What each pen makes of a gesture.
///
/// The coordinates compared exactly are copied from the gesture or offset by whole
/// points, which `f32` holds exactly — nothing here is the result of a rounding.
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod placed {
    use super::{AnnotateTool, Pen};
    use fepdf::{AnnotationKind, ShapeForm};

    fn tool(pen: Pen) -> AnnotateTool {
        AnnotateTool {
            is_active: true,
            pen,
            words: "注記".to_owned(),
            target: "3".to_owned(),
            ..AnnotateTool::default()
        }
    }

    fn drag(from: (f32, f32), to: (f32, f32)) -> Vec<egui::Pos2> {
        vec![egui::pos2(from.0, from.1), egui::pos2(to.0, to.1)]
    }

    /// **A highlight covers what was dragged over**, whichever way it was dragged.
    #[test]
    fn a_highlight_covers_the_drag() {
        let placed = tool(Pen::Highlight).placed(2, &drag((300.0, 500.0), (100.0, 480.0)));
        let spec = placed.expect("it places");
        assert_eq!(spec.page, 2);
        assert_eq!(spec.rect, [100.0, 480.0, 300.0, 500.0]);
        assert!(matches!(spec.kind, AnnotationKind::Highlight { .. }));
    }

    /// A click with a pen that needs a box is refused, saying so.
    #[test]
    fn a_click_is_not_a_box() {
        for pen in [Pen::Highlight, Pen::TextBox, Pen::Rectangle, Pen::Stamp, Pen::Callout] {
            let placed = tool(pen).placed(0, &[egui::pos2(100.0, 100.0)]);
            assert_eq!(placed.err(), Some("annotate_needs_drag"), "{pen:?} took a click");
        }
    }

    /// **A click is enough for a note and for typed words**, which go where it was.
    #[test]
    fn a_click_places_a_note_and_typed_words() {
        let at = [egui::pos2(100.0, 700.0)];
        let note = tool(Pen::Note).placed(0, &at).expect("a note");
        assert_eq!(note.rect, [100.0, 680.0, 120.0, 700.0], "the icon is not under the click");
        let typed = tool(Pen::Typewriter).placed(0, &at).expect("typed words");
        assert_eq!((typed.rect[0], typed.rect[3]), (100.0, 700.0), "the words start elsewhere");
        // Two characters at twelve points, and two points of padding each side.
        assert!((typed.rect[2] - 128.0).abs() < 0.01, "{:?}", typed.rect);
    }

    /// Words a reader has not typed are not written.
    #[test]
    fn words_are_needed_where_words_are_written() {
        let mut empty = tool(Pen::Note);
        empty.words = "  \n".to_owned();
        assert_eq!(empty.placed(0, &[egui::pos2(1.0, 1.0)]).err(), Some("annotate_needs_words"));
    }

    /// **A callout points where the drag began and its box grows away from there.**
    #[test]
    fn a_callout_points_where_the_drag_began() {
        let spec =
            tool(Pen::Callout).placed(0, &drag((100.0, 100.0), (200.0, 300.0))).expect("a callout");
        let AnnotationKind::Callout { points_at, .. } = spec.kind else { panic!("{spec:?}") };
        assert_eq!(points_at, [100.0, 100.0]);
        assert_eq!(
            (spec.rect[0], spec.rect[1]),
            (200.0, 300.0),
            "the box is not at the drag's end"
        );
    }

    /// An ink stroke is every point the drag passed through, in order.
    #[test]
    fn an_ink_stroke_is_the_path_of_the_drag() {
        let path: Vec<egui::Pos2> =
            [(10.0, 10.0), (20.0, 30.0), (40.0, 20.0)].map(|(x, y)| egui::pos2(x, y)).to_vec();
        let spec = tool(Pen::Ink).placed(0, &path).expect("a stroke");
        let AnnotationKind::Ink { strokes, .. } = spec.kind else { panic!("{spec:?}") };
        assert_eq!(strokes, vec![vec![[10.0, 10.0], [20.0, 30.0], [40.0, 20.0]]]);
    }

    /// A line runs from where the drag began to where it ended, which a box cannot say.
    #[test]
    fn a_line_keeps_its_direction() {
        let spec = tool(Pen::Line).placed(0, &drag((100.0, 10.0), (10.0, 100.0))).expect("a line");
        let AnnotationKind::Shape { form: ShapeForm::Line { from, to }, .. } = spec.kind else {
            panic!("{spec:?}")
        };
        assert_eq!((from, to), ([100.0, 10.0], [10.0, 100.0]));
    }

    /// **A number is a page counted from one**; anything else is an address.
    #[test]
    fn a_link_goes_to_a_page_or_an_address() {
        let area = drag((10.0, 10.0), (60.0, 30.0));
        let spec = tool(Pen::Link).placed(0, &area).expect("a link");
        assert_eq!(spec.kind, AnnotationKind::Link { destination_page: 2, url: None });
        let mut web = tool(Pen::Link);
        web.target = "https://example.org/".to_owned();
        let spec = web.placed(0, &area).expect("a link");
        assert!(matches!(spec.kind, AnnotationKind::Link { url: Some(_), .. }));
    }

    /// **A polygon is the points clicked, three at least; a polyline two.**
    #[test]
    fn a_polygon_is_its_points() {
        let two = drag((10.0, 10.0), (60.0, 30.0));
        assert_eq!(tool(Pen::Polygon).placed(0, &two).err(), Some("annotate_needs_points"));
        let spec = tool(Pen::PolyLine).placed(0, &two).expect("a polyline");
        let AnnotationKind::Shape { form: ShapeForm::PolyLine { vertices }, .. } = spec.kind else {
            panic!("{spec:?}")
        };
        assert_eq!(vertices, vec![[10.0, 10.0], [60.0, 30.0]]);
        let mut three = two;
        three.push(egui::pos2(30.0, 80.0));
        let spec = tool(Pen::Polygon).placed(0, &three).expect("a polygon");
        assert!(matches!(spec.kind, AnnotationKind::Shape { form: ShapeForm::Polygon { .. }, .. }));
        assert_eq!(spec.rect, [10.0, 10.0, 60.0, 80.0], "the box holds every point");
    }

    /// A caret stands on the click; an attachment needs its file, and says what it is.
    #[test]
    fn a_caret_and_an_attachment_go_where_the_click_was() {
        let at = [egui::pos2(100.0, 200.0)];
        let caret = tool(Pen::Caret).placed(0, &at).expect("a caret");
        assert_eq!(caret.rect, [93.0, 186.0, 107.0, 200.0]);
        assert_eq!(tool(Pen::Attachment).placed(0, &at).err(), Some("annotate_needs_file"));
        let mut attaching = tool(Pen::Attachment);
        attaching.more.file = Some(("clip.MP4".to_owned(), vec![1, 2]));
        let spec = attaching.placed(0, &at).expect("an attachment");
        let AnnotationKind::FileAttachment { mime_type, description, .. } = spec.kind else {
            panic!("{spec:?}")
        };
        assert_eq!(mime_type.as_deref(), Some("video/mp4"));
        assert_eq!(description.as_deref(), Some("注記"), "the words say what it is");
    }

    /// **A popup opens for the comment chosen, on that comment's page.**
    #[test]
    fn a_popup_needs_its_comment_on_its_page() {
        let area = drag((10.0, 10.0), (60.0, 30.0));
        assert_eq!(tool(Pen::Popup).placed(0, &area).err(), Some("annotate_needs_parent"));
        let mut popup = tool(Pen::Popup);
        popup.more.parent = Some(fepdf::AnnotationAt { page: 1, index: 4 });
        assert_eq!(popup.placed(0, &area).err(), Some("annotate_parent_elsewhere"));
        let spec = popup.placed(1, &area).expect("a popup");
        assert_eq!(spec.kind, AnnotationKind::Popup { parent: 4, open: true });
    }

    /// What has no words to be, and a clip of no type a screen plays, are refused.
    #[test]
    fn words_and_a_playable_clip_are_asked_for() {
        let area = drag((10.0, 10.0), (60.0, 30.0));
        for pen in [Pen::Watermark, Pen::Projection] {
            let mut empty = tool(pen);
            empty.words.clear();
            assert_eq!(empty.placed(0, &area).err(), Some("annotate_needs_words"), "{pen:?}");
        }
        let mut screen = tool(Pen::Screen);
        assert!(matches!(
            screen.placed(0, &area).map(|s| s.kind),
            Ok(AnnotationKind::Screen { clip: None, .. })
        ));
        screen.more.file = Some(("notes.txt".to_owned(), vec![]));
        assert_eq!(screen.placed(0, &area).err(), Some("annotate_unknown_media"));
    }

    /// A stamp with no picture chosen says so, rather than stamping nothing.
    #[test]
    fn a_stamp_needs_its_picture() {
        let placed = tool(Pen::Stamp).placed(0, &drag((10.0, 10.0), (60.0, 30.0)));
        assert_eq!(placed.err(), Some("annotate_needs_picture"));
    }
}
