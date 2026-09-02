use std::collections::BTreeMap;

#[derive(Clone)]
pub struct PageLayout {
    pub index: usize,
    pub rect: egui::Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Continuous,
    SinglePage,
    TwoPageSpread,
    TwoPageSingle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingDirection {
    LeftToRight,
    RightToLeft,
}

/// Where the page area gets its pixels for this frame.
///
/// **The two are alternatives, not a flag and a value**, which is why this is a type: in
/// one the whole viewport is a single texture composed from every visible page's vector
/// scene, and in the other each page is its own small texture. Passing both and choosing
/// inside would allow a state that means nothing.
pub enum PagePixels<'a> {
    /// One texture covering the viewport. `None` while it is being created.
    Viewport(Option<egui::TextureId>),
    /// One thumbnail per page, drawn at that page's rect. Pages absent from the map have
    /// not been rendered yet and keep their placeholder.
    Thumbnails(&'a BTreeMap<usize, egui::TextureId>),
}

pub struct PDFView {
    /// Private, so the only ways to change it are [`Self::set_zoom`] and
    /// [`Self::zoom_at`]. It was `pub`, four callers assigned it directly, and three of
    /// them wrote out the same `clamp(0.1, 10.0)` — a bound repeated is a bound that
    /// drifts, and one caller forgetting it is a view that cannot be zoomed back.
    zoom: f32,
    /// What a continuous gesture has accumulated, before snapping.
    ///
    /// **`zoom` alone would stick to a step and never leave it.** A pinch computes its next
    /// value from the current one, so once snapping had pulled it onto 1.00, every
    /// small delta landed back inside the snap band and was pulled onto 1.00 again.
    /// Accumulating the raw value separately lets the gesture travel through the band.
    /// Nothing renders from this; it is the gesture's own memory.
    zoom_unsnapped: f32,
    pub pan: egui::Vec2,
    pub visible_pages: Vec<usize>,
    pub display_mode: DisplayMode,
    pub active_page: usize,
    pub scroll_direction: ScrollDirection,
    pub binding_direction: BindingDirection,
    pub cover_page_alone: bool,
    pub overscroll_accumulator: egui::Vec2,
}

impl PDFView {
    pub fn get_spread_indices(&self, page_index: usize, total_pages: usize) -> Vec<usize> {
        if total_pages == 0 {
            return Vec::new();
        }
        if self.cover_page_alone {
            if page_index == 0 {
                vec![0]
            } else {
                let pair_index = ((page_index - 1) / 2) * 2 + 1;
                let mut spread = vec![pair_index];
                if pair_index + 1 < total_pages {
                    spread.push(pair_index + 1);
                }
                spread
            }
        } else {
            let pair_index = (page_index / 2) * 2;
            let mut spread = vec![pair_index];
            if pair_index + 1 < total_pages {
                spread.push(pair_index + 1);
            }
            spread
        }
    }
    pub fn new() -> Self {
        Self {
            zoom: 1.0,
            zoom_unsnapped: 1.0,
            pan: egui::Vec2::ZERO,
            visible_pages: Vec::new(),
            display_mode: DisplayMode::Continuous,
            active_page: 0,
            scroll_direction: ScrollDirection::Vertical,
            binding_direction: BindingDirection::LeftToRight,
            cover_page_alone: true,
            overscroll_accumulator: egui::Vec2::ZERO,
        }
    }
    pub fn get_origin(&self, viewport_rect: egui::Rect) -> egui::Pos2 {
        let origin_x = viewport_rect.center().x;
        let origin_y = if self.scroll_direction == ScrollDirection::Horizontal
            || self.display_mode == DisplayMode::SinglePage
            || self.display_mode == DisplayMode::TwoPageSingle
        {
            viewport_rect.center().y
        } else {
            viewport_rect.min.y + 20.0
        };
        egui::pos2(origin_x, origin_y) + self.pan
    }

    pub fn get_origin_no_pan(&self, viewport_rect: egui::Rect) -> egui::Pos2 {
        let origin_x = viewport_rect.center().x;
        let origin_y = if self.scroll_direction == ScrollDirection::Horizontal
            || self.display_mode == DisplayMode::SinglePage
            || self.display_mode == DisplayMode::TwoPageSingle
        {
            viewport_rect.center().y
        } else {
            viewport_rect.min.y + 20.0
        };
        egui::pos2(origin_x, origin_y)
    }

    /// The current zoom factor. Read freely; changing it goes through
    /// [`Self::set_zoom`] or [`Self::zoom_at`].
    #[must_use]
    pub const fn zoom(&self) -> f32 {
        self.zoom
    }

    /// The bounds a zoom factor is held to, in one place.
    ///
    /// Written out three times before — inside `zoom_at` and twice in `fit_to_width` — and
    /// a bound repeated is a bound that drifts.
    const ZOOM_BOUNDS: std::ops::RangeInclusive<f32> = 0.1..=10.0;

    /// The zoom the view stops being a page being read and becomes a sheet of thumbnails.
    ///
    /// Below it the pages tile, a drag reorders them, `Cmd+A` selects all of them, and text
    /// selection is off. It was written out at nine call sites across three files before it
    /// had a name.
    pub const OVERVIEW_ZOOM: f32 = 0.65;

    /// Below this, what is on the page cannot be read, so nothing that acts on the *content*
    /// of a page is offered.
    ///
    /// Body text is set at 10 to 11pt, so at 40% it draws at a little over 4pt and its
    /// x-height is around 2pt. **The boundary deliberately sits between two steps** — 33%
    /// and 50% straddle it — so that stepping through [`Self::ZOOM_STEPS`] never lands on
    /// the boundary itself and leaves the mode ambiguous.
    pub const LEGIBLE_ZOOM: f32 = 0.40;

    /// Where double-clicking out lands: the first step that is unambiguously an overview.
    pub const OVERVIEW_STEP: f32 = 0.33;

