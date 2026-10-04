//! Viewport panel and canvas rendering for `FepdfApp`.
//!
//! **The panel's methods are in three files** (ROADMAP Y-7): the menus a page offers are
//! `page_menus`, what the pointer does on a page is `page_pointer`, and drawing the view
//! and the panel around it stay here.

/// The menus a page offers: rotating, sheets, tab order, files, and the pages they act on.
mod page_menus;
/// What the pointer does on a page: selecting, dragging tiles and runs, the caliper and
/// the content tools.
mod page_pointer;

use super::FepdfApp;
use crate::interaction::SelectionManager;
use crate::view::{Act, DisplayMode};
use crate::worker::WorkerRequest;
use std::collections::BTreeMap;
use std::sync::Arc;

impl FepdfApp {
    pub(crate) fn check_gpu_support(&self, ui: &mut egui::Ui, frame: &mut eframe::Frame) -> bool {
        let has_wgpu = frame.wgpu_render_state().is_some();
        if !has_wgpu {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    // **The one screen a reader sees when nothing else works** was the
                    // only one written in English in the source, while `gpu_unavailable`
                    // sat translated in both locale files with nothing naming it. UI-5
                    // missed it because the literal is the second argument of the call
                    // and its check reads the first.
                    ui.colored_label(
                        crate::app::theme::colors::note::FAIL,
                        self.tr("gpu_unavailable"),
                    );
                });
            });
            return false;
        }
        true
    }

    /// Asks the worker for the pages that are on screen, and the ones either side of them.
    ///
    /// **What is on screen is the view's question, not the mode's.** This chose its targets
    /// by `display_mode`, and the arm that queued every visible tile was the one for the
    /// mode that no longer exists: with the grid moved to the zoom, a reader who zoomed out
    /// was in `SinglePage` looking at twenty-three tiles while this asked for three pages.
    /// The other twenty never arrived, and the tiles spun for ever.
    pub(crate) fn queue_visible_pages(&mut self) {
        // Collect visible pages and calculate pre-render lookahead indices
        let mut render_targets = std::collections::BTreeSet::new();
        if !self.view.is_page_view() {
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
        } else if self.view.display_mode == DisplayMode::SinglePage {
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
        frame: crate::interaction::PageFrame,
        zoom: f32,
    ) {
        let response = ui.allocate_rect(page_screen_rect, egui::Sense::drag());
        let screen_pos = ui.input(|i| i.pointer.hover_pos());

        if response.drag_started()
            && let Some(pos) = screen_pos
        {
            let pdf_pos = SelectionManager::screen_to_pdf(page_screen_rect, zoom, frame, pos);
            self.signature_position =
                Some((visible_index, egui::Rect::from_min_max(pdf_pos, pdf_pos)));
        }

        if response.dragged()
            && let Some(pos) = screen_pos
            && let Some((sig_idx, sig_rect)) = &mut self.signature_position
            && *sig_idx == visible_index
        {
            let pdf_pos = SelectionManager::screen_to_pdf(page_screen_rect, zoom, frame, pos);
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
        let frame = layout.frame;
        let screen_min = SelectionManager::pdf_to_screen(
            page_screen_rect,
            zoom,
            frame,
            egui::pos2(rect[0], rect[3]),
        );
        let screen_max = SelectionManager::pdf_to_screen(
            page_screen_rect,
            zoom,
            frame,
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
        let frame = layout.frame;
        let screen_min = SelectionManager::pdf_to_screen(
            page_screen_rect,
            zoom,
            frame,
            egui::pos2(sig_rect.min.x, sig_rect.max.y),
        );
        let screen_max = SelectionManager::pdf_to_screen(
            page_screen_rect,
            zoom,
            frame,
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
        let mut redactions =
            crate::view::draw::Redactions { zones: BTreeMap::new(), going: BTreeMap::new() };
        let mut active_redaction_drag = None;
        let mut snapshot_drag = None;

        for &visible_index in &self.view.visible_pages {
            if let Some(layout) = self.page_layouts.get(visible_index) {
                let page_screen_rect = egui::Rect::from_min_size(
                    origin + layout.rect.min.to_vec2() * zoom,
                    layout.rect.size() * zoom,
                );
                let frame = layout.frame;

                let (completed, active_drag) = self.redaction_manager.get_screen_highlights(
                    visible_index,
                    page_screen_rect,
                    frame,
                    zoom,
                );
                if !completed.is_empty() {
                    redactions.zones.insert(visible_index, completed);
                }
                let going = self.redaction_manager.going_on_screen(
                    visible_index,
                    page_screen_rect,
                    frame,
                    zoom,
                );
                if !going.is_empty() {
                    redactions.going.insert(visible_index, going);
                }
                if let Some(drag_rect) = active_drag {
                    active_redaction_drag = Some((visible_index, drag_rect));
                }
                // The snapshot's rectangle is drawn the same way the redaction brush's
                // is, and through the same overlay: a reader dragging one is looking at
                // the same question — what is inside this.
                if let Some(drag_rect) = self.snapshot_tool.dragging(page_screen_rect, frame, zoom)
                {
                    snapshot_drag = Some((visible_index, drag_rect));
                }
                // Where the page looks different from the one it was compared with, on a
                // layer above the page so it is drawn whatever else is.
                crate::comparing::outline(
                    &self.compare,
                    ui,
                    visible_index,
                    page_screen_rect,
                    frame,
                    zoom,
                );
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
            &redactions,
            &active_redaction_drag.or(snapshot_drag),
            &structural_highlight,
            &signature_highlight,
            &self.selected_pages,
            &self.ust_registry,
            // Off in the tile view: the borders are drawn per node of every visible page,
            // and the lines are finer than the glyphs they enclose.
            self.show_reading_order && self.view.is_page_view(),
            marquee_rect,
            &crate::view::draw::Words {
                placeholder: &self.tr("page_rendering"),
                signature: &self.tr("signature_field"),
            },
            &crate::view::draw::TextRuns {
                boxes: &self.page_runs,
                selected: self.selected_run,
                dragging: self.dragging_run,
                // Off in the tile view, for the reason the reading order is: a frame per
                // run of every visible page is finer than the glyphs it encloses.
                // **The drawer is the switch**, rather than a flag beside it: a frame
                // round every run is what a reader asks for by opening the drawer that
                // edits them, and two switches for one thing is one of them going stale.
                showing: self.active_drawer == crate::sidebar::ActiveDrawer::TextRuns
                    && self.view.is_page_view(),
            },
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

        if self.view.does(Act::SelectPages) && shift_down {
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

    /// The pages to hand the renderer: what the view shows, with the pixels they have.
    ///
    /// **It asked the display mode which pages were on screen**, and the answer predates
    /// the grid belonging to the zoom: a reader who zoomed out was in `SinglePage` looking
    /// at twenty-three tiles while this named one, so every other tile was handed no scene
    /// and span for ever. [`PDFView::visible_page_rects`] is the one answer now.
    fn collect_visible_pages_data(
        &self,
        viewport_rect: egui::Rect,
    ) -> Vec<(usize, Arc<vello::Scene>, egui::Rect, egui::Vec2)> {
        self.view
            .visible_page_rects(viewport_rect, &self.page_layouts)
            .into_iter()
            .filter_map(|(layout, page_screen_rect)| {
                let scene = self.scenes.get(&layout.index)?;
                let unscaled_size = egui::vec2(layout.rect.width(), layout.rect.height());
                Some((layout.index, Arc::clone(scene), page_screen_rect, unscaled_size))
            })
            .collect()
    }

    pub(crate) fn render_document_panel(
        // RR-15 Limit: GUI - Renders document panel, handles centering, page layouts, and vello texture projection
        &mut self,
        ui: &mut egui::Ui,
        rs: &egui_wgpu::RenderState,
        viewport_rect: egui::Rect,
    ) {
        self.last_viewport_rect = Some(viewport_rect);
        // **The tiles are laid out against the window, so they are laid out again when it
        // changes.** This asked whether the mode was `Continuous`, which was the mode the
        // grid used to live in; the grid belongs to the zoom now and the page view's
        // layout does not read the window at all.
        if self.view.is_page_view() {
            // **A request to centre a page waits for a window to centre it in.** The first
            // layout after a document loads runs before the viewport is known, so the
            // request is left standing; the tiles get their answer from the recompute
            // below, and the page view — which does not recompute, its layout reading
            // neither the zoom nor the window — needs asking here. Without this a document
            // opened at its first page half a page below the middle.
            self.view.restore_anchor(None, viewport_rect, &self.page_layouts);
        } else {
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
        let visible_pages_data = self.collect_visible_pages_data(viewport_rect);

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
                self.view.pan,
            );
            self.pages_left_out = vello_renderer.pages_left_out();
            crate::view::PagePixels::Viewport(id)
        };

        self.draw_view_with_highlights(ui, viewport_rect, zoom, &pixels);
        self.handle_page_interactions(ui, viewport_rect, zoom);
    }
}

/// The pages a menu entry reached from `page_idx` acts on: the selection when that page is
/// in it, and the page alone when it is not.
///
/// **Worked out, not written down.** Both halves of the menu used to *set* the selection
/// while drawing themselves, so a right-click on an unselected page threw the selection
/// away before the reader had chosen anything — pressing Escape cost them a selection —
/// and choosing "even pages" from the menu above lost it on the same frame, because the
/// entries below ran afterwards and found the clicked page was not one of the even ones.
/// `&self` is what keeps it that way now.
fn pages_in_hand(
    selection: &std::collections::BTreeSet<usize>,
    page_idx: usize,
) -> std::collections::BTreeSet<usize> {
    if selection.contains(&page_idx) {
        selection.clone()
    } else {
        std::collections::BTreeSet::from([page_idx])
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
/// The tools that take the page's pointer from the page itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentTool {
    Signature,
    Caliper,
    Redaction,
    Snapshot,
    Annotate,
}

const fn page_input(tool_active: bool, page_view: bool) -> PageInput {
    match (tool_active, page_view) {
        (true, true) => PageInput::Tool,
        (true, false) => PageInput::Suppressed,
        (false, _) => PageInput::Normal,
    }
}

#[cfg(test)]
mod what_an_entry_acts_on {
    use super::pages_in_hand;
    use std::collections::BTreeSet;

    /// A right-click inside the selection acts on all of it.
    #[test]
    fn a_page_in_the_selection_brings_the_selection() {
        let picked: BTreeSet<usize> = [1, 3, 5, 7].into_iter().collect();
        assert_eq!(pages_in_hand(&picked, 3), picked);
    }

    /// **And outside it means that page alone.** A reader who picks out four pages, then
    /// right-clicks a fifth and reads "delete the selected pages (4)", would lose four
    /// pages they were not pointing at.
    #[test]
    fn a_page_outside_the_selection_means_that_page() {
        let picked: BTreeSet<usize> = [1, 3, 5, 7].into_iter().collect();
        assert_eq!(pages_in_hand(&picked, 4), BTreeSet::from([4]));
        assert_eq!(pages_in_hand(&BTreeSet::new(), 0), BTreeSet::from([0]));
    }

    /// Asking cannot change what it is asking about: the entries are drawn every frame the
    /// menu is open, and one that wrote the selection as it drew ate the reader's.
    #[test]
    fn asking_leaves_the_selection_alone() {
        let picked: BTreeSet<usize> = [0, 2, 4].into_iter().collect();
        let before = picked.clone();
        assert_eq!(pages_in_hand(&picked, 9), BTreeSet::from([9]));
        assert_eq!(picked, before, "asking changed the selection");
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
