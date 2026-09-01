use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapType {
    EndPoint,
    MidPoint,
    Intersection,
}

#[derive(Clone, Copy, Debug)]
pub struct SnapPoint {
    pub point: egui::Pos2, // PDF space
    pub snap_type: SnapType,
    pub description: &'static str,
}

pub struct CadSnapEngine {
    // We cache geometric key points for each page.
    // In a fully-production environment, this is populated during content stream rendering.
    pub page_snap_points: BTreeMap<usize, Vec<SnapPoint>>,
}

impl CadSnapEngine {
    pub fn new() -> Self {
        Self { page_snap_points: BTreeMap::new() }
    }

    /// Populates simulated snapping points for the page based on text spans and page layout.
    /// This mimics real vector path extraction for demonstration.
    fn add_margin_snap_points(&self, points: &mut Vec<SnapPoint>, page_w: f32, page_h: f32) {
        let margins = [50.0_f32, 50.0_f32];
        let w_act = margins[0].mul_add(-2.0, page_w);
        let h_act = margins[1].mul_add(-2.0, page_h);

        let corners = [
            egui::pos2(margins[0], margins[1]),
            egui::pos2(margins[0] + w_act, margins[1]),
            egui::pos2(margins[0], margins[1] + h_act),
            egui::pos2(margins[0] + w_act, margins[1] + h_act),
        ];

        for &c in &corners {
            points.push(SnapPoint {
                point: c,
                snap_type: SnapType::EndPoint,
                description: "Corner Vertex",
            });
        }

        let midpoints = [
            egui::pos2(margins[0] + w_act / 2.0, margins[1]),
            egui::pos2(margins[0], margins[1] + h_act / 2.0),
            egui::pos2(margins[0] + w_act, margins[1] + h_act / 2.0),
            egui::pos2(margins[0] + w_act / 2.0, margins[1] + h_act),
        ];

        for &m in &midpoints {
            points.push(SnapPoint {
                point: m,
                snap_type: SnapType::MidPoint,
                description: "Edge Midpoint",
            });
        }
    }

    fn add_text_span_snap_points(
        &self,
        points: &mut Vec<SnapPoint>,
        text_spans: &[crate::interaction::TextSpan],
    ) {
        for span in text_spans.iter().take(12) {
            let r = span.rect;
            points.push(SnapPoint {
                point: egui::pos2(r.min.x, r.min.y),
                snap_type: SnapType::EndPoint,
                description: "Base Point",
            });
            points.push(SnapPoint {
                point: egui::pos2(r.max.x, r.max.y),
                snap_type: SnapType::EndPoint,
                description: "Terminus",
            });
            points.push(SnapPoint {
                point: r.center(),
                snap_type: SnapType::MidPoint,
                description: "Centroid",
            });
        }
    }

    pub fn ensure_snap_points(
        &mut self,
        page_index: usize,
        page_w: f32,
        page_h: f32,
        text_spans: &[crate::interaction::TextSpan],
    ) {
        if self.page_snap_points.contains_key(&page_index) {
            return;
        }

        let mut points = Vec::new();

        // 1. Add page margins corners and midpoints
        self.add_margin_snap_points(&mut points, page_w, page_h);

        // 2. Add text span bounding box endpoints and midpoints
        self.add_text_span_snap_points(&mut points, text_spans);

        // 3. Add simulated intersection point
        if points.len() >= 2 {
            let p1 = points[0].point;
            let p2 = points[1].point;
            points.push(SnapPoint {
                point: egui::pos2(f32::midpoint(p1.x, p2.x), f32::midpoint(p1.y, p2.y) + 10.0),
                snap_type: SnapType::Intersection,
                description: "Path Junction",
            });
        }

        self.page_snap_points.insert(page_index, points);
    }