    /// The zooms the buttons, the keyboard and the menu move between.
    ///
    /// **A multiplier could not reach them.** The buttons used to scale the current zoom by
    /// 1.2, so from any value not already on a step — anything a pinch or a fit had
    /// produced — no number of presses ever arrived at 100%; a separate reset button existed
    /// to paper over it. Stepping along a fixed ladder lands on 100% from anywhere, and the
    /// percentage the status bar prints is then the percentage in force.
    const ZOOM_STEPS: [f32; 18] = [
        0.10, 0.125, 0.15, 0.20, 0.25, 0.33, 0.50, 0.67, 0.75, 1.00, 1.25, 1.50, 2.00, 3.00, 4.00,
        6.00, 8.00, 10.00,
    ];

    /// How close a continuous gesture must come to a step before it is taken to mean it.
    ///
    /// Without this a pinch can stop at 99.4% and be indistinguishable from 100% on screen
    /// and in the label while rendering differently.
    const SNAP_TOLERANCE: f32 = 0.02;

    /// The first step above the current zoom, or the top of the range.
    #[must_use]
    pub fn zoom_step_up(&self) -> f32 {
        Self::ZOOM_STEPS
            .iter()
            .copied()
            .find(|step| *step > self.zoom * (1.0 + Self::SNAP_TOLERANCE))
            .unwrap_or(*Self::ZOOM_BOUNDS.end())
    }

    /// The first step below the current zoom, or the bottom of the range.
    #[must_use]
    pub fn zoom_step_down(&self) -> f32 {
        Self::ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|step| *step < self.zoom * (1.0 - Self::SNAP_TOLERANCE))
            .unwrap_or(*Self::ZOOM_BOUNDS.start())
    }

    /// A zoom pulled onto a step when it is within [`Self::SNAP_TOLERANCE`] of one.
    fn snap_to_step(zoom: f32) -> f32 {
        Self::ZOOM_STEPS
            .iter()
            .copied()
            .find(|step| (zoom - step).abs() <= step * Self::SNAP_TOLERANCE)
            .unwrap_or(zoom)
    }

    /// Whether the view is showing pages to be read rather than tiles to be arranged.
    #[must_use]
    pub fn is_reading_view(&self) -> bool {
        self.zoom >= Self::OVERVIEW_ZOOM
    }

    /// Whether what is drawn is large enough that acting on the content of a page means
    /// anything. See [`Self::LEGIBLE_ZOOM`].
    #[must_use]
    pub fn is_legible(&self) -> bool {
        self.zoom >= Self::LEGIBLE_ZOOM
    }

