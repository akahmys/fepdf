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
    /// selection, the drag-and-drop slot, and this. Below `PDFView::OVERVIEW_ZOOM` the
    /// tiles are thumbnails being reordered, not pages being read from.
    fn handle_text_selection_on_page(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        page_idx: usize,
        page_screen_rect: egui::Rect,
        unscaled_h: f32,
        zoom: f32,
    ) {
        if zoom >= crate::view::PDFView::OVERVIEW_ZOOM
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
        self.render_page_context_menu(&response, page_idx);
        self.handle_page_click_selection(ui, &response, page_idx);

        if response.drag_started() && !self.selected_pages.contains(&page_idx) {
            self.selected_pages.clear();
            self.selected_pages.insert(page_idx);
            self.last_selected_page = Some(page_idx);
        }

        if response.double_clicked() && zoom < crate::view::PDFView::OVERVIEW_ZOOM {
            self.view.set_zoom(1.0);
            self.view.scroll_to_page(page_idx, &self.page_layouts);
        }

        let is_r2l = self.view.binding_direction == crate::view::BindingDirection::RightToLeft;
        let target_slot = if zoom < crate::view::PDFView::OVERVIEW_ZOOM {
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
        if self.is_placing_signature {
            self.handle_signature_placement_interaction(
                ui,
                visible_index,
                page_screen_rect,
                unscaled_h,
                zoom,
            );
            None
        } else if self.caliper_tool.is_active {
            self.handle_caliper_page_interaction(
                ui,
                visible_index,
                page_screen_rect,
                unscaled_h,
                zoom,
            );
            None
        } else if self.redaction_manager.is_active {
            self.redaction_manager.handle_interaction(
                ui,
                visible_index,
                page_screen_rect,
                unscaled_h,
                zoom,
            );
            None
        } else {
            self.handle_page_tile_interaction(
                ui,
                visible_index,
                page_screen_rect,
                unscaled_h,
                zoom,
                dragged_from,
            )
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
        viewport_texture_id: Option<egui::TextureId>,
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
            viewport_texture_id,
            viewport_rect,
            &self.scenes,
            &self.selection_manager.highlights,
            &redaction_highlights,
            &active_redaction_drag,
            &structural_highlight,
            &signature_highlight,
            &self.selected_pages,
            &self.ust_registry,
            // Off below `LEGIBLE_ZOOM`: the borders are drawn per node of every visible
            // page, so at the zoom floor this is the heaviest thing on screen and the
            // least readable — the lines are finer than the glyphs they enclose.
            self.show_reading_order && self.view.is_legible(),
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

        if zoom < crate::view::PDFView::OVERVIEW_ZOOM && shift_down {
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
        let viewport_texture_id = vello_renderer.render_viewport(
            rs,
            &visible_pages_data,
            viewport_rect,
            scale_factor,
            zoom,
        );
        let pages_left_out = vello_renderer.pages_left_out();
        self.pages_left_out = pages_left_out;

        self.draw_view_with_highlights(ui, viewport_rect, zoom, viewport_texture_id);
        self.handle_page_interactions(ui, viewport_rect, zoom);
    }
}
