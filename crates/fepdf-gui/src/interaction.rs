use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct TextSpan {
    pub text: String,
    pub rect: egui::Rect, // PDF User Space coordinates (0, 0 at bottom-left)
}

#[derive(Clone, Debug)]
pub struct PendingTagRequest {
    pub page_index: usize,
    pub combined_rect: egui::Rect, // PDF User Space coordinates
    pub text: String,
}

pub struct SelectionManager {
    pub active_page: Option<usize>,
    pub drag_start: Option<egui::Pos2>, // PDF User Space coordinates
    pub drag_current: Option<egui::Pos2>, // PDF User Space coordinates
    pub marquee_start: Option<egui::Pos2>, // Screen space coordinates for box selection
    pub marquee_current: Option<egui::Pos2>, // Screen space coordinates
    pub selected_text: String,
    /// What is selected on each page, in PDF user space.
    ///
    /// **Not screen space, which is where these used to be kept.** A highlight computed
    /// once at drag time and drawn as it stood is a highlight that stays where the screen
    /// was: zooming or panning left the selection behind, over whatever had moved into
    /// its place. A selection belongs to the page, so it is stored in the page's own
    /// coordinates and mapped to the screen by whoever draws it, every frame.
    pub highlights: BTreeMap<usize, Vec<egui::Rect>>,
    pub is_tagging_brush_active: bool,
    pub pending_tag_request: Option<PendingTagRequest>,
}

impl Default for SelectionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectionManager {
    pub fn new() -> Self {
        Self {
            active_page: None,
            drag_start: None,
            drag_current: None,
            marquee_start: None,
            marquee_current: None,
            selected_text: String::new(),
            highlights: BTreeMap::new(),
            is_tagging_brush_active: false,
            pending_tag_request: None,
        }
    }

    pub fn clear(&mut self) {
        self.active_page = None;
        self.drag_start = None;
        self.drag_current = None;
        self.marquee_start = None;
        self.marquee_current = None;
        self.selected_text.clear();
        self.highlights.clear();
        self.pending_tag_request = None;
    }

    pub fn marquee_rect(&self) -> Option<egui::Rect> {
        match (self.marquee_start, self.marquee_current) {
            (Some(s), Some(c)) => {
                let rect = egui::Rect::from_two_pos(s, c);
                if rect.width() > 3.0 || rect.height() > 3.0 { Some(rect) } else { None }
            }
            _ => None,
        }
    }

    /// Maps screen coordinate to PDF space.
    pub fn screen_to_pdf(
        page_rect: egui::Rect,
        zoom: f32,
        page_h: f32,
        pos: egui::Pos2,
    ) -> egui::Pos2 {
        let x = (pos.x - page_rect.min.x) / zoom;
        let y = page_h - (pos.y - page_rect.min.y) / zoom;
        egui::pos2(x, y)
    }

    /// Maps PDF space coordinate to screen space.
    pub fn pdf_to_screen(
        page_rect: egui::Rect,
        zoom: f32,
        page_h: f32,
        pos: egui::Pos2,
    ) -> egui::Pos2 {
        let x = pos.x.mul_add(zoom, page_rect.min.x);
        let y = (page_h - pos.y).mul_add(zoom, page_rect.min.y);
        egui::pos2(x, y)
    }

    /// Generates high-fidelity simulated TextSpans from raw extracted text of the page.
    /// Distributes lines and words evenly inside the page boundaries for sub-pixel hit testing.
    pub fn generate_spans_for_page(text: &str, page_w: f32, page_h: f32) -> Vec<TextSpan> {
        let mut spans = Vec::new();
        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return spans;
        }

        // Layout parameters
        let top_margin = 50.0f32;
        let bottom_margin = 50.0f32;
        let left_margin = 50.0f32;
        let right_margin = 50.0f32;

        let available_h = page_h - top_margin - bottom_margin;
        let available_w = page_w - left_margin - right_margin;