    /// Sets the zoom without moving anything, for a caller that places the view itself.
    ///
    /// **Not the same operation as [`Self::zoom_at`], which is why both exist.** `zoom_at`
    /// keeps a chosen point under the cursor and computes `pan` to do it; this is for
    /// `reset_view`, `fit_to_width` and double-click-to-fit, which set `pan` explicitly on
    /// the line after and would have that work thrown away. Routing them through `zoom_at`
    /// would compute an anchor nobody reads.
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(*Self::ZOOM_BOUNDS.start(), *Self::ZOOM_BOUNDS.end());
        self.zoom_unsnapped = self.zoom;
    }

    /// Sets `zoom` alone, leaving the gesture's accumulator where it is.
    ///
    /// `zoom_at` records the raw target itself and must not have it overwritten with the
    /// snapped one, which is exactly what `set_zoom` would do.
    fn apply_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(*Self::ZOOM_BOUNDS.start(), *Self::ZOOM_BOUNDS.end());
    }

    /// The zoom a gesture should compute its next value from. See [`Self::zoom_unsnapped`].
    #[must_use]
    pub const fn zoom_before_snapping(&self) -> f32 {
        self.zoom_unsnapped
    }

    /// The page under `center_pos`, else the nearest, else the active one.
    ///
    /// Only pages the current display mode actually shows are candidates: zooming in
    /// single-page mode must not anchor to a page that is not on screen, and a two-page
    /// spread anchors within its own spread.
    fn layout_under<'a>(
        &self,
        center_pos: egui::Pos2,
        current_origin: egui::Pos2,
        old_zoom: f32,
        layouts: &'a [PageLayout],
    ) -> Option<&'a PageLayout> {
        let (mut closest, mut min_dist_sq) = (None, f32::MAX);
        for layout in layouts {
            if self.display_mode == DisplayMode::SinglePage && layout.index != self.active_page {
                continue;
            }
            if self.display_mode == DisplayMode::TwoPageSingle {
                let spread = self.get_spread_indices(self.active_page, layouts.len());
                if !spread.contains(&layout.index) {
                    continue;
                }
            }
            let page_screen_rect = egui::Rect::from_min_size(
                current_origin + layout.rect.min.to_vec2() * old_zoom,
                layout.rect.size() * old_zoom,
            );
            if page_screen_rect.contains(center_pos) {
                return Some(layout);
            }
            let dist_sq = page_screen_rect.distance_sq_to_pos(center_pos);
            if dist_sq < min_dist_sq {
                min_dist_sq = dist_sq;
                closest = Some(layout);
            }
        }
        closest.or_else(|| layouts.get(self.active_page))
    }

    /// Zooms to `new_zoom`, anchoring strictly to the page (and the local position on that page) under `center_pos`.
    pub fn zoom_at(
        &mut self,
        new_zoom: f32,
        center_pos: egui::Pos2,
        viewport_rect: egui::Rect,
        layouts: &[PageLayout],
    ) {
        let old_zoom = self.zoom;
        let raw = new_zoom.clamp(*Self::ZOOM_BOUNDS.start(), *Self::ZOOM_BOUNDS.end());
        // Recorded before the early return: a gesture that is crossing a snap band must
        // keep accumulating even on the frames where the snapped zoom does not move.
        self.zoom_unsnapped = raw;
        let new_zoom = Self::snap_to_step(raw);
        if (new_zoom - old_zoom).abs() < f32::EPSILON {
            return;
        }

        let origin_no_pan = self.get_origin_no_pan(viewport_rect);
        let current_origin = origin_no_pan + self.pan;

        let target_layout = self.layout_under(center_pos, current_origin, old_zoom, layouts);

        if let Some(layout) = target_layout {
            // Page-anchored zoom: calculate the point on this page in unscaled page coordinates
            let page_screen_min = current_origin + layout.rect.min.to_vec2() * old_zoom;
            let local_offset_doc = (center_pos - page_screen_min) / old_zoom;

            self.apply_zoom(new_zoom);
            // Place the exact same local page point under center_pos after zoom
            self.pan = (center_pos - origin_no_pan)
                - (layout.rect.min.to_vec2() + local_offset_doc) * new_zoom;
        } else {
            let cursor_doc = (center_pos - origin_no_pan - self.pan) / old_zoom;
            self.apply_zoom(new_zoom);
            self.pan = (center_pos - origin_no_pan) - cursor_doc * new_zoom;
        }
    }

    pub fn scroll_to_page(&mut self, page_index: usize, layouts: &[PageLayout]) {
        self.active_page = page_index;
        if self.display_mode == DisplayMode::Continuous
            || self.display_mode == DisplayMode::TwoPageSpread
        {
            if let Some(layout) = layouts.get(page_index) {
                if self.scroll_direction == ScrollDirection::Vertical {
                    self.pan.y = -layout.rect.min.y * self.zoom;
                    self.pan.x = 0.0;
                } else {
                    self.pan.x = -layout.rect.min.x * self.zoom;
                    self.pan.y = 0.0;
                }
            }
        } else if self.display_mode == DisplayMode::TwoPageSingle {
            // In TwoPageSingle, we center the active spread's bounding box relative to origin
            let spread_indices = self.get_spread_indices(page_index, layouts.len());
            if !spread_indices.is_empty() {
                let mut min_x = f32::MAX;
                let mut max_x = f32::MIN;
                let mut min_y = f32::MAX;
                let mut max_y = f32::MIN;
                for &idx in &spread_indices {
                    if let Some(layout) = layouts.get(idx) {
                        min_x = min_x.min(layout.rect.min.x);
                        max_x = max_x.max(layout.rect.max.x);
                        min_y = min_y.min(layout.rect.min.y);
                        max_y = max_y.max(layout.rect.max.y);
                    }
                }
                self.pan.x = -f32::midpoint(min_x, max_x) * self.zoom;
                self.pan.y = -f32::midpoint(min_y, max_y) * self.zoom;
            }
        } else if self.display_mode == DisplayMode::SinglePage {
            if let Some(layout) = layouts.get(page_index) {
                // In SinglePage, we center the page on both x and y
                self.pan.x = -layout.rect.center().x * self.zoom;
                self.pan.y = -layout.rect.center().y * self.zoom;
            }
        } else {
            self.pan = egui::Vec2::ZERO;
        }
    }

    pub fn center_on_rect(
        &mut self,
        viewport_rect: egui::Rect,
        page_layout: &PageLayout,
        rect: [f32; 4],
    ) {
        let pdf_center_x = f32::midpoint(rect[0], rect[2]);
        let pdf_center_y = f32::midpoint(rect[1], rect[3]);

        let unscaled_h = page_layout.rect.height();

        // Convert to egui page-local coordinate system (Y=0 is top)
        let local_x = pdf_center_x;
        let local_y = unscaled_h - pdf_center_y;

        // In virtual space (relative to layout center/top):
        let page_local_pos = page_layout.rect.min + egui::vec2(local_x, local_y);

        // We want origin + page_local_pos * zoom = viewport_rect.center()
        let origin_no_pan = self.get_origin_no_pan(viewport_rect);
        self.pan = viewport_rect.center().to_vec2()
            - origin_no_pan.to_vec2()
            - page_local_pos.to_vec2() * self.zoom;
    }

    pub fn show_virtual(
        // RR-15 Limit: Dispatcher - Renders a virtualized grid layout of PDF pages and overlays highlights/signals
        &mut self,
        ui: &mut egui::Ui,
        layouts: &[PageLayout],
        pixels: &PagePixels<'_>,
        viewport_rect: egui::Rect, // Unified viewport rect from app.rs
        scenes: &std::collections::BTreeMap<usize, std::sync::Arc<vello::Scene>>,
        highlights: &BTreeMap<usize, Vec<egui::Rect>>,
        redaction_highlights: &BTreeMap<usize, Vec<egui::Rect>>,
        active_redaction_drag: &Option<(usize, egui::Rect)>,
        structural_highlight: &Option<(usize, egui::Rect)>,
        signature_highlight: &Option<(usize, egui::Rect)>,
        selected_pages: &std::collections::BTreeSet<usize>,
        ust_registry: &crate::sidebar::USTRegistry,
        show_reading_order: bool,
        marquee_rect: Option<egui::Rect>,
    ) {
        // Completely disable egui's default focus ring/outline/selection stroke before allocating any rects to prevent flashing orange/red borders
        let visuals = ui.visuals_mut();
        visuals.selection.stroke = egui::Stroke::NONE;
        visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;

        let response = ui.allocate_rect(viewport_rect, egui::Sense::click_and_drag());
        self.handle_input(ui, &response, viewport_rect, layouts);
        self.clamp_pan(viewport_rect, layouts);

        // 1. Workspace background & CAD Grid lines
        ui.painter().rect_filled(viewport_rect, 0.0, crate::app::theme::colors::CANVAS_BG);
        Self::draw_canvas_grid(ui.painter(), viewport_rect, self.pan, self.zoom);

        // 2. Drop shadows and solid pure-white page backings
        self.draw_page_backings(ui.painter(), viewport_rect, layouts, scenes);

        // 3. Unified viewport texture covering workspace. In thumbnail mode there is no
        //    such texture: each page paints its own inside the loop below.
        if let PagePixels::Viewport(Some(tid)) = pixels {
            ui.painter().image(
                *tid,
                viewport_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }

        let mut new_visible = Vec::new();
        let origin = self.get_origin(viewport_rect);
        let active_spread = self.get_spread_indices(self.active_page, layouts.len());

        for layout in layouts {
            if self.display_mode == DisplayMode::SinglePage && layout.index != self.active_page {
                continue;
            }
            if self.display_mode == DisplayMode::TwoPageSingle
                && !active_spread.contains(&layout.index)
            {
                continue;
            }
            let page_rect = egui::Rect::from_min_size(
                origin + layout.rect.min.to_vec2() * self.zoom,
                layout.rect.size() * self.zoom,
            );

            if viewport_rect.intersects(page_rect) {
                new_visible.push(layout.index);
                let is_selected = selected_pages.contains(&layout.index);

                // A thumbnail is this page's whole appearance, so it is painted before the
                // placeholder decides whether anything is missing.
                let thumbnail = match pixels {
                    PagePixels::Thumbnails(map) => map.get(&layout.index).copied(),
                    PagePixels::Viewport(_) => None,
                };
                if let Some(tid) = thumbnail {
                    ui.painter().image(
                        tid,
                        page_rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else if thumbnail.is_none() && !scenes.contains_key(&layout.index) {
                    Self::draw_placeholder_card(ui.painter(), page_rect, layout.index);
                } else if matches!(pixels, PagePixels::Thumbnails(_)) {
                    // The scene is ready but its thumbnail is not yet: this frame made its
                    // quota. Say so rather than showing a blank page backing.
                    Self::draw_placeholder_card(ui.painter(), page_rect, layout.index);
                }

                // Page selection border
                if is_selected {
                    ui.painter().rect_stroke(
                        page_rect,
                        3.0,
                        egui::Stroke::new(2.5_f32, crate::app::theme::colors::RUST_PRIMARY),
                        egui::StrokeKind::Outside,
                    );
                } else if !self.is_reading_view() {
                    ui.painter().rect_stroke(
                        page_rect,
                        3.0,
                        egui::Stroke::new(1.0_f32, crate::app::theme::colors::STEEL_BORDER),
                        egui::StrokeKind::Outside,
                    );
                }

                // Page number badge
                Self::draw_page_number_badge(ui, page_rect, layout.index, is_selected, self.zoom);

                // Overlays
                self.draw_selection_highlights(ui, layout.index, highlights);
                self.draw_redaction_highlights(ui, layout.index, redaction_highlights);
                self.draw_active_redaction_drag(ui, layout.index, active_redaction_drag);
                self.draw_structural_highlight(ui, layout.index, structural_highlight);
                self.draw_signature_highlight(ui, layout.index, signature_highlight);

                if show_reading_order && let Some(ref root) = ust_registry.root {
                    Self::draw_semantic_borders(
                        ui,
                        page_rect,
                        self.zoom,
                        layout.rect.height(),
                        root,
                        ust_registry.selected_node_id,
                    );
                    self.draw_reading_order_bar(ui, page_rect, root);
                }
            }
        }

        Self::draw_marquee_overlay(ui.painter(), marquee_rect);

        self.visible_pages = new_visible;
        if response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }
    }

    fn draw_canvas_grid(
        painter: &egui::Painter,
        viewport_rect: egui::Rect,
        pan: egui::Vec2,
        zoom: f32,
    ) {
        let grid_size = 32.0;
        let step = grid_size * zoom;
        let grid_stroke =
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgba_unmultiplied(203, 213, 225, 40));

        if step > 0.1 {
            let start_x = viewport_rect.min.x + (pan.x % step);
            let width = viewport_rect.max.x - start_x;
            if width > 0.0 {
                let count = (width / step).ceil() as usize;
                for i in 0..count {
                    let x = (i as f32).mul_add(step, start_x);
                    painter.line_segment(
                        [egui::pos2(x, viewport_rect.min.y), egui::pos2(x, viewport_rect.max.y)],
                        grid_stroke,
                    );
                }
            }

            let start_y = viewport_rect.min.y + (pan.y % step);
            let height = viewport_rect.max.y - start_y;
            if height > 0.0 {
                let count = (height / step).ceil() as usize;
                for i in 0..count {
                    let y = (i as f32).mul_add(step, start_y);
                    painter.line_segment(
                        [egui::pos2(viewport_rect.min.x, y), egui::pos2(viewport_rect.max.x, y)],
                        grid_stroke,
                    );
                }
            }
        }
    }

    fn draw_page_backings(
        &self,
        painter: &egui::Painter,
        viewport_rect: egui::Rect,
        layouts: &[PageLayout],
        scenes: &std::collections::BTreeMap<usize, std::sync::Arc<vello::Scene>>,
    ) {
        let origin = self.get_origin(viewport_rect);
        let active_spread = self.get_spread_indices(self.active_page, layouts.len());
        for layout in layouts {
            if self.display_mode == DisplayMode::SinglePage && layout.index != self.active_page {
                continue;
            }
            if self.display_mode == DisplayMode::TwoPageSingle
                && !active_spread.contains(&layout.index)
            {
                continue;
            }
            let page_rect = egui::Rect::from_min_size(
                origin + layout.rect.min.to_vec2() * self.zoom,
                layout.rect.size() * self.zoom,
            );
            if viewport_rect.intersects(page_rect) {
                if scenes.contains_key(&layout.index) {
                    for offset in 1..=4 {
                        painter.rect_filled(
                            page_rect.translate(egui::vec2(
                                f32::from(offset) * 1.5,
                                f32::from(offset) * 1.5,
                            )),
                            4.0,
                            egui::Color32::from_rgba_unmultiplied(30, 41, 59, 20 - offset * 4),
                        );
                    }
                }
                painter.rect_filled(page_rect, 0.0, egui::Color32::WHITE);
            }
        }
    }

    fn draw_placeholder_card(painter: &egui::Painter, page_rect: egui::Rect, page_index: usize) {
        painter.rect_filled(page_rect, 4.0, egui::Color32::WHITE);
        painter.rect_stroke(
            page_rect,
            4.0,
            egui::Stroke::new(1.0_f32, crate::app::theme::colors::STEEL_BORDER_SUBTLE),
            egui::StrokeKind::Inside,
        );
        painter.text(
            page_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("⌛ Rendering Page {}...", page_index + 1),
            egui::FontId::proportional(15.0),
            crate::app::theme::colors::STEEL_SECONDARY,
        );
    }

    fn draw_marquee_overlay(painter: &egui::Painter, marquee_rect: Option<egui::Rect>) {
        if let Some(m_rect) = marquee_rect {
            painter.rect_filled(m_rect, 0.0, crate::app::theme::colors::RUST_SELECTION_BG);
            painter.rect_stroke(
                m_rect,
                0.0,
                egui::Stroke::new(1.5_f32, crate::app::theme::colors::RUST_PRIMARY),
                egui::StrokeKind::Outside,
            );
        }
    }

    fn draw_page_number_badge(
        ui: &mut egui::Ui,
        page_rect: egui::Rect,
        page_index: usize,
        is_selected: bool,
        zoom: f32,
    ) {
        let badge_text = format!("{}", page_index + 1);
        let font_size = if zoom < Self::OVERVIEW_ZOOM { 11.0 } else { 12.0 };
        let (badge_bg, badge_fg, badge_border) = if is_selected {
            (
                crate::app::theme::colors::RUST_BADGE_BG,
                crate::app::theme::colors::RUST_BADGE_TEXT,
                crate::app::theme::colors::RUST_PRIMARY,
            )
        } else {
            (
                crate::app::theme::colors::PANEL_BG,
                crate::app::theme::colors::STEEL_SECONDARY,
                crate::app::theme::colors::STEEL_BORDER,
            )
        };

        let galley = ui.painter().layout_no_wrap(
            badge_text,
            egui::FontId::proportional(font_size),
            badge_fg,
        );
        let badge_w = (galley.size().x + 14.0).max(22.0);
        let badge_h = (galley.size().y + 4.0).max(18.0);
        let badge_center_y = page_rect.max.y + (badge_h / 2.0) + 6.0;
        let badge_rect = egui::Rect::from_center_size(
            egui::pos2(page_rect.center().x, badge_center_y),
            egui::vec2(badge_w, badge_h),
        );

        ui.painter().rect_filled(badge_rect, 4.0, badge_bg);
        ui.painter().rect_stroke(
            badge_rect,
            4.0,
            egui::Stroke::new(1.0_f32, badge_border),
            egui::StrokeKind::Outside,
        );
        ui.painter().galley(
            egui::pos2(
                badge_rect.center().x - galley.size().x / 2.0,
                badge_rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            badge_fg,
        );
    }

    fn draw_selection_highlights(
        &self,
        ui: &mut egui::Ui,
        page_index: usize,
        highlights: &BTreeMap<usize, Vec<egui::Rect>>,
    ) {
        if let Some(hl_rects) = highlights.get(&page_index) {
            for hl_rect in hl_rects {
                ui.painter().rect_filled(
                    *hl_rect,
                    0.0,
                    crate::app::theme::colors::RUST_SELECTION_BG,
                );
            }
        }
    }

    fn draw_redaction_highlights(
        &self,
        ui: &mut egui::Ui,
        page_index: usize,
        redaction_highlights: &BTreeMap<usize, Vec<egui::Rect>>,
    ) {
        if let Some(redact_rects) = redaction_highlights.get(&page_index) {
            for redact_rect in redact_rects {
                ui.painter().rect_filled(*redact_rect, 0.0, egui::Color32::BLACK);
                if redact_rect.width() > 60.0 && redact_rect.height() > 12.0 {
                    ui.painter().text(
                        redact_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "[REDACTED]",
                        egui::FontId::monospace(9.0),
                        egui::Color32::from_rgb(255, 75, 75),
                    );
                }
            }
        }
    }

    fn draw_active_redaction_drag(
        &self,
        ui: &mut egui::Ui,
        page_index: usize,
        active_redaction_drag: &Option<(usize, egui::Rect)>,
    ) {
        if let Some((active_page, drag_rect)) = active_redaction_drag
            && *active_page == page_index
        {
            ui.painter().rect_filled(
                *drag_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(255, 0, 0, 100),
            );
            ui.painter().rect_stroke(
                *drag_rect,
                0.0,
                egui::Stroke::new(1.5_f32, egui::Color32::RED),
                egui::StrokeKind::Outside,
            );
        }
    }

    fn draw_structural_highlight(
        &self,
        ui: &mut egui::Ui,
        page_index: usize,
        structural_highlight: &Option<(usize, egui::Rect)>,
    ) {
        if let Some((highlight_page, highlight_rect)) = structural_highlight
            && *highlight_page == page_index
        {
            let time = ui.ctx().input(|i| i.time);
            let pulse = (time * 6.0).sin().abs() as f32;
            let outline_color = egui::Color32::from_rgb(255, 165, 0);
            let fill_opacity = 20 + (pulse * 35.0) as u8;
            let stroke_w = 2.0 + pulse * 2.0;

            ui.painter().rect_stroke(
                *highlight_rect,
                0.0,
                egui::Stroke::new(stroke_w, outline_color),
                egui::StrokeKind::Outside,
            );
            ui.painter().rect_filled(
                *highlight_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(255, 165, 0, fill_opacity),
            );
            ui.ctx().request_repaint();
        }
    }

    fn draw_signature_highlight(
        &self,
        ui: &mut egui::Ui,
        page_index: usize,
        signature_highlight: &Option<(usize, egui::Rect)>,
    ) {
        if let Some((sig_page, sig_rect)) = signature_highlight
            && *sig_page == page_index
        {
            ui.painter().rect_filled(
                *sig_rect,
                4.0,
                egui::Color32::from_rgba_unmultiplied(226, 135, 67, 30),
            );
            ui.painter().rect_stroke(
                *sig_rect,
                4.0,
                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(226, 135, 67)),
                egui::StrokeKind::Outside,
            );
            ui.painter().line_segment(
                [sig_rect.left_top(), sig_rect.right_bottom()],
                egui::Stroke::new(
                    1.0_f32,
                    egui::Color32::from_rgba_unmultiplied(226, 135, 67, 100),
                ),
            );
            ui.painter().line_segment(
                [sig_rect.right_top(), sig_rect.left_bottom()],
                egui::Stroke::new(
                    1.0_f32,
                    egui::Color32::from_rgba_unmultiplied(226, 135, 67, 100),
                ),
            );
            ui.painter().text(
                sig_rect.center(),
                egui::Align2::CENTER_CENTER,
                "🔏 [ DIGITAL SIGNATURE FIELD ]",
                egui::FontId::monospace(12.0),
                egui::Color32::from_rgb(226, 135, 67),
            );
        }
    }

    fn handle_zoom_gestures(
        &mut self,
        ui: &egui::Ui,
        viewport_rect: egui::Rect,
        layouts: &[PageLayout],
    ) {
        ui.input(|i| {
            let cursor_pos = i
                .pointer
                .hover_pos()
                .or(i.pointer.latest_pos())
                .filter(|p| viewport_rect.contains(*p))
                .unwrap_or_else(|| viewport_rect.center());

            let zoom_delta = i.zoom_delta();
            #[allow(clippy::float_cmp)]
            if zoom_delta != 1.0 {
                // From the unsnapped value, so the gesture can travel through a step's
                // snap band rather than being pulled back onto it every frame.
                let target = self.zoom_before_snapping() * zoom_delta;
                self.zoom_at(target, cursor_pos, viewport_rect, layouts);
            }

            let is_zoom_modifier = i.modifiers.command || i.modifiers.ctrl;
            let scroll_y = i.smooth_scroll_delta.y;

            if is_zoom_modifier && scroll_y != 0.0 {
                let factor = (scroll_y * 0.005).exp();
                let target = self.zoom_before_snapping() * factor;
                self.zoom_at(target, cursor_pos, viewport_rect, layouts);
            }
        });
    }

    fn handle_scroll_panning(&mut self, ui: &egui::Ui) {
        ui.input(|i| {
            if !i.modifiers.command && !i.modifiers.ctrl {
                let scroll_delta = i.smooth_scroll_delta;
                if self.scroll_direction == ScrollDirection::Horizontal {
                    if scroll_delta.x != 0.0 {
                        self.pan.x += scroll_delta.x;
                    } else {
                        self.pan.x += scroll_delta.y;
                    }
                } else {
                    self.pan += scroll_delta;
                }
            }
        });
    }

    fn handle_input(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        viewport_rect: egui::Rect,
        layouts: &[PageLayout],
    ) {
        let is_hovered = ui.ctx().input(|i| {
            i.pointer
                .hover_pos()
                .or(i.pointer.latest_pos())
                .is_some_and(|pos| viewport_rect.contains(pos))
        });
        if is_hovered {
            self.handle_zoom_gestures(ui, viewport_rect, layouts);
            self.handle_scroll_panning(ui);
        }
        let shift_down = ui.input(|i| i.modifiers.shift);
        if response.dragged() && (!shift_down || self.is_reading_view()) {
            self.pan += response.drag_delta();
        }
        if response.double_clicked() {
            let pos = ui
                .input(|i| i.pointer.hover_pos().or(i.pointer.latest_pos()))
                .filter(|p| viewport_rect.contains(*p))
                .unwrap_or_else(|| viewport_rect.center());
            let target_zoom = if self.is_reading_view() { Self::OVERVIEW_STEP } else { 1.0 };
            self.zoom_at(target_zoom, pos, viewport_rect, layouts);
        }
        if response.drag_stopped()
            || (!response.dragged() && ui.input(|i| i.pointer.any_released()))
        {
            self.overscroll_accumulator = egui::Vec2::ZERO;
        }
    }

    fn draw_semantic_borders(
        ui: &mut egui::Ui,
        page_rect: egui::Rect,
        zoom: f32,
        unscaled_h: f32,
        node: &crate::sidebar::USTNode,
        selected_id: Option<usize>,
    ) {
        if let Some(rect) = node.rect {
            let min_screen = crate::interaction::SelectionManager::pdf_to_screen(
                page_rect,
                zoom,
                unscaled_h,
                egui::pos2(rect[0], rect[3]),
            );
            let max_screen = crate::interaction::SelectionManager::pdf_to_screen(
                page_rect,
                zoom,
                unscaled_h,
                egui::pos2(rect[2], rect[1]),
            );
            let element_rect = egui::Rect::from_min_max(min_screen, max_screen);

            let color = match node.tag.as_str() {
                "H1" | "H2" | "H3" => egui::Color32::from_rgb(0, 120, 215), // Blue
                "P" => egui::Color32::from_rgb(34, 197, 94),                // Green
                "Figure" => egui::Color32::from_rgb(168, 85, 247),          // Purple
                "Table" => egui::Color32::from_rgb(249, 115, 22),           // Orange
                _ => egui::Color32::from_gray(120),
            };

            let stroke = if Some(node.id) == selected_id {
                egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(240, 165, 0)) // Amber selection
            } else {
                egui::Stroke::new(1.0_f32, color)
            };

            ui.painter().rect_stroke(element_rect, 2.0, stroke, egui::StrokeKind::Outside);
        }

        for child in &node.children {
            Self::draw_semantic_borders(ui, page_rect, zoom, unscaled_h, child, selected_id);
        }
    }

    fn collect_nodes_for_reading_order(
        node: &crate::sidebar::USTNode,
        list: &mut Vec<(String, egui::Color32)>,
    ) {
        if node.rect.is_some() {
            let color = match node.tag.as_str() {
                "H1" | "H2" | "H3" => egui::Color32::from_rgb(0, 120, 215), // Blue
                "P" => egui::Color32::from_rgb(34, 197, 94),                // Green
                "Figure" => egui::Color32::from_rgb(168, 85, 247),          // Purple
                "Table" => egui::Color32::from_rgb(249, 115, 22),           // Orange
                _ => egui::Color32::from_gray(120),
            };
            list.push((node.tag.clone(), color));
        }
        for child in &node.children {
            Self::collect_nodes_for_reading_order(child, list);
        }
    }

    fn draw_reading_order_bar(
        &self,
        ui: &mut egui::Ui,
        page_rect: egui::Rect,
        root_node: &crate::sidebar::USTNode,
    ) {
        let mut list = Vec::new();
        Self::collect_nodes_for_reading_order(root_node, &mut list);

        if list.is_empty() {
            return;
        }

        let bar_height = 24.0;
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(page_rect.left(), page_rect.bottom() + 8.0),
            egui::vec2(page_rect.width(), bar_height),
        );

        ui.painter().rect_filled(bar_rect, 4.0, crate::app::theme::colors::STEEL_PRIMARY);

        let mut x_offset = bar_rect.left() + 4.0;
        for (i, (tag, color)) in list.iter().enumerate() {
            let label = format!("{}: {}", i + 1, tag);
            let text_gal = ui.painter().layout_no_wrap(
                label.clone(),
                egui::FontId::proportional(10.0),
                egui::Color32::WHITE,
            );
            let block_width = text_gal.size().x + 12.0;

            if x_offset + block_width > bar_rect.right() - 4.0 {
                break;
            }

            let block_rect = egui::Rect::from_min_size(
                egui::pos2(x_offset, bar_rect.top() + 3.0),
                egui::vec2(block_width, bar_height - 6.0),
            );

            ui.painter().rect_filled(block_rect, 2.0, *color);
            ui.painter().text(
                block_rect.center(),
                egui::Align2::CENTER_CENTER,
                &label,
                egui::FontId::proportional(10.0),
                egui::Color32::WHITE,
            );

            x_offset += block_width + 6.0;
        }
    }

    pub fn clamp_pan(&mut self, viewport_rect: egui::Rect, layouts: &[PageLayout]) {
        // RR-15 Limit: GUI
        if layouts.is_empty() {
            return;
        }

        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;

        // If SinglePage or TwoPageSingle, only clamp using target layouts
        let target_layouts: Vec<&PageLayout> = if self.display_mode == DisplayMode::SinglePage {
            if let Some(layout) = layouts.get(self.active_page) {
                vec![layout]
            } else {
                layouts.iter().collect()
            }
        } else if self.display_mode == DisplayMode::TwoPageSingle {
            let spread_indices = self.get_spread_indices(self.active_page, layouts.len());
            spread_indices.iter().filter_map(|&idx| layouts.get(idx)).collect()
        } else {
            layouts.iter().collect()
        };

        for layout in &target_layouts {
            min_x = min_x.min(layout.rect.min.x);
            max_x = max_x.max(layout.rect.max.x);
            min_y = min_y.min(layout.rect.min.y);
            max_y = max_y.max(layout.rect.max.y);
        }

        let origin_no_pan = self.get_origin_no_pan(viewport_rect);
        let min_overlap = 50.0f32;

        let min_pan_x =
            max_x.mul_add(-self.zoom, viewport_rect.min.x + min_overlap - origin_no_pan.x);
        let max_pan_x =
            min_x.mul_add(-self.zoom, viewport_rect.max.x - min_overlap - origin_no_pan.x);
        let clamped_x = self.pan.x.clamp(min_pan_x, max_pan_x);

        let min_pan_y =
            max_y.mul_add(-self.zoom, viewport_rect.min.y + min_overlap - origin_no_pan.y);
        let max_pan_y =
            min_y.mul_add(-self.zoom, viewport_rect.max.y - min_overlap - origin_no_pan.y);
        let clamped_y = self.pan.y.clamp(min_pan_y, max_pan_y);

        if self.display_mode == DisplayMode::SinglePage
            || self.display_mode == DisplayMode::TwoPageSingle
        {
            let threshold = 80.0; // Pull past edge distance threshold

            if self.scroll_direction == ScrollDirection::Vertical {
                let diff_y = self.pan.y - clamped_y;
                if diff_y.abs() > 0.0 {
                    self.overscroll_accumulator.y += diff_y;
                } else {
                    self.overscroll_accumulator.y = 0.0;
                }

                if self.overscroll_accumulator.y.abs() > threshold {
                    let total_pages = layouts.len();
                    if self.overscroll_accumulator.y < 0.0 {
                        // Pulled up / past bottom -> next page/spread
                        if self.display_mode == DisplayMode::TwoPageSingle {
                            let spread = self.get_spread_indices(self.active_page, total_pages);
                            if let Some(&last_idx) = spread.last()
                                && last_idx + 1 < total_pages
                            {
                                let next = last_idx + 1;
                                self.scroll_to_page(next, layouts);
                                self.overscroll_accumulator = egui::Vec2::ZERO;
                                return;
                            }
                        } else if self.active_page + 1 < total_pages {
                            let next = self.active_page + 1;
                            self.scroll_to_page(next, layouts);
                            self.overscroll_accumulator = egui::Vec2::ZERO;
                            return;
                        }
                    } else {
                        // Pulled down / past top -> prev page/spread
                        if self.display_mode == DisplayMode::TwoPageSingle {
                            let spread = self.get_spread_indices(self.active_page, total_pages);
                            if let Some(&first_idx) = spread.first()
                                && first_idx > 0
                            {
                                let prev = first_idx - 1;
                                self.scroll_to_page(prev, layouts);
                                self.overscroll_accumulator = egui::Vec2::ZERO;
                                return;
                            }
                        } else if self.active_page > 0 {
                            let prev = self.active_page - 1;
                            self.scroll_to_page(prev, layouts);
                            self.overscroll_accumulator = egui::Vec2::ZERO;
                            return;
                        }
                    }
                }
            } else {
                let diff_x = self.pan.x - clamped_x;
                if diff_x.abs() > 0.0 {
                    self.overscroll_accumulator.x += diff_x;
                } else {
                    self.overscroll_accumulator.x = 0.0;
                }

                if self.overscroll_accumulator.x.abs() > threshold {
                    let total_pages = layouts.len();
                    let is_r2l = self.binding_direction == BindingDirection::RightToLeft;

                    if (self.overscroll_accumulator.x < 0.0 && !is_r2l)
                        || (self.overscroll_accumulator.x > 0.0 && is_r2l)
                    {
                        // Go to next page/spread
                        if self.display_mode == DisplayMode::TwoPageSingle {
                            let spread = self.get_spread_indices(self.active_page, total_pages);
                            if let Some(&last_idx) = spread.last()
                                && last_idx + 1 < total_pages
                            {
                                let next = last_idx + 1;
                                self.scroll_to_page(next, layouts);
                                self.overscroll_accumulator = egui::Vec2::ZERO;
                                return;
                            }
                        } else if self.active_page + 1 < total_pages {
                            let next = self.active_page + 1;
                            self.scroll_to_page(next, layouts);
                            self.overscroll_accumulator = egui::Vec2::ZERO;
                            return;
                        }
                    } else {
                        // Go to prev page/spread
                        if self.display_mode == DisplayMode::TwoPageSingle {
                            let spread = self.get_spread_indices(self.active_page, total_pages);
                            if let Some(&first_idx) = spread.first()
                                && first_idx > 0
                            {
                                let prev = first_idx - 1;
                                self.scroll_to_page(prev, layouts);
                                self.overscroll_accumulator = egui::Vec2::ZERO;
                                return;
                            }
                        } else if self.active_page > 0 {
                            let prev = self.active_page - 1;
                            self.scroll_to_page(prev, layouts);
                            self.overscroll_accumulator = egui::Vec2::ZERO;
                            return;
                        }
                    }
                }
            }
        }

        self.pan.x = clamped_x;
        self.pan.y = clamped_y;
    }
}

