//! The ten pens of ROADMAP AA-4g, for the kinds the engine makes since AA-4f.
//!
//! **A polygon and a polyline are clicked point by point**, as the caliper's polygon is,
//! and finished by a double-click, Enter, or the drawer's button; Escape starts again. A
//! caret and an attachment go where the click was, as a note does. The rest are dragged
//! as a box. What a pen needs besides the gesture — a file, the comment a popup opens
//! for, which printer's mark — is chosen in the drawer first.

use super::{AnnotateTool, COLOURS, NOTE, Pen, Refusal};
use crate::interaction::SelectionManager;
use fepdf::comments::Comment;
use fepdf::{AnnotationAt, AnnotationKind, MediaClip, PrinterMarkKind, ShapeForm};

/// Which of the ten pens: a polygon and a polyline are one, clicked point by point.
#[derive(Clone, Copy)]
pub(super) enum Kind {
    Points,
    Caret,
    Attachment,
    Screen,
    Popup,
    PrinterMark,
    Watermark,
    Redact,
    Projection,
}

/// What the ten pens hold between frames.
pub struct More {
    /// The points clicked so far, in the page's own space.
    pub points: Vec<egui::Pos2>,
    /// The page they are on, and the pen that clicked them: a point clicked for a
    /// polyline is not a polygon's corner.
    pub points_page: Option<(usize, Pen)>,
    /// Whether the drawer's button asked for the points to be finished.
    pub finish: bool,
    /// Whether a caret stands for a new paragraph.
    pub paragraph: bool,
    /// An attachment's file, or a screen's clip: its name, and what it holds.
    pub file: Option<(String, Vec<u8>)>,
    /// The comment a popup opens for.
    pub parent: Option<AnnotationAt>,
    /// Which printer's mark.
    pub mark: PrinterMarkKind,
    /// How opaque a watermark is.
    pub opacity: f32,
}

impl Default for More {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            points_page: None,
            finish: false,
            paragraph: false,
            file: None,
            parent: None,
            mark: PrinterMarkKind::RegistrationTarget,
            opacity: 0.5,
        }
    }
}

/// Media types by extension: what a screen can play, and what an attachment is said to
/// be. Table 284 requires a clip's type, so a clip of another extension is refused.
const MEDIA: [(&str, &str); 7] = [
    ("mp4", "video/mp4"),
    ("m4v", "video/mp4"),
    ("mov", "video/quicktime"),
    ("webm", "video/webm"),
    ("mp3", "audio/mpeg"),
    ("m4a", "audio/mp4"),
    ("wav", "audio/wav"),
];

/// The media type of `filename`, by its extension.
fn media_type(filename: &str) -> Option<&'static str> {
    let extension = std::path::Path::new(filename).extension()?.to_str()?.to_ascii_lowercase();
    MEDIA.iter().find(|(e, _)| *e == extension).map(|(_, t)| *t)
}

/// What the drawer shows for the ten pens, under what every pen shows.
pub(super) fn show(
    tool: &mut AnnotateTool,
    ui: &mut egui::Ui,
    tr: &dyn Fn(&str) -> String,
    comments: &[Comment],
) {
    match tool.pen {
        Pen::Polygon | Pen::PolyLine => points(tool, ui, tr),
        Pen::Caret => {
            ui.checkbox(&mut tool.more.paragraph, tr("annotate_paragraph"));
        }
        Pen::Attachment => choose_file(tool, ui, tr, "annotate_choose_file", false),
        Pen::Screen => choose_file(tool, ui, tr, "annotate_choose_clip", true),
        Pen::Popup => parent(tool, ui, tr, comments),
        Pen::PrinterMark => {
            ui.horizontal_wrapped(|ui| {
                let marks = [
                    (PrinterMarkKind::RegistrationTarget, "annotate_mark_registration"),
                    (PrinterMarkKind::ColorBar, "annotate_mark_colour_bar"),
                ];
                for (mark, key) in marks {
                    ui.selectable_value(&mut tool.more.mark, mark, tr(key));
                }
            });
        }
        Pen::Watermark => {
            ui.horizontal(|ui| {
                ui.label(tr("annotate_opacity"));
                ui.add(egui::DragValue::new(&mut tool.more.opacity).range(0.05..=1.0).speed(0.01));
            });
        }
        Pen::Note
        | Pen::Typewriter
        | Pen::TextBox
        | Pen::Callout
        | Pen::Highlight
        | Pen::Underline
        | Pen::StrikeOut
        | Pen::Squiggly
        | Pen::Ink
        | Pen::Rectangle
        | Pen::Ellipse
        | Pen::Line
        | Pen::Stamp
        | Pen::Link
        | Pen::Redact
        | Pen::Projection => {}
    }
}

/// How many points are clicked, and the buttons that finish them or start again.
fn points(tool: &mut AnnotateTool, ui: &mut egui::Ui, tr: &dyn Fn(&str) -> String) {
    ui.horizontal(|ui| {
        let mine = tool.more.points_page.is_some_and(|(_, pen)| pen == tool.pen);
        let count = if mine { tool.more.points.len() } else { 0 }.to_string();
        // Only points this pen clicked can be finished; a request with none would linger
        // and finish the next shape at its first click.
        if ui.button(tr("annotate_finish").replace("{}", &count)).clicked() && mine {
            tool.more.finish = true;
        }
        if ui.button(tr("annotate_clear_points")).clicked() {
            tool.more.points.clear();
            tool.more.points_page = None;
        }
    });
}