        let line_height = (available_h / lines.len() as f32).min(24.0);

        for (row_idx, line) in lines.iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            // PDF coordinates: Y starts at 0 at bottom
            let line_y = (row_idx as f32).mul_add(-line_height, page_h - top_margin);

            let words: Vec<&str> = line.split_whitespace().collect();
            if words.is_empty() {
                continue;
            }

            let word_gap = 6.0f32;
            let total_gap_w = (words.len() - 1) as f32 * word_gap;
            let total_word_chars: usize = words.iter().map(|w| w.len()).sum();

            let char_w = if total_word_chars > 0 {
                (available_w - total_gap_w) / total_word_chars as f32
            } else {
                10.0
            };

            let mut current_x = left_margin;

            for word in words {
                let word_w = word.len() as f32 * char_w;
                let rect = egui::Rect::from_min_size(
                    egui::pos2(current_x, line_y - line_height * 0.8),
                    egui::vec2(word_w, line_height),
                );

                spans.push(TextSpan { text: word.to_string(), rect });

                current_x += word_w + word_gap;
            }
        }

        spans
    }

    /// Handles mouse dragging to select text spans on a page.
    pub fn handle_drag(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        page_index: usize,
        page_rect: egui::Rect,
        page_unscaled_h: f32,
        spans: &[TextSpan],
        zoom: f32,
    ) {
        let screen_pos = ui.input(|i| i.pointer.hover_pos());

        if response.drag_started()
            && let Some(pos) = screen_pos
        {
            self.clear();
            self.active_page = Some(page_index);
            self.drag_start = Some(Self::screen_to_pdf(page_rect, zoom, page_unscaled_h, pos));
        }

        if response.dragged()
            && let Some(pos) = screen_pos
            && self.active_page == Some(page_index)
        {
            self.drag_current = Some(Self::screen_to_pdf(page_rect, zoom, page_unscaled_h, pos));
            self.recalculate_selection(page_index, spans);
        }

        if response.drag_stopped() && !self.selected_text.is_empty() {
            ui.ctx().copy_text(self.selected_text.clone());
        }

        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
    }

    /// The spans a drag rect covers.
    ///
    /// **Three places ask this** — text selection, the redaction brush, and the brush's
    /// drop — and each wrote the test out.
    fn spans_under(select_rect: egui::Rect, spans: &[TextSpan]) -> impl Iterator<Item = &TextSpan> {
        spans.iter().filter(move |span| select_rect.intersects(span.rect))
    }

    /// The screen rect that highlights `span`.
    ///
    /// **The corners swap.** A PDF rect is y-up and the screen is y-down, so the screen's
    /// top-left comes from the span's `(min.x, max.y)` and its bottom-right from
    /// `(max.x, min.y)`. Taking the corners straight across gives a rect of negative
    /// height, which draws as nothing rather than as something visibly wrong. Selection
    /// and the brush each wrote this out.
    pub fn highlight_rect(
        page_rect: egui::Rect,
        zoom: f32,
        page_unscaled_h: f32,
        span: egui::Rect,
    ) -> egui::Rect {
        egui::Rect::from_min_max(
            Self::pdf_to_screen(
                page_rect,
                zoom,
                page_unscaled_h,
                egui::pos2(span.min.x, span.max.y),
            ),
            Self::pdf_to_screen(
                page_rect,
                zoom,
                page_unscaled_h,
                egui::pos2(span.max.x, span.min.y),
            ),
        )
    }

    /// Works out what the drag covers, entirely in the page's own coordinates.
    ///
    /// **It used to take the page's rectangle on screen and the zoom**, because it turned
    /// each selected span into a screen rect there and then. Nothing it does now depends
    /// on where the page is being drawn, which is the point: a selection that knows the
    /// screen is a selection that is left behind when the screen moves.
    pub(crate) fn recalculate_selection(&mut self, page_index: usize, spans: &[TextSpan]) {
        let (Some(start), Some(current)) = (self.drag_start, self.drag_current) else {
            return;
        };

        // Create PDF space selection bounding box
        let select_rect = egui::Rect::from_two_pos(start, current);

        let selected_spans: Vec<TextSpan> =
            Self::spans_under(select_rect, spans).cloned().collect();
        let page_highlights: Vec<egui::Rect> = selected_spans.iter().map(|s| s.rect).collect();

        // Build selected text
        let mut text = String::new();
        for (i, span) in selected_spans.iter().enumerate() {
            if i > 0 {
                text.push(' ');
            }
            text.push_str(&span.text);
        }

        self.selected_text = text;
        self.highlights.insert(page_index, page_highlights);
    }

    fn handle_brush_drag_stop(
        &mut self,
        page_index: usize,
        spans: &[TextSpan],
        start: egui::Pos2,
        current: egui::Pos2,
    ) {
        let select_rect = egui::Rect::from_two_pos(start, current);
        if select_rect.width() > 2.0 && select_rect.height() > 2.0 {
            let intersecting_spans: Vec<TextSpan> =
                Self::spans_under(select_rect, spans).cloned().collect();
            let combined_rect = intersecting_spans
                .iter()
                .fold(egui::Rect::NOTHING, |acc, span| acc.union(span.rect));

            if !intersecting_spans.is_empty() {
                let combined_text = intersecting_spans
                    .iter()
                    .map(|s| s.text.clone())
                    .collect::<Vec<String>>()
                    .join(" ");

                self.pending_tag_request =
                    Some(PendingTagRequest { page_index, combined_rect, text: combined_text });
            }
        }
    }

    pub fn handle_tagging_brush_interaction(
        &mut self,
        ui: &mut egui::Ui,
        page_index: usize,
        page_rect: egui::Rect,
        page_unscaled_h: f32,
        spans: &[TextSpan],
        zoom: f32,
    ) {
        if !self.is_tagging_brush_active {
            return;
        }

        let response = ui.allocate_rect(page_rect, egui::Sense::drag());
        let screen_pos = ui.input(|i| i.pointer.hover_pos());

        if response.drag_started()
            && let Some(pos) = screen_pos
        {
            self.clear();
            self.drag_start = Some(Self::screen_to_pdf(page_rect, zoom, page_unscaled_h, pos));
        }

        if response.dragged()
            && let Some(pos) = screen_pos
        {
            self.drag_current = Some(Self::screen_to_pdf(page_rect, zoom, page_unscaled_h, pos));
            // The same question as a text drag asks, so the same function answers it.
            // The two had one body each, and one of them stopped being updated.
            self.recalculate_selection(page_index, spans);
        }

        if response.drag_stopped() {
            if let (Some(start), Some(current)) = (self.drag_start, self.drag_current) {
                self.handle_brush_drag_stop(page_index, spans, start, current);
            }
            self.drag_start = None;
            self.drag_current = None;
        }

        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE_H: f32 = 800.0;

    fn page_rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(600.0, PAGE_H))
    }

    #[test]
    fn pdf_and_screen_coordinates_round_trip() {
        let rect = page_rect();
        for zoom in [0.5_f32, 1.0, 2.5] {
            let original = egui::pos2(123.5, 456.25);
            let screen = SelectionManager::pdf_to_screen(rect, zoom, PAGE_H, original);
            let back = SelectionManager::screen_to_pdf(rect, zoom, PAGE_H, screen);
            assert!((back.x - original.x).abs() < 1e-3, "x drifted at zoom {zoom}");
            assert!((back.y - original.y).abs() < 1e-3, "y drifted at zoom {zoom}");
        }
    }

    #[test]
    fn pdf_origin_is_bottom_left_of_the_page() {
        // PDF user space grows upwards, screen space downwards. The PDF origin must
        // therefore land on the page's bottom edge, not its top.
        let rect = page_rect();
        let origin = SelectionManager::pdf_to_screen(rect, 1.0, PAGE_H, egui::pos2(0.0, 0.0));
        assert!((origin.x - rect.min.x).abs() < 1e-3);
        assert!((origin.y - rect.max.y).abs() < 1e-3);

        let top = SelectionManager::pdf_to_screen(rect, 1.0, PAGE_H, egui::pos2(0.0, PAGE_H));
        assert!((top.y - rect.min.y).abs() < 1e-3);
    }

    #[test]
    fn zoom_scales_distance_from_the_page_origin() {
        let rect = page_rect();
        let at_1x = SelectionManager::pdf_to_screen(rect, 1.0, PAGE_H, egui::pos2(100.0, 0.0));
        let at_2x = SelectionManager::pdf_to_screen(rect, 2.0, PAGE_H, egui::pos2(100.0, 0.0));
        assert!((at_1x.x - rect.min.x - 100.0).abs() < 1e-3);
        assert!((at_2x.x - rect.min.x - 200.0).abs() < 1e-3);
    }
}