#[cfg(test)]
mod zoom_steps {
    use super::PDFView;

    /// The defect the ladder replaces: from any zoom a pinch or a fit had produced,
    /// multiplying by 1.2 never arrived at 100%. Stepping does, from either side.
    #[test]
    fn stepping_reaches_one_hundred_percent_from_anywhere() {
        for start in [0.11_f32, 0.37, 0.83, 0.96, 1.04, 2.7, 9.4] {
            let mut view = PDFView::new();
            view.set_zoom(start);
            let up = start < 1.0;
            for _ in 0..20 {
                let next = if up { view.zoom_step_up() } else { view.zoom_step_down() };
                view.set_zoom(next);
                if (view.zoom() - 1.0).abs() < f32::EPSILON {
                    break;
                }
            }
            assert!(
                (view.zoom() - 1.0).abs() < f32::EPSILON,
                "from {start} the ladder stopped at {}",
                view.zoom()
            );
        }
    }

    /// Stepping moves, and moves the right way. Without this a `find` that matched the
    /// current value would leave the buttons dead.
    #[test]
    fn a_step_always_moves_and_stops_at_the_bounds() {
        let mut view = PDFView::new();
        view.set_zoom(1.0);
        assert!(view.zoom_step_up() > 1.0);
        assert!(view.zoom_step_down() < 1.0);

        view.set_zoom(10.0);
        assert!((view.zoom_step_up() - 10.0).abs() < f32::EPSILON, "held at the ceiling");
        view.set_zoom(0.1);
        assert!((view.zoom_step_down() - 0.1).abs() < f32::EPSILON, "held at the floor");
    }

