//! What the pointer does on a page: selecting, dragging tiles and runs, the caliper and
//! the content tools.

use super::{ContentTool, FepdfApp, PageInput, page_input};
use crate::view::Act;

impl FepdfApp {
    pub(super) fn handle_page_click_selection(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        page_idx: usize,
    ) {
        if response.clicked() {
            let shift = ui.input(|ins| ins.modifiers.shift);
            let cmd = ui.input(|ins| ins.modifiers.command || ins.modifiers.ctrl);

            // **The page just clicked is the page being looked at, however it was
            // clicked.** Only the plain click set this, so a reader who extended a
            // selection with shift and then zoomed in crossed into the page view on
            // whichever page had been clicked plainly before — see
            // `PDFView::follow_arrangement_change`, which reads it.
            self.view.active_page = page_idx;

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
            }
        }
    }

    pub(super) fn handle_tile_drag_and_drop(
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

            let indicator_color = crate::app::theme::colors::rust::ACCENT;
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
    /// Names the run under a click, while the text tool is on.
    ///
    /// **The unit is the run**, which is the unit an edit names — not the span the
    /// dragging selection works in. Clicking where no run is unnames the one that was
    /// named, because a reader who clicks off a thing means to stop pointing at it.
    pub(super) fn handle_run_click_on_page(
        &mut self,
        response: &egui::Response,
        page_idx: usize,
        page_screen_rect: egui::Rect,
        frame: crate::interaction::PageFrame,
        zoom: f32,
    ) {
        if self.active_drawer != crate::sidebar::ActiveDrawer::TextRuns || !response.clicked() {
            return;
        }
        let Some(at) = response.interact_pointer_pos() else { return };
        if !page_screen_rect.contains(at) {
            return;
        }
        let on_page =
            crate::interaction::SelectionManager::screen_to_pdf(page_screen_rect, zoom, frame, at);
        // The last one that contains the point, so that a run drawn over another is the
        // one that answers — which is the one the reader sees.
        self.selected_run = self
            .page_runs
            .get(&page_idx)
            .and_then(|runs| runs.iter().rev().find(|run| run.contains(on_page)))
            .map(|run| (page_idx, run.index));
    }

    /// Moves the run under the pointer while it is dragged, and puts it down on release.
    ///
    /// **The other four verbs are a click and a button and this one is a gesture**, so it
    /// is here rather than in the drawer: a reader moving something wants to see where it
    /// will land before they let go.
    pub(super) fn handle_run_drag_on_page(
        &mut self,
        response: &egui::Response,
        page_idx: usize,
        page_screen_rect: egui::Rect,
        frame: crate::interaction::PageFrame,
        zoom: f32,
    ) {
        if self.active_drawer != crate::sidebar::ActiveDrawer::TextRuns {
            return;
        }
        let at = |screen: egui::Pos2| {
            crate::interaction::SelectionManager::screen_to_pdf(
                page_screen_rect,
                zoom,
                frame,
                screen,
            )
        };
        if response.drag_started() {
            self.pick_up_run(response, page_idx, page_screen_rect, &at);
        }
        let Some(mut drag) = self.dragging_run.filter(|drag| drag.page == page_idx) else {
            return;
        };
        if let Some(screen) = response.interact_pointer_pos() {
            drag.now = at(screen);
            self.dragging_run = Some(drag);
        }
        if response.drag_stopped() {
            self.put_down_run(drag);
        }
    }

    /// Takes hold of the run the drag started on, if it started on one.
    pub(super) fn pick_up_run(
        &mut self,
        response: &egui::Response,
        page_idx: usize,
        page_screen_rect: egui::Rect,
        at: &dyn Fn(egui::Pos2) -> egui::Pos2,
    ) {
        let Some(screen) = response.interact_pointer_pos() else { return };
        if !page_screen_rect.contains(screen) {
            return;
        }
        let grabbed_at = at(screen);
        // The last one containing the point, so a run drawn over another is the one that
        // answers — which is the one the reader sees.
        let Some(run) = self
            .page_runs
            .get(&page_idx)
            .and_then(|runs| runs.iter().rev().find(|run| run.contains(grabbed_at)))
        else {
            return;
        };
        self.dragging_run = Some(crate::interaction::DraggingRun {
            page: page_idx,
            run: run.index,
            origin: run.origin,
            grabbed_at,
            now: grabbed_at,
        });
        self.selected_run = Some((page_idx, run.index));
    }

    /// Puts the run down where the drag ended.
    ///
    /// A drag that went nowhere is a click, and a click names a run rather than rewriting
    /// the page. The run keeps its number through a move, so what is named stays named.
    pub(super) fn put_down_run(&mut self, drag: crate::interaction::DraggingRun) {
        self.dragging_run = None;
        if drag.moved_by().length() < 1.0 {
            return;
        }
        let lands = drag.lands_at();
        let _ = self.tx_worker.send(crate::worker::WorkerRequest::Apply {
            operation: Box::new(fepdf::Operation::MoveRun {
                page: drag.page,
                run: drag.run,
                to: (f64::from(lands.x), f64::from(lands.y)),
            }),
            done: self.tr("runs_title"),
        });
    }

    pub(super) fn handle_text_selection_on_page(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        page_idx: usize,
        page_screen_rect: egui::Rect,
        frame: crate::interaction::PageFrame,
        zoom: f32,
    ) {
        self.handle_run_click_on_page(response, page_idx, page_screen_rect, frame, zoom);
        self.handle_run_drag_on_page(response, page_idx, page_screen_rect, frame, zoom);
        if self.view.does(Act::SelectText)
            && let Some(spans) = self.page_spans.get(&page_idx)
        {
            if self.selection_manager.is_tagging_brush_active {
                self.selection_manager.handle_tagging_brush_interaction(
                    ui,
                    page_idx,
                    page_screen_rect,
                    frame,
                    spans,
                    zoom,
                );
            } else {
                self.selection_manager.handle_drag(
                    ui,
                    response,
                    page_idx,
                    page_screen_rect,
                    frame,
                    spans,
                    zoom,
                );
            }
        }
    }

    pub(super) fn handle_page_tile_interaction(
        &mut self,
        ui: &mut egui::Ui,
        page_idx: usize,
        page_screen_rect: egui::Rect,
        frame: crate::interaction::PageFrame,
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
        if self.view.does(Act::SelectPages) {
            self.handle_page_click_selection(ui, &response, page_idx);

            if response.drag_started() && !self.selected_pages.contains(&page_idx) {
                self.selected_pages.clear();
                self.selected_pages.insert(page_idx);
                self.last_selected_page = Some(page_idx);
            }
        }

        // Double-clicking a tile opens that page, in the middle of the window — the same
        // answer a double-click on the bench gives for the page being read, and by the
        // same route. It used to rebuild the layout itself and scroll into it, which was
        // the one way across the tile boundary that did not go through the anchor.
        if response.double_clicked() && self.view.does(Act::OpenPage) {
            self.view.open_page(page_idx);
            self.view.set_zoom(1.0);
        }

        let is_r2l = self.view.binding_direction == crate::view::BindingDirection::RightToLeft;
        let target_slot = if self.view.does(Act::ArrangePages) {
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

        self.handle_text_selection_on_page(ui, &response, page_idx, page_screen_rect, frame, zoom);

        target_slot
    }

    pub(super) fn handle_caliper_page_interaction(
        &mut self,
        ui: &mut egui::Ui,
        page_idx: usize,
        page_screen_rect: egui::Rect,
        frame: crate::interaction::PageFrame,
        zoom: f32,
    ) {
        if let Some(spans) = self.page_spans.get(&page_idx) {
            self.caliper_tool.handle_interaction(
                ui,
                page_idx,
                page_screen_rect,
                frame,
                zoom,
                &mut self.cad_snap_engine,
                spans,
            );
            let locale = &self.locale_mgr;
            let lang = &self.active_language;
            self.caliper_tool.draw_overlay(ui, (page_idx, page_screen_rect), frame, zoom, &|key| {
                locale.tr(lang, key)
            });
        }
    }

    pub(super) fn handle_single_page_interaction(
        &mut self,
        ui: &mut egui::Ui,
        visible_index: usize,
        page_screen_rect: egui::Rect,
        frame: crate::interaction::PageFrame,
        zoom: f32,
        dragged_from: Option<usize>,
    ) -> Option<usize> {
        let tool_active = self.content_tool().is_some();

        match page_input(tool_active, self.view.is_page_view()) {
            // A content tool is out where the tool cannot see what it is acting on, and
            // its drags must not become something else on the way past.
            PageInput::Suppressed => None,
            PageInput::Tool => {
                self.handle_content_tool(ui, visible_index, page_screen_rect, frame, zoom);
                None
            }
            PageInput::Normal => self.handle_page_tile_interaction(
                ui,
                visible_index,
                page_screen_rect,
                frame,
                zoom,
                dragged_from,
            ),
        }
    }

    /// The content tool that has the page, if one does.
    ///
    /// **One home for which tools there are.** Whether a tool is on and what it is handed
    /// were two lists: the snapshot was added to the second and not the first, and for
    /// five days a drag with it on selected the words under it and copied nothing. A
    /// `drag` in a capture plan is what showed it. Both now read this.
    pub(super) fn content_tool(&self) -> Option<ContentTool> {
        if self.is_placing_signature {
            Some(ContentTool::Signature)
        } else if self.caliper_tool.is_active {
            Some(ContentTool::Caliper)
        } else if self.redaction_manager.is_active {
            Some(ContentTool::Redaction)
        } else if self.snapshot_tool.is_active {
            Some(ContentTool::Snapshot)
        } else if self.annotate_tool.is_active {
            Some(ContentTool::Annotate)
        } else {
            None
        }
    }

    /// Hands the page to whichever content tool is active. Only reached in the page view.
    pub(super) fn handle_content_tool(
        &mut self,
        ui: &mut egui::Ui,
        visible_index: usize,
        page_screen_rect: egui::Rect,
        frame: crate::interaction::PageFrame,
        zoom: f32,
    ) {
        let Some(tool) = self.content_tool() else { return };
        let (page, rect) = (visible_index, page_screen_rect);
        match tool {
            ContentTool::Signature => {
                self.handle_signature_placement_interaction(ui, page, rect, frame, zoom);
            }
            ContentTool::Caliper => {
                self.handle_caliper_page_interaction(ui, page, rect, frame, zoom);
            }
            ContentTool::Redaction => {
                self.redaction_manager.handle_interaction(ui, page, rect, frame, zoom);
            }
            ContentTool::Snapshot => {
                if let Some(taken) = self.snapshot_tool.interaction(ui, page, rect, frame, zoom) {
                    self.copy_snapshot(taken);
                }
            }
            ContentTool::Annotate => {
                match self.annotate_tool.interaction(ui, page, rect, frame, zoom) {
                    None => {}
                    Some(Ok(spec)) => {
                        let done = self.tr("annotate_done");
                        let _ = self.tx_worker.send(crate::worker::WorkerRequest::Apply {
                            operation: Box::new(fepdf::Operation::AddAnnotation(spec)),
                            done,
                        });
                    }
                    Some(Err(why)) => self.notice = Some(crate::app::Notice::failed(why)),
                }
            }
        }
    }

    /// Asks the worker for the rectangle the reader dragged.
    ///
    /// **The window does not draw it.** The document lives on the worker's side, and a
    /// rasterisation on the drawing thread is a window that stops while it happens. What
    /// comes back goes on the clipboard, and what it was is said — a picture put there
    /// with nothing said is a gesture a reader cannot tell worked from one that did not.
    pub(super) fn copy_snapshot(&mut self, taken: crate::snapshot::Taken) {
        use crate::snapshot::Purpose;
        let outside = match self.snapshot_tool.purpose {
            Purpose::Copy => {
                let _ = self.tx_worker.send(crate::worker::WorkerRequest::Snapshot {
                    page: taken.page,
                    keep: taken.keep,
                    scale: taken.scale,
                });
                return;
            }
            Purpose::Crop => fepdf::WhatFallsOutside::Stays,
            Purpose::CropAndRemove => fepdf::WhatFallsOutside::Goes,
        };
        // The same rectangle, handed to the crop: the page becomes what was dragged.
        let crop = fepdf::CropRegion { keep: taken.keep, outside };
        let _ = self.tx_worker.send(crate::worker::WorkerRequest::Apply {
            operation: Box::new(fepdf::Operation::CropPages(
                fepdf::PageSelection::Single(taken.page),
                crop,
            )),
            done: self.tr("snapshot_cropped"),
        });
    }

    /// Applies a page drag once the pointer is released, and clears the payload.
    ///
    /// Split from `handle_page_interactions`, which was walking the visible pages *and*
    /// deciding what a finished drag meant. A drag that ends over no slot still has to
    /// clear its payload, which is why the release and the target are separate conditions.
    pub(super) fn commit_page_reorder(
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

    pub(super) fn handle_page_interactions(
        &mut self,
        ui: &mut egui::Ui,
        viewport_rect: egui::Rect,
        zoom: f32,
    ) {
        let mut reorder_target = None;
        let dragged_from = egui::DragAndDrop::payload::<usize>(ui.ctx()).map(|p| *p);
        // **A page that is drawn is a page that can be clicked.** This chose which pages
        // to allocate a rect for by `display_mode`, the third copy of a decision that
        // predates the grid belonging to the zoom: in the tiles the mode is `SinglePage`,
        // so every tile but one was skipped and neither a click nor a right-click reached
        // any of them.
        let pages: Vec<(usize, egui::Rect, crate::interaction::PageFrame)> = self
            .view
            .visible_page_rects(viewport_rect, &self.page_layouts)
            .into_iter()
            .map(|(layout, page_screen_rect)| (layout.index, page_screen_rect, layout.frame))
            .collect();
        for (visible_index, page_screen_rect, frame) in pages {
            if let Some(target) = self.handle_single_page_interaction(
                ui,
                visible_index,
                page_screen_rect,
                frame,
                zoom,
                dragged_from,
            ) {
                reorder_target = Some(target);
            }
        }

        self.commit_page_reorder(ui, dragged_from, reorder_target);
    }
}