/// An attachment's file or a screen's clip, from disk.
fn choose_file(
    tool: &mut AnnotateTool,
    ui: &mut egui::Ui,
    tr: &dyn Fn(&str) -> String,
    key: &str,
    media: bool,
) {
    if let Some((name, _)) = &tool.more.file {
        ui.label(name.as_str());
    }
    let mut dialog = rfd::FileDialog::new();
    if media {
        dialog = dialog.add_filter("Media", &MEDIA.map(|(e, _)| e));
    }
    if ui.button(tr(key)).clicked()
        && let Some(path) = dialog.pick_file()
    {
        tool.more.file = std::fs::read(&path).ok().map(|bytes| {
            (path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()), bytes)
        });
    }
}

/// The comment a popup opens for: one of the comments that answers nothing.
fn parent(
    tool: &mut AnnotateTool,
    ui: &mut egui::Ui,
    tr: &dyn Fn(&str) -> String,
    comments: &[Comment],
) {
    let named = |c: &Comment| {
        let page = tr("comments_page").replace("{}", &(c.at.page + 1).to_string());
        let words: String = c.contents.as_deref().unwrap_or_default().chars().take(24).collect();
        format!("{page} · {} · {words}", c.subtype)
    };
    let chosen = comments.iter().find(|c| Some(c.at) == tool.more.parent);
    let shown = chosen.map_or_else(|| tr("annotate_parent"), named);
    egui::ComboBox::from_id_salt("annotate_parent").selected_text(shown).show_ui(ui, |ui| {
        for comment in comments.iter().filter(|c| c.reply_to.is_none()) {
            ui.selectable_value(&mut tool.more.parent, Some(comment.at), named(comment));
        }
    });
}

impl AnnotateTool {
    /// The chosen colour.
    pub(super) fn colour_rgb(&self) -> [f32; 3] {
        COLOURS.get(self.colour).or(COLOURS.first()).map_or([0.0; 3], |(_, rgb)| *rgb)
    }

    /// Follows clicks on `page` for a polygon or a polyline, and answers the points once
    /// they are finished. A click on another page starts again there.
    pub(super) fn follow_points(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        page: usize,
        at: &dyn Fn(egui::Pos2) -> egui::Pos2,
        pointer: Option<egui::Pos2>,
    ) -> Option<Vec<egui::Pos2>> {
        let here = Some((page, self.pen));
        let more = &mut self.more;
        let mut added = false;
        if response.clicked()
            && let Some(pos) = pointer.map(at)
        {
            if more.points_page != here {
                more.points.clear();
                more.points_page = here;
            }
            // A double-click's second click lands where the first did, and adds nothing.
            if more.points.last().is_none_or(|last| last.distance(pos) >= 2.0) {
                more.points.push(pos);
                added = true;
            }
        }
        if more.points_page != here {
            return None;
        }
        let (enter, escape) =
            ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        if escape {
            more.points.clear();
            more.points_page = None;
            return None;
        }
        // **A double-click finishes, and so does a triple.** egui counts a click within
        // twice its double-click delay of the one before last as a third, so a reader who
        // clicked the corners briskly and double-clicked the last had a triple-click, and
        // nothing finished (traced 2026-10-11). Its second click may have added a point a
        // little off the first, which is the same corner and is taken off again.
        let twice = response.double_clicked() || response.triple_clicked();
        if twice && added {
            more.points.pop();
        }
        if !(twice || enter || more.finish) {
            return None;
        }
        more.finish = false;
        more.points_page = None;
        Some(std::mem::take(&mut more.points))
    }

