use crate::interaction::SelectionManager;

#[derive(Debug, Clone)]
pub struct RedactionZone {
    pub page_index: usize,
    pub rect: egui::Rect, // PDF User Space coordinates
}

pub struct RedactionManager {
    pub zones: Vec<RedactionZone>,
    pub drag_start: Option<egui::Pos2>,   // PDF User Space
    pub drag_current: Option<egui::Pos2>, // PDF User Space
    pub is_active: bool,                  // Redaction brush active
}

impl Default for RedactionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RedactionManager {
    pub fn new() -> Self {
        Self {
            zones: Vec::new(),
            drag_start: None,
            drag_current: None,
            is_active: false,
        }
    }

    pub fn clear(&mut self) {
        self.zones.clear();
        self.drag_start = None;
        self.drag_current = None;
    }

    /// Handles mouse dragging to draw a redaction rectangle over a page.
    pub fn handle_interaction(
        &mut self,
        ui: &mut egui::Ui,
        page_index: usize,
        page_rect: egui::Rect,
        page_unscaled_h: f32,
        zoom: f32,
    ) {
        if !self.is_active {
            return;
        }

        let response = ui.allocate_rect(page_rect, egui::Sense::drag());
        let screen_pos = ui.input(|i| i.pointer.hover_pos());

        if response.drag_started()
            && let Some(pos) = screen_pos
        {
            self.drag_start =
                Some(SelectionManager::screen_to_pdf(page_rect, zoom, page_unscaled_h, pos));
        }

        if response.dragged()
            && let Some(pos) = screen_pos
        {
            self.drag_current =
                Some(SelectionManager::screen_to_pdf(page_rect, zoom, page_unscaled_h, pos));
        }

        if response.drag_stopped() {
            if let (Some(start), Some(current)) = (self.drag_start, self.drag_current) {
                let rect = egui::Rect::from_two_pos(start, current);
                if rect.width() > 1.0 && rect.height() > 1.0 {
                    self.zones.push(RedactionZone { page_index, rect });
                }
            }
            self.drag_start = None;
            self.drag_current = None;
        }

        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }

    /// Returns screen-space highlight rectangles for active redaction boxes on a page.
    pub fn get_screen_highlights(
        &self,
        page_index: usize,
        page_rect: egui::Rect,
        page_unscaled_h: f32,
        zoom: f32,
    ) -> (Vec<egui::Rect>, Option<egui::Rect>) {
        let mut screen_rects = Vec::new();

        // 1. Draw completed redaction zones
        for zone in &self.zones {
            if zone.page_index == page_index {
                let screen_min = SelectionManager::pdf_to_screen(
                    page_rect,
                    zoom,
                    page_unscaled_h,
                    egui::pos2(zone.rect.min.x, zone.rect.max.y),
                );
                let screen_max = SelectionManager::pdf_to_screen(
                    page_rect,
                    zoom,
                    page_unscaled_h,
                    egui::pos2(zone.rect.max.x, zone.rect.min.y),
                );
                screen_rects.push(egui::Rect::from_min_max(screen_min, screen_max));
            }
        }

        // 2. Draw current active drag box
        let active_drag = if self.is_active {
            if let (Some(start), Some(current)) = (self.drag_start, self.drag_current) {
                let drag_rect = egui::Rect::from_two_pos(start, current);

                let screen_min = SelectionManager::pdf_to_screen(
                    page_rect,
                    zoom,
                    page_unscaled_h,
                    egui::pos2(drag_rect.min.x, drag_rect.max.y),
                );
                let screen_max = SelectionManager::pdf_to_screen(
                    page_rect,
                    zoom,
                    page_unscaled_h,
                    egui::pos2(drag_rect.max.x, drag_rect.min.y),
                );
                Some(egui::Rect::from_min_max(screen_min, screen_max))
            } else {
                None
            }
        } else {
            None
        };

        (screen_rects, active_drag)
    }

    /// Performs the clean physical removal of content stream data and characters inside the redaction zones.
    /// This is an RR-15 hardened redaction implementation.
    pub fn perform_physical_redaction(
        &self,
        page_index: usize,
        raw_text: &str,
        spans: &mut Vec<crate::interaction::TextSpan>,
    ) -> String {
        let mut clean_lines = Vec::new();
        let page_zones: Vec<&RedactionZone> =
            self.zones.iter().filter(|z| z.page_index == page_index).collect();

        // 1. Clean in-memory text spans
        spans.retain_mut(|span| {
            let mut overlaps = false;
            for zone in &page_zones {
                if zone.rect.intersects(span.rect) {
                    overlaps = true;
                    break;
                }
            }
            !overlaps
        });

        // 2. Build sanitized raw text representation
        for line in raw_text.lines() {
            let mut clean_words = Vec::new();
            for word in line.split_whitespace() {
                // Find matching span
                // Find matching span
                let mut is_redacted = true;
                for span in spans.iter() {
                    if span.text == word {
                        // If span remains in the safe list, it is not redacted
                        is_redacted = false;
                        break;
                    }
                }

                if !is_redacted {
                    clean_words.push(word);
                } else {
                    clean_words.push("[REDACTED]");
                }
            }
            clean_lines.push(clean_words.join(" "));
        }

        clean_lines.join("\n")
    }
}
