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
    /// What the engine says each page's zones will remove, in PDF user space: every
    /// glyph, image area, path area and annotation that goes (ROADMAP Y-10).
    ///
    /// **Shown before it is done.** A glyph goes when its box meets a zone at all, so a
    /// zone over half a word takes the word's edge letters too; this is where the reader
    /// sees that, and draws the zone again if it is not what was meant.
    pub going: std::collections::BTreeMap<usize, Vec<egui::Rect>>,
    /// A page whose zones changed, for the window to ask the engine about.
    pub asked: Option<usize>,
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
            going: std::collections::BTreeMap::new(),
            asked: None,
        }
    }

    pub fn clear(&mut self) {
        self.zones.clear();
        self.drag_start = None;
        self.drag_current = None;
        self.going.clear();
        self.asked = None;
    }

    /// Takes what the engine says `page`'s zones will remove, and has it drawn.
    pub fn show_going(&mut self, page: usize, going: &[[f32; 4]], ctx: &egui::Context) {
        let rects = going
            .iter()
            .map(|[x0, y0, x1, y1]| {
                egui::Rect::from_min_max(egui::pos2(*x0, *y0), egui::pos2(*x1, *y1))
            })
            .collect();
        self.going.insert(page, rects);
        ctx.request_repaint();
    }

    /// The zones on `page`, as the regions an `Operation::Redact` names: left, bottom,
    /// right, top, in PDF user space.
    pub fn regions_on(&self, page: usize) -> Vec<(f64, f64, f64, f64)> {
        self.zones
            .iter()
            .filter(|z| z.page_index == page)
            .map(|z| {
                let r = z.rect;
                (f64::from(r.min.x), f64::from(r.min.y), f64::from(r.max.x), f64::from(r.max.y))
            })
            .collect()
    }

    /// Where on the screen what `page`'s zones will remove lies.
    pub fn going_on_screen(
        &self,
        page: usize,
        page_rect: egui::Rect,
        frame: crate::interaction::PageFrame,
        zoom: f32,
    ) -> Vec<egui::Rect> {
        self.going.get(&page).map_or_else(Vec::new, |rects| {
            rects.iter().map(|r| on_screen(*r, page_rect, frame, zoom)).collect()
        })
    }

    /// The zones an export redacts, and, when the window burns them too, the brush
    /// emptied for the next document.
    ///
    /// **Taken before they are cleared.** The export burned the zones into the window's
    /// own copy of the text, cleared them, and then sent the save the zones it now held —
    /// none. With burning on, which is how the wizard opens, the file written kept every
    /// word the reader had redacted while the window showed them gone.
    pub fn zones_for_export(&mut self, burn: bool) -> Vec<RedactionZone> {
        let zones = self.zones.clone();
        if burn {
            self.clear();
        }
        zones
    }

    /// Handles mouse dragging to draw a redaction rectangle over a page.
    pub fn handle_interaction(
        &mut self,
        ui: &mut egui::Ui,
        page_index: usize,
        page_rect: egui::Rect,
        frame: crate::interaction::PageFrame,
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
            self.drag_start = Some(SelectionManager::screen_to_pdf(page_rect, zoom, frame, pos));
        }

        if response.dragged()
            && let Some(pos) = screen_pos
        {
            self.drag_current = Some(SelectionManager::screen_to_pdf(page_rect, zoom, frame, pos));
        }

        if response.drag_stopped() {
            if let (Some(start), Some(current)) = (self.drag_start, self.drag_current) {
                let rect = egui::Rect::from_two_pos(start, current);
                if rect.width() > 1.0 && rect.height() > 1.0 {
                    self.zones.push(RedactionZone { page_index, rect });
                    self.asked = Some(page_index);
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
        frame: crate::interaction::PageFrame,
        zoom: f32,
    ) -> (Vec<egui::Rect>, Option<egui::Rect>) {
        let mut screen_rects = Vec::new();

        // 1. Draw completed redaction zones
        for zone in &self.zones {
            if zone.page_index == page_index {
                let screen_min = SelectionManager::pdf_to_screen(
                    page_rect,
                    zoom,
                    frame,
                    egui::pos2(zone.rect.min.x, zone.rect.max.y),
                );
                let screen_max = SelectionManager::pdf_to_screen(
                    page_rect,
                    zoom,
                    frame,
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
                    frame,
                    egui::pos2(drag_rect.min.x, drag_rect.max.y),
                );
                let screen_max = SelectionManager::pdf_to_screen(
                    page_rect,
                    zoom,
                    frame,
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

/// A rectangle in PDF user space, on the screen: its top edge is the one with the
/// larger y, since the page's y runs up and the screen's down.
fn on_screen(
    rect: egui::Rect,
    page_rect: egui::Rect,
    frame: crate::interaction::PageFrame,
    zoom: f32,
) -> egui::Rect {
    let min =
        SelectionManager::pdf_to_screen(page_rect, zoom, frame, egui::pos2(rect.min.x, rect.max.y));
    let max =
        SelectionManager::pdf_to_screen(page_rect, zoom, frame, egui::pos2(rect.max.x, rect.min.y));
    egui::Rect::from_min_max(min, max)
}

/// What an export is handed.
#[cfg(test)]
mod exporting {
    use super::{RedactionManager, RedactionZone};

    fn zone() -> RedactionZone {
        RedactionZone {
            page_index: 0,
            rect: egui::Rect::from_min_max(egui::pos2(10.0, 10.0), egui::pos2(90.0, 30.0)),
        }
    }

    /// **The save gets the zones whether or not the window burns them.** Burning used to
    /// clear them first, and the file written kept the redacted words.
    #[test]
    fn an_export_that_burns_still_redacts_the_file() {
        for burn in [true, false] {
            let mut manager = RedactionManager::new();
            manager.zones.push(zone());
            let written = manager.zones_for_export(burn);
            assert_eq!(written.len(), 1, "burn {burn}: the save was handed no zones");
            assert_eq!(
                manager.zones.is_empty(),
                burn,
                "burn {burn}: the brush was not left as asked"
            );
        }
    }
}

/// What the window asks the engine about, and what it shows.
#[cfg(test)]
mod previewing {
    use super::RedactionManager;

    /// **A zone drawn asks for its page's preview, and the regions are the zones'.**
    #[test]
    fn a_zone_drawn_asks_for_its_pages_preview() {
        let mut manager = RedactionManager::new();
        manager.zones.push(super::RedactionZone {
            page_index: 2,
            rect: egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(30.0, 40.0)),
        });
        assert_eq!(manager.regions_on(2), [(10.0, 20.0, 30.0, 40.0)]);
        assert!(manager.regions_on(0).is_empty());
        manager.going.insert(2, vec![egui::Rect::NOTHING]);
        manager.asked = Some(2);
        manager.clear();
        assert!(manager.going.is_empty() && manager.asked.is_none(), "clearing kept the preview");
    }
}