    /// The points clicked so far on `page`, joined, and a line on to the pointer.
    pub(super) fn paint_points(
        &self,
        ui: &egui::Ui,
        page: usize,
        page_rect: egui::Rect,
        frame: crate::interaction::PageFrame,
        (zoom, hover): (f32, Option<egui::Pos2>),
    ) {
        if self.more.points_page != Some((page, self.pen)) || self.more.points.is_empty() {
            return;
        }
        let to = |pos| SelectionManager::pdf_to_screen(page_rect, zoom, frame, pos);
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("annotate_points"));
        let painter = ui.ctx().layer_painter(layer).with_clip_rect(page_rect);
        let stroke = egui::Stroke::new(1.5_f32, crate::app::theme::colors::rust::ACCENT);
        let mut path: Vec<egui::Pos2> = self.more.points.iter().map(|p| to(*p)).collect();
        for point in &path {
            painter.circle_filled(*point, 3.0, stroke.color);
        }
        path.extend(hover.filter(|h| page_rect.contains(*h)));
        if self.pen == Pen::Polygon
            && let Some(first) = path.first().copied()
        {
            path.push(first);
        }
        painter.add(egui::Shape::line(path, stroke));
    }

    /// What one of the ten pens places, from a click at `start` or a drag over `dragged`.
    ///
    /// # Errors
    /// The locale key of what is missing: points, words, a file, a comment, or a drag.
    pub(super) fn more_placed(
        &self,
        kind: Kind,
        page: usize,
        (start, dragged, has_area): (egui::Pos2, egui::Rect, bool),
        points: &[egui::Pos2],
    ) -> Result<(egui::Rect, AnnotationKind), Refusal> {
        let words = self.words.trim_end().to_owned();
        let said = (!words.trim().is_empty()).then(|| words.clone());
        match kind {
            Kind::Points => self.run_of(points),
            // The caret's point is where the click was, and it stands on it.
            Kind::Caret => Ok((
                egui::Rect::from_min_max(
                    egui::pos2(start.x - 7.0, start.y - 14.0),
                    egui::pos2(start.x + 7.0, start.y),
                ),
                AnnotationKind::Caret {
                    contents: words,
                    color_rgb: self.colour_rgb(),
                    paragraph: self.more.paragraph,
                },
            )),
            Kind::Attachment => self.attachment(start, said),
            Kind::Screen
            | Kind::Popup
            | Kind::PrinterMark
            | Kind::Watermark
            | Kind::Redact
            | Kind::Projection
                if has_area =>
            {
                Ok((dragged, self.boxed(kind, page, dragged, said)?))
            }
            Kind::Screen
            | Kind::Popup
            | Kind::PrinterMark
            | Kind::Watermark
            | Kind::Redact
            | Kind::Projection => Err("annotate_needs_drag"),
        }
    }

    /// An attachment: the file chosen, under an icon whose top left corner is at the
    /// click, as a note's is.
    fn attachment(
        &self,
        start: egui::Pos2,
        description: Option<String>,
    ) -> Result<(egui::Rect, AnnotationKind), Refusal> {
        let (filename, data) = self.more.file.clone().ok_or("annotate_needs_file")?;
        let mime_type = media_type(&filename).map(str::to_owned);
        let icon = egui::Rect::from_min_max(
            egui::pos2(start.x, start.y - NOTE),
            egui::pos2(start.x + NOTE, start.y),
        );
        Ok((icon, AnnotationKind::FileAttachment { filename, mime_type, data, description }))
    }

    /// What a pen dragged as a box places in `dragged`, its words being `said`.
    fn boxed(
        &self,
        kind: Kind,
        page: usize,
        dragged: egui::Rect,
        said: Option<String>,
    ) -> Result<AnnotationKind, Refusal> {
        Ok(match (kind, said) {
            (Kind::Screen, title) => AnnotationKind::Screen { title, clip: self.clip()? },
            (Kind::Popup, _) => {
                let parent = self.more.parent.ok_or("annotate_needs_parent")?;
                if parent.page != page {
                    return Err("annotate_parent_elsewhere");
                }
                AnnotationKind::Popup { parent: parent.index, open: true }
            }
            (Kind::PrinterMark, _) => AnnotationKind::PrinterMark { mark: self.more.mark },
            // The words fill the box dragged: as tall as its lines allow, and the engine
            // narrows them to its width.
            (Kind::Watermark, Some(text)) => {
                #[allow(clippy::cast_precision_loss)] // a watermark has a handful of lines
                let lines = text.lines().count().max(1) as f32;
                let font_size = dragged.height() / (lines * 1.2);
                AnnotationKind::Watermark { text, font_size, opacity: self.more.opacity }
            }
            (Kind::Projection, Some(contents)) => AnnotationKind::Projection { contents },
            (Kind::Redact, overlay_text) => {
                AnnotationKind::Redact { overlay_text, interior_rgb: Some(self.colour_rgb()) }
            }
            (Kind::Watermark | Kind::Projection, None) => return Err("annotate_needs_words"),
            (Kind::Points | Kind::Caret | Kind::Attachment, _) => {
                return Err("annotate_needs_drag");
            }
        })
    }

    /// A polygon or a polyline through `points`: three at least for the one, two for the
    /// other.
    fn run_of(&self, points: &[egui::Pos2]) -> Result<(egui::Rect, AnnotationKind), Refusal> {
        let closed = self.pen == Pen::Polygon;
        if points.len() < if closed { 3 } else { 2 } {
            return Err("annotate_needs_points");
        }
        let vertices = points.iter().map(|p| [p.x, p.y]).collect();
        let form =
            if closed { ShapeForm::Polygon { vertices } } else { ShapeForm::PolyLine { vertices } };
        let kind = AnnotationKind::Shape { form, color_rgb: self.colour_rgb(), width: self.width };
        Ok((egui::Rect::from_points(points), kind))
    }

    /// A screen's clip, where one is chosen; refused where its type is not one it plays.
    fn clip(&self) -> Result<Option<MediaClip>, Refusal> {
        let Some((filename, data)) = self.more.file.clone() else { return Ok(None) };
        let mime_type = media_type(&filename).ok_or("annotate_unknown_media")?.to_owned();
        Ok(Some(MediaClip { filename, mime_type, data }))
    }
}
