//! Viewport panel and canvas rendering for `FepdfApp`.

use super::FepdfApp;
use crate::interaction::SelectionManager;
use crate::view::DisplayMode;
use crate::worker::WorkerRequest;
use std::collections::BTreeMap;
use std::sync::Arc;

impl FepdfApp {
    pub(crate) fn check_gpu_support(&self, ui: &mut egui::Ui, frame: &mut eframe::Frame) -> bool {
        let has_wgpu = frame.wgpu_render_state().is_some();
        if !has_wgpu {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(
                        egui::Color32::RED,
                        "WGPU RenderState not available. GPU compute acceleration is disabled.",
                    );
                });
            });
            return false;
        }
        true
    }

    pub(crate) fn queue_visible_pages(&mut self) {
        // Collect visible pages and calculate pre-render lookahead indices
        let mut render_targets = std::collections::BTreeSet::new();
        if self.view.display_mode == DisplayMode::SinglePage {
            let active = self.view.active_page;
            render_targets.insert(active);
            if active > 0 {
                render_targets.insert(active - 1);
            }
            if active + 1 < self.total_pages {
                render_targets.insert(active + 1);
            }
        } else if self.view.display_mode == DisplayMode::TwoPageSingle {
            let spread_indices =
                self.view.get_spread_indices(self.view.active_page, self.total_pages);
            for &idx in &spread_indices {
                render_targets.insert(idx);
            }
            // Pre-render pages before and after the spread
            if let Some(&first_idx) = spread_indices.first()
                && first_idx > 0
            {
                render_targets.insert(first_idx - 1);
            }
            if let Some(&last_idx) = spread_indices.last()
                && last_idx + 1 < self.total_pages
            {
                render_targets.insert(last_idx + 1);
            }
        } else {
            for &visible_index in &self.view.visible_pages {
                render_targets.insert(visible_index);

                // Lookahead pre-rendering: queue previous page and next page in the background
                if visible_index > 0 {
                    render_targets.insert(visible_index - 1);
                }
                if visible_index + 1 < self.total_pages {
                    render_targets.insert(visible_index + 1);
                }
            }
        }

        // Queue rendering requests to the worker thread
        for index in render_targets {
            if !self.scenes.contains_key(&index) && !self.request_queue.contains(&index) {
                let scale = 2.0;
                self.request_queue.insert(index);
                let _ = self.tx_worker.send(WorkerRequest::RenderPage { index, scale });
            }
        }
    }

    fn handle_signature_placement_interaction(
        &mut self,
        ui: &mut egui::Ui,
        visible_index: usize,
        page_screen_rect: egui::Rect,
        unscaled_h: f32,
        zoom: f32,
    ) {
        let response = ui.allocate_rect(page_screen_rect, egui::Sense::drag());
        let screen_pos = ui.input(|i| i.pointer.hover_pos());

        if response.drag_started()
            && let Some(pos) = screen_pos
        {
            let pdf_pos = SelectionManager::screen_to_pdf(page_screen_rect, zoom, unscaled_h, pos);
            self.signature_position =
                Some((visible_index, egui::Rect::from_min_max(pdf_pos, pdf_pos)));
        }

        if response.dragged()
            && let Some(pos) = screen_pos
            && let Some((sig_idx, sig_rect)) = &mut self.signature_position
            && *sig_idx == visible_index
        {
            let pdf_pos = SelectionManager::screen_to_pdf(page_screen_rect, zoom, unscaled_h, pos);
            let start_pos = sig_rect.min;
            *sig_rect = egui::Rect::from_two_pos(start_pos, pdf_pos);
        }

        if response.drag_stopped() {
            self.is_placing_signature = false;
        }

        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }

    fn render_page_context_menu(&mut self, response: &egui::Response, page_idx: usize) {
        response.context_menu(|ui| {
            ui.label(format!("Page {}", page_idx + 1));
            ui.separator();
            if ui.button("↷ 右に90°回転").clicked() {
                self.rotate_page_action(page_idx, fepdf::Quarter::Q90);
                ui.close();
            }
            if ui.button("↶ 左に90°回転").clicked() {
                self.rotate_page_action(page_idx, fepdf::Quarter::Q270);
                ui.close();
            }
            if ui.button("🔄 180°回転").clicked() {
                self.rotate_page_action(page_idx, fepdf::Quarter::Q180);
                ui.close();
            }
            ui.separator();
            if ui.button("📑 ページを複製").clicked() {
                self.duplicate_page(page_idx);
                ui.close();
            }
            if self.total_pages > 1 {
                ui.separator();
                if ui.button("🗑 ページを削除").clicked() {
                    self.selected_pages.clear();
                    self.selected_pages.insert(page_idx);
                    self.remove_selected_pages();
                    ui.close();
                }
            }
        });
    }

    fn handle_page_click_selection(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        page_idx: usize,
    ) {
        if response.clicked() {
            let shift = ui.input(|ins| ins.modifiers.shift);
            let cmd = ui.input(|ins| ins.modifiers.command || ins.modifiers.ctrl);

            if shift {
                if let Some(start) = self.last_selected_page {
                    self.selected_pages.clear();
                    let min = start.min(page_idx);
                    let max = start.max(page_idx);
                    for p in min..=max {
                        self.selected_pages.insert(p);
                    }
                } else {
                    self.selected_pages.clear();
                    self.selected_pages.insert(page_idx);
                    self.last_selected_page = Some(page_idx);
                }
            } else if cmd {
                if self.selected_pages.contains(&page_idx) {
                    self.selected_pages.remove(&page_idx);
                } else {
                    self.selected_pages.insert(page_idx);
                }
                self.last_selected_page = Some(page_idx);
            } else {
                self.selected_pages.clear();
                self.selected_pages.insert(page_idx);
                self.last_selected_page = Some(page_idx);
                self.view.active_page = page_idx;
            }
        }
    }

    fn handle_tile_drag_and_drop(
        ui: &egui::Ui,
        response: &egui::Response,
        page_idx: usize,
        page_screen_rect: egui::Rect,
        dragged_from: Option<usize>,
        is_r2l: bool,
        zoom: f32,
    ) -> Option<usize> {
        if response.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), page_idx);
        }

        let pointer_pos = ui.input(|i| {
            i.pointer.interact_pos().or(i.pointer.latest_pos()).or(i.pointer.hover_pos())
        });

        if let Some(_from_idx) = dragged_from
            && let Some(mouse_pos) = pointer_pos
            && page_screen_rect.expand(4.0).contains(mouse_pos)
        {
            let is_left_half = mouse_pos.x < page_screen_rect.center().x;
            let target_slot = if is_r2l {
                if is_left_half { page_idx + 1 } else { page_idx }
            } else if is_left_half {
                page_idx
            } else {
                page_idx + 1
            };

            // Inter-page horizontal gap center (vertical line between pages)
            let gap_offset = (12.0 * zoom).clamp(4.0, 12.0);
            let indicator_x = if is_left_half {
                page_screen_rect.min.x - gap_offset
            } else {
                page_screen_rect.max.x + gap_offset
            };

            let indicator_color = crate::app::theme::colors::RUST_PRIMARY;
            let y_top = page_screen_rect.min.y - 4.0;
            let y_bottom = page_screen_rect.max.y + 4.0;

            ui.painter().line_segment(
                [egui::pos2(indicator_x, y_top), egui::pos2(indicator_x, y_bottom)],
                egui::Stroke::new(3.5_f32, indicator_color),
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            return Some(target_slot);
        }
        None
    }

    /// Text selection on a page, once it is large enough to select on.
    ///
    /// Split from `handle_page_tile_interaction`, which was doing three things: click
    /// selection, the drag-and-drop slot, and this.
    ///
    /// Below `PDFView::TILE_ZOOM` the tiles are thumbnails being reordered, not pages
    /// being read from.
    fn handle_text_selection_on_page(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        page_idx: usize,
        page_screen_rect: egui::Rect,
        unscaled_h: f32,
        zoom: f32,
    ) {
        if self.view.selects_text()
            && let Some(spans) = self.page_spans.get(&page_idx)
        {
            if self.selection_manager.is_tagging_brush_active {
                self.selection_manager.handle_tagging_brush_interaction(
                    ui,
                    page_idx,
                    page_screen_rect,
                    unscaled_h,
                    spans,
                    zoom,
                );
            } else {
                self.selection_manager.handle_drag(
                    ui,
                    response,
                    page_idx,
                    page_screen_rect,
                    unscaled_h,
                    spans,
                    zoom,
                );
            }
        }
    }

    fn handle_page_tile_interaction(
        &mut self,
        ui: &mut egui::Ui,
        page_idx: usize,
        page_screen_rect: egui::Rect,
        unscaled_h: f32,
        zoom: f32,
        dragged_from: Option<usize>,
    ) -> Option<usize> {
        let response = ui.allocate_rect(page_screen_rect, egui::Sense::click_and_drag());
        // The context menu stays in both views: a right-click is not what text selection
        // reads, and rotating the page being read is a reasonable thing to want.
        self.render_page_context_menu(&response, page_idx);

        // Selecting *the page* is the tile view's click. In the page view the same
        // `Response` belongs to text selection, and having both meant a drag over a
        // sentence also selected the page under it — which `Delete`, gated by nothing,
        // would then remove.
        if self.view.selects_pages() {
            self.handle_page_click_selection(ui, &response, page_idx);

            if response.drag_started() && !self.selected_pages.contains(&page_idx) {
                self.selected_pages.clear();
                self.selected_pages.insert(page_idx);
                self.last_selected_page = Some(page_idx);
            }
        }

        // Double-clicking a tile opens that page. **The layout has to be rebuilt between
        // the two.** `scroll_to_page` places the view from the rectangle the page has in
        // the layout it is handed, and the layout in hand is the grid the double-click
        // just left: page 25 is at `2 * 954` there and at `25 * 890` in a column, so
        // scrolling with the grid's rectangle lands near the front of the document.
        if response.double_clicked() && zoom < crate::view::PDFView::TILE_ZOOM {
            self.view.set_zoom(1.0);
            self.compute_layouts();
            self.view.scroll_to_page(page_idx, &self.page_layouts);
        }

        let is_r2l = self.view.binding_direction == crate::view::BindingDirection::RightToLeft;
        let target_slot = if zoom < crate::view::PDFView::TILE_ZOOM {
            Self::handle_tile_drag_and_drop(
                ui,
                &response,
                page_idx,
                page_screen_rect,
                dragged_from,
                is_r2l,
                zoom,
            )
        } else {
            None
        };

        self.handle_text_selection_on_page(
            ui,
            &response,
            page_idx,
            page_screen_rect,
            unscaled_h,
            zoom,
        );

        target_slot
    }

    fn handle_caliper_page_interaction(
        &mut self,
        ui: &mut egui::Ui,
        page_idx: usize,
        page_screen_rect: egui::Rect,
        unscaled_h: f32,
        zoom: f32,
    ) {
        if let Some(spans) = self.page_spans.get(&page_idx) {
            self.caliper_tool.handle_interaction(
                ui,
                page_idx,
                page_screen_rect,
                unscaled_h,
                zoom,
                &mut self.cad_snap_engine,
                spans,
            );
            self.caliper_tool.draw_overlay(ui, page_screen_rect, unscaled_h, zoom);
        }
    }

    fn handle_single_page_interaction(
        &mut self,
        ui: &mut egui::Ui,
        visible_index: usize,
        page_screen_rect: egui::Rect,
        unscaled_h: f32,
        zoom: f32,
        dragged_from: Option<usize>,
    ) -> Option<usize> {
        let tool_active = self.is_placing_signature
            || self.caliper_tool.is_active
            || self.redaction_manager.is_active;

        match page_input(tool_active, self.view.is_page_view()) {
            // A content tool is out where the tool cannot see what it is acting on, and
            // its drags must not become something else on the way past.
            PageInput::Suppressed => None,
            PageInput::Tool => {
                self.handle_content_tool(ui, visible_index, page_screen_rect, unscaled_h, zoom);
                None
            }
            PageInput::Normal => self.handle_page_tile_interaction(
                ui,
                visible_index,
                page_screen_rect,
                unscaled_h,
                zoom,
                dragged_from,
            ),
        }
    }

    /// Hands the page to whichever content tool is active. Only reached in the page view.
    fn handle_content_tool(
        &mut self,
        ui: &mut egui::Ui,
        visible_index: usize,
        page_screen_rect: egui::Rect,
        unscaled_h: f32,
        zoom: f32,
    ) {
        if self.is_placing_signature {
            self.handle_signature_placement_interaction(
                ui,
                visible_index,
                page_screen_rect,
                unscaled_h,
                zoom,
            );
        } else if self.caliper_tool.is_active {
            self.handle_caliper_page_interaction(
                ui,
                visible_index,
                page_screen_rect,
                unscaled_h,
                zoom,
            );
        } else if self.redaction_manager.is_active {
            self.redaction_manager.handle_interaction(
                ui,
                visible_index,
                page_screen_rect,
                unscaled_h,
                zoom,
            );
        }
    }

    /// Applies a page drag once the pointer is released, and clears the payload.
    ///
    /// Split from `handle_page_interactions`, which was walking the visible pages *and*
    /// deciding what a finished drag meant. A drag that ends over no slot still has to
    /// clear its payload, which is why the release and the target are separate conditions.
    fn commit_page_reorder(
        &mut self,
        ui: &egui::Ui,
        dragged_from: Option<usize>,
        reorder_target: Option<usize>,
    ) {
        if let Some(from_idx) = dragged_from
            && ui.input(|i| i.pointer.any_released())
        {
            if let Some(target_insert_pos) = reorder_target {
                let sources = if self.selected_pages.contains(&from_idx) {
                    self.selected_pages.iter().copied().collect()
                } else {
                    vec![from_idx]
                };
                self.reorder_pages_batch(&sources, target_insert_pos);
            }
            egui::DragAndDrop::clear_payload(ui.ctx());
        }
    }

    fn handle_page_interactions(
        &mut self,
        ui: &mut egui::Ui,
        viewport_rect: egui::Rect,
        zoom: f32,
    ) {
        let mut reorder_target = None;
        let dragged_from = egui::DragAndDrop::payload::<usize>(ui.ctx()).map(|p| *p);
        let visible_pages = self.view.visible_pages.clone();
        let active_spread = self.view.get_spread_indices(self.view.active_page, self.total_pages);
        for &visible_index in &visible_pages {
            if (self.view.display_mode == DisplayMode::SinglePage
                && visible_index != self.view.active_page)
                || (self.view.display_mode == DisplayMode::TwoPageSingle
                    && !active_spread.contains(&visible_index))
            {
                continue;
            }
            if let Some(layout) = self.page_layouts.get(visible_index) {
                let origin = self.view.get_origin(viewport_rect);
                let page_screen_rect = egui::Rect::from_min_size(
                    origin + layout.rect.min.to_vec2() * zoom,
                    layout.rect.size() * zoom,
                );
                let unscaled_h = layout.rect.height();

                if let Some(target) = self.handle_single_page_interaction(
                    ui,
                    visible_index,
                    page_screen_rect,
                    unscaled_h,
                    zoom,
                    dragged_from,
                ) {
                    reorder_target = Some(target);
                }
            }
        }

        self.commit_page_reorder(ui, dragged_from, reorder_target);
    }

    fn get_structural_highlight(
        &self,
        viewport_rect: egui::Rect,
        zoom: f32,
    ) -> Option<(usize, egui::Rect)> {
        let selected_id = self.ust_registry.selected_node_id?;
        if let Some(ref root) = self.ust_registry.root
            && root.id == selected_id
        {
            return None;
        }
        let (page_idx, rect) = self.ust_registry.find_placement_by_id(selected_id)?;
        let layout = self.page_layouts.get(page_idx)?;
        let origin = self.view.get_origin(viewport_rect);
        let page_screen_rect = egui::Rect::from_min_size(
            origin + layout.rect.min.to_vec2() * zoom,
            layout.rect.size() * zoom,
        );
        let unscaled_h = layout.rect.height();
        let screen_min = SelectionManager::pdf_to_screen(
            page_screen_rect,
            zoom,
            unscaled_h,
            egui::pos2(rect[0], rect[3]),
        );
        let screen_max = SelectionManager::pdf_to_screen(
            page_screen_rect,
            zoom,
            unscaled_h,
            egui::pos2(rect[2], rect[1]),
        );
        Some((page_idx, egui::Rect::from_min_max(screen_min, screen_max)))
    }

    fn get_signature_highlight(
        &self,
        viewport_rect: egui::Rect,
        zoom: f32,
    ) -> Option<(usize, egui::Rect)> {
        let (sig_idx, sig_rect) = self.signature_position?;
        let layout = self.page_layouts.get(sig_idx)?;
        let origin = self.view.get_origin(viewport_rect);
        let page_screen_rect = egui::Rect::from_min_size(
            origin + layout.rect.min.to_vec2() * zoom,
            layout.rect.size() * zoom,
        );
        let unscaled_h = layout.rect.height();
        let screen_min = SelectionManager::pdf_to_screen(
            page_screen_rect,
            zoom,
            unscaled_h,
            egui::pos2(sig_rect.min.x, sig_rect.max.y),
        );
        let screen_max = SelectionManager::pdf_to_screen(
            page_screen_rect,
            zoom,
            unscaled_h,
            egui::pos2(sig_rect.max.x, sig_rect.min.y),
        );
        Some((sig_idx, egui::Rect::from_min_max(screen_min, screen_max)))
    }

    fn draw_view_with_highlights(
        // RR-15 Limit: GUI - Paints selection, redaction, and structural highlights onto canvas
        &mut self,
        ui: &mut egui::Ui,
        viewport_rect: egui::Rect,
        zoom: f32,
        pixels: &crate::view::PagePixels<'_>,
    ) {
        let origin = self.view.get_origin(viewport_rect);
        let mut redaction_highlights = BTreeMap::new();
        let mut active_redaction_drag = None;

        for &visible_index in &self.view.visible_pages {
            if let Some(layout) = self.page_layouts.get(visible_index) {
                let page_screen_rect = egui::Rect::from_min_size(
                    origin + layout.rect.min.to_vec2() * zoom,
                    layout.rect.size() * zoom,
                );
                let unscaled_h = layout.rect.height();

                let (completed, active_drag) = self.redaction_manager.get_screen_highlights(
                    visible_index,
                    page_screen_rect,
                    unscaled_h,
                    zoom,
                );
                if !completed.is_empty() {
                    redaction_highlights.insert(visible_index, completed);
                }
                if let Some(drag_rect) = active_drag {
                    active_redaction_drag = Some((visible_index, drag_rect));
                }
            }
        }

        let structural_highlight = self.get_structural_highlight(viewport_rect, zoom);
        let signature_highlight = self.get_signature_highlight(viewport_rect, zoom);
        let marquee_rect = self.selection_manager.marquee_rect();

        self.view.show_virtual(
            ui,
            &self.page_layouts,
            pixels,
            viewport_rect,
            &self.scenes,
            &self.selection_manager.highlights,
            &redaction_highlights,
            &active_redaction_drag,
            &structural_highlight,
            &signature_highlight,
            &self.selected_pages,
            &self.ust_registry,
            // Off in the tile view: the borders are drawn per node of every visible page,
            // and the lines are finer than the glyphs they enclose.
            self.show_reading_order && self.view.is_page_view(),
            marquee_rect,
        );
    }

    fn handle_marquee_drag_selection(
        &mut self,
        ui: &egui::Ui,
        viewport_rect: egui::Rect,
        zoom: f32,
    ) {
        let shift_down = ui.input(|i| i.modifiers.shift);
        let mouse_pos = ui.input(|i| i.pointer.hover_pos());
        let any_pressed = ui.input(|i| i.pointer.any_pressed());
        let any_down = ui.input(|i| i.pointer.any_down());
        let any_released = ui.input(|i| i.pointer.any_released());

        if zoom < crate::view::PDFView::TILE_ZOOM && shift_down {
            if any_pressed && let Some(pos) = mouse_pos {
                self.selection_manager.marquee_start = Some(pos);
                self.selection_manager.marquee_current = Some(pos);
            } else if any_down && let Some(pos) = mouse_pos {
                self.selection_manager.marquee_current = Some(pos);
            }
        }
        if any_released || !shift_down {
            if let Some(m_rect) = self.selection_manager.marquee_rect() {
                let origin = self.view.get_origin(viewport_rect);
                for layout in &self.page_layouts {
                    let page_screen_rect = egui::Rect::from_min_size(
                        origin + layout.rect.min.to_vec2() * zoom,
                        layout.rect.size() * zoom,
                    );
                    if m_rect.intersects(page_screen_rect) {
                        self.selected_pages.insert(layout.index);
                    }
                }
            }
            self.selection_manager.marquee_start = None;
            self.selection_manager.marquee_current = None;
        }
    }

    fn collect_visible_pages_data(
        &self,
        viewport_rect: egui::Rect,
        zoom: f32,
    ) -> Vec<(usize, Arc<vello::Scene>, egui::Rect, egui::Vec2)> {
        let mut visible_pages_data = Vec::new();
        let origin = self.view.get_origin(viewport_rect);
        let active_spread = self.view.get_spread_indices(self.view.active_page, self.total_pages);

        for layout in &self.page_layouts {
            if self.view.display_mode == DisplayMode::SinglePage
                && layout.index != self.view.active_page
            {
                continue;
            }
            if self.view.display_mode == DisplayMode::TwoPageSingle
                && !active_spread.contains(&layout.index)
            {
                continue;
            }
            let page_screen_rect = egui::Rect::from_min_size(
                origin + layout.rect.min.to_vec2() * zoom,
                layout.rect.size() * zoom,
            );

            if viewport_rect.intersects(page_screen_rect)
                && let Some(scene) = self.scenes.get(&layout.index)
            {
                let unscaled_size = egui::vec2(layout.rect.width(), layout.rect.height());
                visible_pages_data.push((
                    layout.index,
                    Arc::clone(scene),
                    page_screen_rect,
                    unscaled_size,
                ));
            }
        }
        visible_pages_data
    }

    pub(crate) fn render_document_panel(
        // RR-15 Limit: GUI - Renders document panel, handles centering, page layouts, and vello texture projection
        &mut self,
        ui: &mut egui::Ui,
        rs: &egui_wgpu::RenderState,
        viewport_rect: egui::Rect,
    ) {
        self.last_viewport_rect = Some(viewport_rect);
        if self.view.display_mode == DisplayMode::Continuous {
            self.compute_layouts();
        }

        if let Some(center_id) = self.ust_registry.pending_center_node_id.take()
            && let Some((page_idx, rect)) = self.ust_registry.find_placement_by_id(center_id)
            && let Some(layout) = self.page_layouts.get(page_idx)
        {
            self.view.center_on_rect(viewport_rect, layout, rect);
        }

        let zoom = self.view.zoom();
        self.handle_marquee_drag_selection(ui, viewport_rect, zoom);
        let visible_pages_data = self.collect_visible_pages_data(viewport_rect, zoom);

        let vello_renderer = match self.vello_renderer.as_mut() {
            Some(r) => r,
            None => return,
        };
        vello_renderer.next_frame(rs);

        let scale_factor = ui.ctx().pixels_per_point();

        // The tile view draws each page from its own texture; the page view composes
        // vector scenes. They are the same boundary, so that a reader who sees tiles is
        // always seeing thumbnails and never wonders which of two thresholds they crossed.
        // A thumbnail costs one draw whatever the page holds, which is what keeps the tile
        // view away from vello's fixed bin-data buffer — 39 consecutive pages of
        // `intel_sdm.pdf` reach it when composed.
        let thumbnails;
        let pixels = if !self.view.is_page_view() {
            thumbnails =
                vello_renderer.ensure_thumbnails(rs, &visible_pages_data, zoom, scale_factor);
            self.pages_left_out = 0;
            crate::view::PagePixels::Thumbnails(&thumbnails)
        } else {
            let id = vello_renderer.render_viewport(
                rs,
                &visible_pages_data,
                viewport_rect,
                scale_factor,
                zoom,
            );
            self.pages_left_out = vello_renderer.pages_left_out();
            crate::view::PagePixels::Viewport(id)
        };

        self.draw_view_with_highlights(ui, viewport_rect, zoom, &pixels);
        self.handle_page_interactions(ui, viewport_rect, zoom);
    }
}