#[cfg(test)]
mod drag_coverage {
    use super::{SelectionManager, TextSpan};

    const PAGE_H: f32 = 800.0;

    fn page_rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(600.0, PAGE_H))
    }

    fn span(text: &str, x0: f32, y0: f32, x1: f32, y1: f32) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            rect: egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1)),
        }
    }

    /// **A highlight must have positive height.** The PDF rect is y-up and the screen is
    /// y-down, so the screen's top-left comes from the span's `(min.x, max.y)`. Taking the
    /// corners straight across gives a rect of negative height, which draws as nothing —
    /// a highlight that silently fails to appear rather than appearing wrong.
    #[test]
    fn a_highlight_is_the_right_way_up_on_screen() {
        let rect = SelectionManager::highlight_rect(
            page_rect(),
            1.0,
            PAGE_H,
            span("x", 10.0, 20.0, 60.0, 40.0).rect,
        );
        assert!(rect.height() > 0.0, "height was {}", rect.height());
        assert!(rect.width() > 0.0, "width was {}", rect.width());
        assert!(
            (rect.height() - 20.0).abs() < 1e-3,
            "a 20-unit-tall span at zoom 1 is 20 tall, not {}",
            rect.height()
        );
    }

    /// The highlight scales with the zoom it is drawn at.
    #[test]
    fn a_highlight_scales_with_the_zoom() {
        let s = span("x", 10.0, 20.0, 60.0, 40.0);
        let one = SelectionManager::highlight_rect(page_rect(), 1.0, PAGE_H, s.rect);
        let two = SelectionManager::highlight_rect(page_rect(), 2.0, PAGE_H, s.rect);
        assert!(
            one.height().mul_add(-2.0, two.height()).abs() < 1e-3,
            "{} against {}",
            two.height(),
            one.height()
        );
    }

    /// Selection, the redaction brush and the brush's drop all ask this one question.
    #[test]
    fn a_drag_covers_the_spans_it_touches_and_no_others() {
        let spans = vec![
            span("above", 0.0, 100.0, 50.0, 120.0),
            span("inside", 0.0, 10.0, 50.0, 30.0),
            span("touching", 40.0, 25.0, 90.0, 45.0),
        ];
        let drag = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(45.0, 35.0));
        let covered: Vec<&str> =
            SelectionManager::spans_under(drag, &spans).map(|s| s.text.as_str()).collect();
        assert_eq!(covered, vec!["inside", "touching"]);
    }
}