    /// Finds the closest snap point within a threshold radius (in screen coordinates).
    pub fn find_snap(
        &self,
        page_index: usize,
        pointer_pdf: egui::Pos2,
        _page_screen_rect: egui::Rect,
        _page_unscaled_h: f32,
        zoom: f32,
        threshold_screen: f32,
    ) -> Option<SnapPoint> {
        let points = self.page_snap_points.get(&page_index)?;
        let mut closest_snap = None;
        let mut min_dist_screen = threshold_screen;

        for &snap in points {
            // PDF distance
            let dx = snap.point.x - pointer_pdf.x;
            let dy = snap.point.y - pointer_pdf.y;
            let dist_pdf = dx.hypot(dy);

            // Convert to screen distance
            let dist_screen = dist_pdf * zoom;

            if dist_screen < min_dist_screen {
                min_dist_screen = dist_screen;
                closest_snap = Some(snap);
            }
        }

        closest_snap
    }
}

pub struct CaliperTool {
    pub is_active: bool,
    pub start_point: Option<SnapPoint>,
    pub current_point: Option<egui::Pos2>, // PDF space
    pub current_snap: Option<SnapPoint>,
    pub measured_dist: Option<f32>,
    pub caliper_line: Option<(egui::Pos2, egui::Pos2)>, // PDF space start/end
}

impl CaliperTool {
    pub fn new() -> Self {
        Self {
            is_active: false,
            start_point: None,
            current_point: None,
            current_snap: None,
            measured_dist: None,
            caliper_line: None,
        }
    }

    pub fn clear(&mut self) {
        self.start_point = None;
        self.current_point = None;
        self.current_snap = None;
        self.measured_dist = None;
        self.caliper_line = None;
    }

    pub fn handle_interaction(
        // RR-15 Limit: GUI - signature UI layout section declaration for Caliper interaction
        &mut self,
        ui: &mut egui::Ui,
        page_index: usize,
        page_screen_rect: egui::Rect,
        page_unscaled_h: f32,
        zoom: f32,
        snap_engine: &mut CadSnapEngine,
        text_spans: &[crate::interaction::TextSpan],
    ) {
        if !self.is_active {
            return;
        }

        // Ensure page snap points exist
        snap_engine.ensure_snap_points(
            page_index,
            page_screen_rect.width() / zoom,
            page_screen_rect.height() / zoom,
            text_spans,
        );

        let response = ui.allocate_rect(page_screen_rect, egui::Sense::drag());
        let screen_pos = ui.input(|i| i.pointer.hover_pos());

        if let Some(pos) = screen_pos {
            let pdf_pos = crate::interaction::SelectionManager::screen_to_pdf(
                page_screen_rect,
                zoom,
                page_unscaled_h,
                pos,
            );

            // Real-time hover snapping (15px threshold)
            let hovered_snap = snap_engine.find_snap(
                page_index,
                pdf_pos,
                page_screen_rect,
                page_unscaled_h,
                zoom,
                15.0,
            );
            self.current_snap = hovered_snap;

            if response.drag_started() {
                self.clear();
                if let Some(snap) = hovered_snap {
                    self.start_point = Some(snap);
                } else {
                    self.start_point = Some(SnapPoint {
                        point: pdf_pos,
                        snap_type: SnapType::EndPoint,
                        description: "Cursor Origin",
                    });
                }
            }

            if response.dragged() {
                let target_pos = hovered_snap.map_or(pdf_pos, |s| s.point);
                self.current_point = Some(target_pos);

                if let Some(start) = &self.start_point {
                    let dx = target_pos.x - start.point.x;
                    let dy = target_pos.y - start.point.y;
                    self.measured_dist = Some(dx.hypot(dy));
                    self.caliper_line = Some((start.point, target_pos));
                }
            }
        }

        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }

