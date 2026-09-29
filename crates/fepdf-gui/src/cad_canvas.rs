use crate::app::theme::{canvas, colors};
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
    /// The locale key naming what kind of point this is.
    ///
    /// **A key and not a sentence**, for the same reason `WorkerResponse::Busy` carries
    /// one: this file computes geometry and holds no language. The seven of these were
    /// English written into the source — `Corner Vertex`, `Edge Midpoint` — and drawn
    /// beside the cursor in every language the window offers.
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
                description: "snap_corner",
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
                description: "snap_midpoint",
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
                description: "snap_base",
            });
            points.push(SnapPoint {
                point: egui::pos2(r.max.x, r.max.y),
                snap_type: SnapType::EndPoint,
                description: "snap_terminus",
            });
            points.push(SnapPoint {
                point: r.center(),
                snap_type: SnapType::MidPoint,
                description: "snap_centroid",
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
                description: "snap_junction",
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
    /// A distance or a shape.
    pub mode: crate::measuring::CaliperMode,
    /// The shape's corners so far, in PDF space.
    pub polygon: Vec<egui::Pos2>,
    /// The page the measurement is on.
    pub page: Option<usize>,
    /// The scales each page declares, as the worker read them (12.9).
    pub scales: BTreeMap<usize, Vec<fepdf::measure::Scale>>,
    /// Each page's `/UserUnit`, which arrives with its scales: a unit of the page's user
    /// space is that many 1/72 inch on the sheet.
    pub user_units: BTreeMap<usize, f64>,
    /// The pages whose scales have been asked for and not yet answered.
    pub asked: std::collections::BTreeSet<usize>,
    /// The scale the drawer would set.
    pub form: crate::measuring::ScaleForm,
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
            mode: crate::measuring::CaliperMode::default(),
            polygon: Vec::new(),
            page: None,
            scales: BTreeMap::new(),
            user_units: BTreeMap::new(),
            asked: std::collections::BTreeSet::new(),
            form: crate::measuring::ScaleForm::default(),
        }
    }

    /// The page whose scales should be asked for: the one being measured on, when they
    /// are neither known nor already asked for.
    pub fn scales_wanted(&mut self) -> Option<usize> {
        let page = self.page?;
        if self.scales.contains_key(&page) || !self.asked.insert(page) {
            return None;
        }
        Some(page)
    }

    /// Keeps the scales and the `/UserUnit` the worker read for `page`.
    pub fn scales_arrived(
        &mut self,
        page: usize,
        scales: Vec<fepdf::measure::Scale>,
        user_unit: f64,
    ) {
        self.asked.remove(&page);
        self.scales.insert(page, scales);
        self.user_units.insert(page, user_unit);
    }

    /// How many 1/72 inch a unit of `page`'s user space is: 1 until the worker has said.
    pub fn user_unit(&self, page: usize) -> f64 {
        self.user_units.get(&page).copied().unwrap_or(1.0)
    }

    /// Forgets every page's scales, which an edit may have changed.
    pub fn forget_scales(&mut self) {
        self.scales.clear();
        self.user_units.clear();
        self.asked.clear();
    }

    pub fn clear(&mut self) {
        self.start_point = None;
        self.current_point = None;
        self.current_snap = None;
        self.measured_dist = None;
        self.caliper_line = None;
        self.polygon.clear();
        self.page = None;
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

        let response = ui.allocate_rect(page_screen_rect, egui::Sense::click_and_drag());
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

            // A shape is taken a corner at a click, on the page its first corner is on.
            if self.mode == crate::measuring::CaliperMode::Polygon {
                if response.clicked() {
                    if self.page != Some(page_index) {
                        self.polygon.clear();
                    }
                    self.page = Some(page_index);
                    self.polygon.push(hovered_snap.map_or(pdf_pos, |s| s.point));
                }
            } else if response.drag_started() {
                self.clear();
                if let Some(snap) = hovered_snap {
                    self.start_point = Some(snap);
                } else {
                    self.start_point = Some(SnapPoint {
                        point: pdf_pos,
                        snap_type: SnapType::EndPoint,
                        description: "snap_cursor",
                    });
                }
                self.page = Some(page_index);
            }

            if response.dragged() && self.mode == crate::measuring::CaliperMode::Distance {
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
        (page_index, page_screen_rect): (usize, egui::Rect),
        page_unscaled_h: f32,
        zoom: f32,
        tr: &dyn Fn(&str) -> String,
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

            // **The three are told apart by their shape, which is what every CAD
            // package does and what a drawing printed in one colour leaves possible.**
            // A green, a cyan and an orange stood here, on a page whose own colours the
            // engine has just gone to some trouble to get right.
            let size = 7.0_f32;
            let stroke = egui::Stroke::new(1.5_f32, colors::steel::TEXT);
            let halo = egui::Stroke::new(3.0_f32, colors::paper::WHITE);
            let marker = |width: f32, stroke: egui::Stroke| match snap.snap_type {
                // A square for an end point, a triangle for a mid point, a cross for an
                // intersection.
                SnapType::EndPoint => painter.rect_stroke(
                    egui::Rect::from_center_size(screen_pos, egui::vec2(width * 2.0, width * 2.0)),
                    0.0,
                    stroke,
                    egui::StrokeKind::Outside,
                ),
                SnapType::MidPoint => painter.add(egui::Shape::closed_line(
                    vec![
                        screen_pos + egui::vec2(0.0, -width),
                        screen_pos + egui::vec2(width, width),
                        screen_pos + egui::vec2(-width, width),
                    ],
                    stroke,
                )),
                SnapType::Intersection => {
                    painter.line_segment(
                        [
                            screen_pos + egui::vec2(-width, -width),
                            screen_pos + egui::vec2(width, width),
                        ],
                        stroke,
                    );
                    painter.line_segment(
                        [
                            screen_pos + egui::vec2(width, -width),
                            screen_pos + egui::vec2(-width, width),
                        ],
                        stroke,
                    )
                }
            };
            marker(size, halo);
            marker(size, stroke);

            canvas::haloed_text(
                painter,
                screen_pos + egui::vec2(12.0, -12.0),
                egui::Align2::LEFT_CENTER,
                &format!("{} ({:.1}, {:.1})", tr(snap.description), snap.point.x, snap.point.y),
                egui::FontId::proportional(crate::app::theme::text::SMALL),
                colors::steel::TEXT,
            );
        }

        // What was measured is drawn on the page it was measured on, and no other.
        if self.page != Some(page_index) {
            return;
        }

        // The shape so far, closed back to its first corner once it has three.
        if self.polygon.len() >= 2 {
            let to = |p| {
                crate::interaction::SelectionManager::pdf_to_screen(
                    page_screen_rect,
                    zoom,
                    page_unscaled_h,
                    p,
                )
            };
            let corners: Vec<egui::Pos2> = self.polygon.iter().map(|p| to(*p)).collect();
            let stroke = egui::Stroke::new(2.0_f32, colors::rust::ACCENT);
            if corners.len() >= 3 {
                painter.add(egui::Shape::closed_line(corners, stroke));
            } else {
                painter.line(corners, stroke);
            }
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
                egui::Stroke::new(2.0_f32, colors::rust::ACCENT),
            );

            // Draw small ticks at start/end endpoints
            let dir = (end_screen - start_screen).normalized();
            let normal = egui::vec2(-dir.y, dir.x) * 6.0;

            painter.line(
                vec![start_screen - normal, start_screen + normal],
                egui::Stroke::new(1.5_f32, colors::rust::ACCENT),
            );
            painter.line(
                vec![end_screen - normal, end_screen + normal],
                egui::Stroke::new(1.5_f32, colors::rust::ACCENT),
            );

            // Draw floating HUD box
            if let Some(dist) = self.measured_dist {
                let mid_screen = start_screen + (end_screen - start_screen) * 0.5;
                let text = format!("{dist:.2} pt");
                let text_font = egui::FontId::monospace(crate::app::theme::text::BODY);

                // Draw background card for readability
                painter.rect_filled(
                    egui::Rect::from_center_size(
                        mid_screen + egui::vec2(0.0, -15.0),
                        egui::vec2(75.0, 20.0),
                    ),
                    4.0,
                    egui::Color32::from_black_alpha(200),
                );

                canvas::haloed_text(
                    painter,
                    mid_screen + egui::vec2(0.0, -15.0),
                    egui::Align2::CENTER_CENTER,
                    &text,
                    text_font,
                    colors::rust::ACCENT,
                );
            }
        }
    }
}