/// What a click on a page is for, this frame.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum PageInput {
    /// A content tool — redaction, caliper, signature placement — has the click.
    Tool,
    /// No tool is active, so the view decides: text in the page view, the page in tiles.
    Normal,
    /// A content tool is active but the view is tiles. The click goes nowhere.
    Suppressed,
}

/// Which of the three a click on a page is.
///
/// **`Suppressed` is the case worth having a name for.** A content tool acts on what is
/// drawn inside a page, and in the tile view a page is 60 pixels wide — a redaction box
/// drawn there covers content nobody can see. Turning the tool off there is not enough on
/// its own: the click would fall through to page selection, and a drag meant to redact
/// would reorder the document instead.
const fn page_input(tool_active: bool, page_view: bool) -> PageInput {
    match (tool_active, page_view) {
        (true, true) => PageInput::Tool,
        (true, false) => PageInput::Suppressed,
        (false, _) => PageInput::Normal,
    }
}

#[cfg(test)]
mod page_input_rules {
    use super::{PageInput, page_input};

    /// A content tool works where its page can be seen, and nowhere else.
    #[test]
    fn a_content_tool_acts_only_in_the_page_view() {
        assert_eq!(page_input(true, true), PageInput::Tool);
        assert_eq!(page_input(true, false), PageInput::Suppressed);
    }

    /// **An active tool never falls through to page selection.** Off is not the same as
    /// out of the way: a drag meant to redact must not reorder the document.
    #[test]
    fn a_suppressed_tool_does_not_become_page_selection() {
        assert_ne!(page_input(true, false), PageInput::Normal);
    }

    /// With no tool active both views behave as they did; the tool is the only thing this
    /// decides.
    #[test]
    fn without_a_tool_the_view_decides_as_before() {
        assert_eq!(page_input(false, true), PageInput::Normal);
        assert_eq!(page_input(false, false), PageInput::Normal);
    }
}
