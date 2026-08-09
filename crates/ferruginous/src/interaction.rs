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
    pub selected_text: String,
    pub highlights: BTreeMap<usize, Vec<egui::Rect>>, // Page -> Screen-space highlights
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
        self.selected_text.clear();
        self.highlights.clear();
        self.pending_tag_request = None;
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
    pub fn handle_interaction(
        &mut self,
        ui: &mut egui::Ui,
        page_index: usize,
        page_rect: egui::Rect,
        page_unscaled_h: f32,
        spans: &[TextSpan],
        zoom: f32,
    ) {
        let response = ui.allocate_rect(page_rect, egui::Sense::drag());

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
            self.recalculate_selection(page_index, page_rect, page_unscaled_h, spans, zoom);
        }

        if response.drag_stopped() {
            // Optional: copy to clipboard automatically or let user copy via Ctrl+C
            if !self.selected_text.is_empty() {
                ui.ctx().copy_text(self.selected_text.clone());
            }
        }

        // Draw hover effect (cursor)
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
    }

    fn recalculate_selection(
        &mut self,
        page_index: usize,
        page_rect: egui::Rect,
        page_unscaled_h: f32,
        spans: &[TextSpan],
        zoom: f32,
    ) {
        let (Some(start), Some(current)) = (self.drag_start, self.drag_current) else {
            return;
        };

        // Create PDF space selection bounding box
        let select_rect = egui::Rect::from_two_pos(start, current);

        let mut selected_spans = Vec::new();
        let mut page_highlights = Vec::new();

        for span in spans {
            if select_rect.intersects(span.rect) {
                selected_spans.push(span.clone());

                // Map PDF-space span rect to screen-space highlight rect
                let screen_min = Self::pdf_to_screen(
                    page_rect,
                    zoom,
                    page_unscaled_h,
                    egui::pos2(span.rect.min.x, span.rect.max.y),
                );
                let screen_max = Self::pdf_to_screen(
                    page_rect,
                    zoom,
                    page_unscaled_h,
                    egui::pos2(span.rect.max.x, span.rect.min.y),
                );
                page_highlights.push(egui::Rect::from_min_max(screen_min, screen_max));
            }
        }

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
            let mut intersecting_spans = Vec::new();
            let mut combined_rect = egui::Rect::NOTHING;

            for span in spans {
                if select_rect.intersects(span.rect) {
                    intersecting_spans.push(span.clone());
                    combined_rect = combined_rect.union(span.rect);
                }
            }

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
            self.recalculate_brush_highlights(page_index, page_rect, page_unscaled_h, spans, zoom);
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

    fn recalculate_brush_highlights(
        &mut self,
        page_index: usize,
        page_rect: egui::Rect,
        page_unscaled_h: f32,
        spans: &[TextSpan],
        zoom: f32,
    ) {
        let (Some(start), Some(current)) = (self.drag_start, self.drag_current) else {
            return;
        };

        let select_rect = egui::Rect::from_two_pos(start, current);
        let mut page_highlights = Vec::new();

        for span in spans {
            if select_rect.intersects(span.rect) {
                let screen_min = Self::pdf_to_screen(
                    page_rect,
                    zoom,
                    page_unscaled_h,
                    egui::pos2(span.rect.min.x, span.rect.max.y),
                );
                let screen_max = Self::pdf_to_screen(
                    page_rect,
                    zoom,
                    page_unscaled_h,
                    egui::pos2(span.rect.max.x, span.rect.min.y),
                );
                page_highlights.push(egui::Rect::from_min_max(screen_min, screen_max));
            }
        }

        self.highlights.insert(page_index, page_highlights);
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