#[cfg(test)]
mod selecting_real_text {
    use super::{SelectionManager, TextSpan};

    /// The spans a page of the corpus yields, as the worker builds them.
    fn spans(name: &str, index: usize) -> Vec<TextSpan> {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples").join(name);
        let bytes = std::fs::read(path).expect("the sample is in the tree");
        let doc = fepdf::PdfDocument::open_with_options(
            bytes.into(),
            &fepdf::IngestionOptions::default(),
        )
        .expect("the sample opens");
        doc.extract_spans(index)
            .expect("the page interprets")
            .into_iter()
            .map(|s| TextSpan {
                #[allow(clippy::cast_possible_truncation)]
                rect: egui::Rect::from_two_pos(
                    egui::pos2(s.x as f32, s.y as f32),
                    egui::pos2((s.x + s.width) as f32, (s.y + s.font_size) as f32),
                ),
                text: s.text,
            })
            .collect()
    }

    /// **A drag across the whole page selects the text on it.** Reported from the window:
    /// text cannot be selected. This is the half of that path with no pointer in it — the
    /// spans a real page yields, and the rectangle a drag from one corner to the other
    /// makes in the same space.
    #[test]
    fn a_drag_over_the_page_covers_the_text_on_it() {
        let spans = spans("constitution.pdf", 0);
        assert!(!spans.is_empty(), "the page yielded no spans at all");
        let whole_page = egui::Rect::from_two_pos(egui::pos2(0.0, 0.0), egui::pos2(612.0, 792.0));
        let covered = SelectionManager::spans_under(whole_page, &spans).count();
        assert_eq!(covered, spans.len(), "a drag over everything missed some of it");
    }