    pub fn draw_overlay(
        // RR-15 Limit: GUI - Renders CAD snap lines and ticks directly onto the page drawing layout overlay
        &self,
        ui: &mut egui::Ui,
        page_screen_rect: egui::Rect,
        page_unscaled_h: f32,
        zoom: f32,
    ) {
        if !self.is_active {
            return;
        }

        let painter = ui.painter();

        // 1. Draw snap marker hover indicator
        if let Some(snap) = &self.current_snap {
            let screen_pos = crate::interaction::SelectionManager::pdf_to_screen(
                page_screen_rect,
                zoom,
                page_unscaled_h,
                snap.point,
            );

            let (color, size) = match snap.snap_type {
                SnapType::EndPoint => (egui::Color32::from_rgb(0, 255, 128), 7.0),
                SnapType::MidPoint => (egui::Color32::from_rgb(0, 192, 255), 8.0),
                SnapType::Intersection => (egui::Color32::from_rgb(255, 128, 0), 9.0),
            };

            painter.rect_stroke(
                egui::Rect::from_center_size(screen_pos, egui::vec2(size * 2.0, size * 2.0)),
                0.0,
                egui::Stroke::new(1.5_f32, color),
                egui::StrokeKind::Outside,
            );

            painter.text(
                screen_pos + egui::vec2(12.0, -12.0),
                egui::Align2::LEFT_CENTER,
                format!("{} ({:.1}, {:.1})", snap.description, snap.point.x, snap.point.y),
                egui::FontId::proportional(11.0),
                egui::Color32::LIGHT_GRAY,
            );
        }

        // 2. Draw Caliper Measurement Line & Text Overlay
        if let Some((start_pdf, end_pdf)) = self.caliper_line {
            let start_screen = crate::interaction::SelectionManager::pdf_to_screen(
                page_screen_rect,
                zoom,
                page_unscaled_h,
                start_pdf,
            );
            let end_screen = crate::interaction::SelectionManager::pdf_to_screen(
                page_screen_rect,
                zoom,
                page_unscaled_h,
                end_pdf,
            );

            // Draw line
            painter.line(
                vec![start_screen, end_screen],
                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(255, 215, 0)),
            );

            // Draw small ticks at start/end endpoints
            let dir = (end_screen - start_screen).normalized();
            let normal = egui::vec2(-dir.y, dir.x) * 6.0;

            painter.line(
                vec![start_screen - normal, start_screen + normal],
                egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(255, 215, 0)),
            );
            painter.line(
                vec![end_screen - normal, end_screen + normal],
                egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(255, 215, 0)),
            );

            // Draw floating HUD box
            if let Some(dist) = self.measured_dist {
                let mid_screen = start_screen + (end_screen - start_screen) * 0.5;
                let text = format!("{dist:.2} pt");
                let text_font = egui::FontId::monospace(12.0);

                // Draw background card for readability
                painter.rect_filled(
                    egui::Rect::from_center_size(
                        mid_screen + egui::vec2(0.0, -15.0),
                        egui::vec2(75.0, 20.0),
                    ),
                    4.0,
                    egui::Color32::from_black_alpha(200),
                );

                painter.text(
                    mid_screen + egui::vec2(0.0, -15.0),
                    egui::Align2::CENTER_CENTER,
                    text,
                    text_font,
                    egui::Color32::from_rgb(255, 215, 0),
                );
            }
        }
    }

    pub fn show_panel(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("📐 CAD Caliper & Snap Measurement").strong());
            ui.add_space(4.0);
            ui.label(
                "Drag across any vector elements, corners, or text on the page to measure exact distances in PDF user points.",
            );
            ui.add_space(8.0);
            if let Some(dist) = self.measured_dist {
                ui.horizontal(|ui| {
                    ui.label("Distance:");
                    ui.label(
                        egui::RichText::new(format!("{:.2} pt  ({:.2} mm)", dist, dist * 25.4 / 72.0))
                            .strong()
                            .color(egui::Color32::from_rgb(255, 215, 0)),
                    );
                });
            } else {
                ui.label(
                    egui::RichText::new("No active measurement. Click & drag on the canvas.").weak(),
                );
            }
            ui.add_space(8.0);
            if ui.button("Clear Measurement").clicked() {
                self.clear();
            }
        });
    }
}
