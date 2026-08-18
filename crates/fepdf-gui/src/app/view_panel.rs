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

    fn handle_page_interactions(
        // RR-15 Limit: GUI - Unified egui pointer and canvas coordinate interaction loop
        &mut self,
        ui: &mut egui::Ui,
        viewport_rect: egui::Rect,
        zoom: f32,
    ) {
        let visible_pages = self.view.visible_pages.clone();
        let active_spread = self.view.get_spread_indices(self.view.active_page, self.total_pages);
        for &visible_index in &visible_pages {
            if self.view.display_mode == DisplayMode::SinglePage
                && visible_index != self.view.active_page
            {
                continue;
            }
            if self.view.display_mode == DisplayMode::TwoPageSingle
                && !active_spread.contains(&visible_index)
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

                if self.is_placing_signature {
                    self.handle_signature_placement_interaction(
                        ui,
                        visible_index,
                        page_screen_rect,
                        unscaled_h,
                        zoom,
                    );
                } else if self.caliper_tool.is_active {
                    if let Some(spans) = self.page_spans.get(&visible_index) {
                        self.caliper_tool.handle_interaction(
                            ui,
                            visible_index,
                            page_screen_rect,
                            unscaled_h,
                            zoom,
                            &mut self.cad_snap_engine,
                            spans,
                        );
                        self.caliper_tool.draw_overlay(ui, page_screen_rect, unscaled_h, zoom);
                    }
                } else if self.redaction_manager.is_active {
                    self.redaction_manager.handle_interaction(
                        ui,
                        visible_index,
                        page_screen_rect,
                        unscaled_h,
                        zoom,
                    );
                } else if let Some(spans) = self.page_spans.get(&visible_index) {
                    if self.selection_manager.is_tagging_brush_active {
                        self.selection_manager.handle_tagging_brush_interaction(
                            ui,
                            visible_index,
                            page_screen_rect,
                            unscaled_h,
                            spans,
                            zoom,
                        );
                    } else {
                        self.selection_manager.handle_interaction(
                            ui,
                            visible_index,
                            page_screen_rect,
                            unscaled_h,
                            spans,
                            zoom,
                        );
                    }
                }
            }
        }
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
            &self.ust_registry,
            self.show_reading_order,
        );
    }

    pub(crate) fn render_document_panel(
        // RR-15 Limit: GUI - Renders document panel, handles centering, page layouts, and vello texture projection
        &mut self,
        ui: &mut egui::Ui,
        rs: &egui_wgpu::RenderState,
        viewport_rect: egui::Rect,
    ) {
        if let Some(center_id) = self.ust_registry.pending_center_node_id.take()
            && let Some((page_idx, rect)) = self.ust_registry.find_placement_by_id(center_id)
            && let Some(layout) = self.page_layouts.get(page_idx)
        {
            self.view.center_on_rect(viewport_rect, layout, rect);
        }

        let vello_renderer = match self.vello_renderer.as_mut() {
            Some(r) => r,
            None => return,
        };
        vello_renderer.next_frame(rs);

        let zoom = self.view.zoom;

        // Collect visible pages and their scenes
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

        let scale_factor = ui.ctx().pixels_per_point();
        let viewport_texture_id = vello_renderer.render_viewport(
            rs,
            &visible_pages_data,
            viewport_rect,
            scale_factor,
            zoom,
        );

        self.handle_page_interactions(ui, viewport_rect, zoom);
        self.draw_view_with_highlights(ui, viewport_rect, zoom, viewport_texture_id);
    }
}