    /// The mode boundary sits between two steps, so no step lands on it and leaves the
    /// mode ambiguous. 33% is an overview, 50% is legible, and nothing is exactly 40%.
    #[test]
    fn no_step_lands_on_a_mode_boundary() {
        let mut view = PDFView::new();
        for step in PDFView::ZOOM_STEPS {
            view.set_zoom(step);
            assert!(
                (step - PDFView::LEGIBLE_ZOOM).abs() > 0.01,
                "step {step} sits on the legibility boundary"
            );
            assert!(
                (step - PDFView::OVERVIEW_ZOOM).abs() > 0.01,
                "step {step} sits on the overview boundary"
            );
        }
        view.set_zoom(PDFView::OVERVIEW_STEP);
        assert!(!view.is_legible() && !view.is_reading_view(), "33% is an overview");
        view.set_zoom(0.50);
        assert!(view.is_legible() && !view.is_reading_view(), "50% reads but still tiles");
        view.set_zoom(0.67);
        assert!(view.is_legible() && view.is_reading_view(), "67% is a reading view");
    }

    /// A gesture that lands within the snap band is taken to mean the step, so the label
    /// and the rendering agree. 0.996 used to print as "100%" while rendering otherwise.
    #[test]
    fn a_gesture_near_a_step_is_taken_to_mean_it() {
        let layouts = [];
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let mut view = PDFView::new();
        view.set_zoom(0.90);
        view.zoom_at(0.996, rect.center(), rect, &layouts);
        assert!((view.zoom() - 1.0).abs() < f32::EPSILON, "snapped: {}", view.zoom());
    }

    /// **A pinch must be able to leave a step.** Snapping used to compute the next value
    /// from the snapped one, so small deltas landed back inside the band and the gesture
    /// stuck at 100% for good.
    #[test]
    fn a_gesture_can_travel_through_a_step() {
        let layouts = [];
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let mut view = PDFView::new();
        view.set_zoom(0.98);
        for _ in 0..40 {
            let target = view.zoom_before_snapping() * 1.004;
            view.zoom_at(target, rect.center(), rect, &layouts);
        }
        assert!(view.zoom() > 1.02, "the gesture stuck at {}", view.zoom());
    }
}