    /// **What is kept is where the selection is on the page, not where it was on the
    /// screen.** Reported from the window: "the selection is left behind when you zoom
    /// with something selected". A rect computed once at drag time and drawn as it stood
    /// stays where the screen was, over whatever has moved into its place.
    #[test]
    fn what_is_selected_is_kept_in_the_pages_own_coordinates() {
        let spans = spans("constitution.pdf", 0);
        let mut manager = SelectionManager::new();
        manager.drag_start = Some(egui::pos2(0.0, 0.0));
        manager.drag_current = Some(egui::pos2(612.0, 792.0));
        manager.recalculate_selection(0, &spans);

        let kept = manager.highlights.get(&0).expect("something was selected");
        assert_eq!(kept.len(), spans.len(), "a drag over the page kept {} of them", kept.len());
        assert!(
            kept.iter().zip(&spans).all(|(k, s)| *k == s.rect),
            "the rects kept are not the spans' own"
        );
    }

    /// And that it lands in the right place at any zoom, which is the point of keeping it
    /// that way.
    #[test]
    fn a_selection_is_drawn_where_the_page_is_at_any_zoom() {
        let spans = spans("constitution.pdf", 0);
        let span = spans[0].rect;
        for zoom in [0.5_f32, 1.0, 2.0] {
            let page_rect =
                egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(612.0, 792.0) * zoom);
            let drawn = SelectionManager::highlight_rect(page_rect, zoom, 792.0, span);
            let text = SelectionManager::pdf_to_screen(
                page_rect,
                zoom,
                792.0,
                egui::pos2(span.min.x, span.max.y),
            );
            assert!(
                (drawn.min - text).length() < 0.01,
                "at {zoom}x the highlight is at {:?} and the text at {text:?}",
                drawn.min
            );
        }
    }

    /// The same drag in the coordinates a pointer actually arrives in.
    #[test]
    fn a_drag_in_screen_coordinates_reaches_the_same_spans() {
        let spans = spans("constitution.pdf", 0);
        let page_rect =
            egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(612.0, 792.0));
        let start = SelectionManager::screen_to_pdf(page_rect, 1.0, 792.0, page_rect.min);
        let end = SelectionManager::screen_to_pdf(page_rect, 1.0, 792.0, page_rect.max);
        let dragged = egui::Rect::from_two_pos(start, end);
        let covered = SelectionManager::spans_under(dragged, &spans).count();
        assert_eq!(covered, spans.len(), "the screen-space drag missed some of the page");
    }
}
