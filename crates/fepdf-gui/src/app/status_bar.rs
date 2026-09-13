//! The status bar, and the view controls that used to sit on its right.
//!
//! **They are two things and are now drawn as two.** The bar says what the engine is
//! doing — which document, what just happened, what the renderer left out — and does not
//! move. The controls act on the view, float over the page, and are only there when they
//! are being reached for.
//!
//! The controls used to be built in reverse, because `Layout::right_to_left` places the
//! first widget furthest right and they were pinned to the bar's right end. Floating, they
//! read left to right and are written that way.

use super::FepdfApp;
use super::icons::{glyph, icon_action, named};
use super::theme::{colors, size, space, text};
use crate::view::{BindingDirection, DisplayMode};

impl FepdfApp {
    pub(crate) fn render_status_bar(&mut self, ui: &mut egui::Ui) {
        let has_doc = self.total_pages > 0;

        egui::Panel::bottom("status_bar").default_size(size::STATUS).resizable(false).show_inside(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = space::ITEM;
                    self.say_file_name(ui);
                    self.status_indicators(ui, has_doc);
                });
            },
        );
    }

    /// The name of the file the window is holding, at the far left.
    ///
    /// **The window's own title bar is not this.** It carries the application's name, and
    /// a reader with two of these open has nothing on screen that says which document is
    /// in front of them. Absent rather than blank when nothing is open: an empty label
    /// still takes the space of one.
    fn say_file_name(&mut self, ui: &mut egui::Ui) {
        let Some(name) = self.pdf_name.clone() else { return };
        ui.label(egui::RichText::new(name).size(text::SMALL).color(colors::steel::TEXT));
        ui.separator();
    }

    /// The left of the bar: what just happened, what the renderer left out, which view
    /// mode is in force, and the reading-order toggle.
    fn status_indicators(&mut self, ui: &mut egui::Ui, has_doc: bool) {
        if let Some(key) = self.busy {
            // What the history is doing outranks what just happened: it is happening now.
            ui.label(
                egui::RichText::new(self.locale_mgr.tr(&self.active_language, key))
                    .size(text::SMALL)
                    .color(colors::rust::ACCENT),
            );
        } else if self.notice.is_some() {
            self.say_notice(ui);
        } else if self.pages_left_out > 0 {
            // Pages the renderer left out say so here rather than in the decision
            // sidebar: see `FepdfApp::pages_left_out` for why that is not a `Decision`.
            let over = self
                .locale_mgr
                .tr(&self.active_language, "status_pages_over_budget")
                .replacen("{}", &self.pages_left_out.to_string(), 1);
            ui.label(egui::RichText::new(over).size(text::SMALL).color(colors::note::WARN));
        } else {
            ui.label(
                egui::RichText::new(self.locale_mgr.tr(&self.active_language, "status_ready"))
                    .size(text::SMALL),
            );
        }

        // Which mode the view is in. Nothing announces that content interactions have
        // stopped below `PDFView::TILE_ZOOM`; this is where a reader finds out which
        // side of that boundary they are on.
        if has_doc {
            ui.add_space(space::SECTION);
            let key =
                if self.view.is_page_view() { "status_mode_page" } else { "status_mode_tile" };
            ui.label(
                egui::RichText::new(self.locale_mgr.tr(&self.active_language, key))
                    .size(text::SMALL)
                    .color(colors::steel::MUTED),
            );
        }

        ui.add_space(space::SECTION);

        let reading_txt = if self.show_reading_order {
            self.locale_mgr.tr(&self.active_language, "reading_order_enabled")
        } else {
            self.locale_mgr.tr(&self.active_language, "reading_order_disabled")
        };
        let reading = egui::RichText::new(reading_txt).size(text::SMALL);
        if ui.selectable_label(self.show_reading_order, reading).clicked() {
            self.show_reading_order = !self.show_reading_order;
        }
    }

    /// What just happened, in the slot the ready line otherwise holds.
    ///
    /// **It is dismissible, and it does not touch the document.** The line this replaces
    /// was drawn over the canvas instead of on the bar, so a save that worked emptied the
    /// window it was reporting on and stayed there until another file was opened.
    fn say_notice(&mut self, ui: &mut egui::Ui) {
        let Some(notice) = &self.notice else { return };
        let colour = notice.colour();
        let words = notice.say(&self.locale_mgr, &self.active_language);
        ui.label(egui::RichText::new(words).size(text::SMALL).color(colour));
        ui.add_space(space::ITEM);
        if icon_action(ui, glyph::CLOSE, false, true, &self.tr("btn_close")).clicked() {
            self.notice = None;
        }
    }

    /// The controls that act on the view, floating over the page near the bottom.
    ///
    /// **Not on the status bar, and not movable.** They are about the view rather than
    /// about the document, they are wanted while reading and in the way while looking, and
    /// a bar the reader can drag is a bar the reader has to find again. It sits above the
    /// status bar, centred, and appears when the pointer comes down to it — or stays, when
    /// the pin says so.
    pub(crate) fn render_view_controls(&mut self, ui: &mut egui::Ui) {
        if self.total_pages == 0 {
            return;
        }
        // **The whole window, not what is left of it.** The reveal band is measured from
        // the bottom of the screen, which is where the reader's hand goes.
        let window = ui.ctx().content_rect();
        let reached_for = ui
            .ctx()
            .pointer_latest_pos()
            .is_some_and(|at| at.y > window.max.y - size::REVEAL && window.contains(at));
        // **Shown or not shown, with nothing in between.** A fade sat here and could stall
        // at zero: `Context::animate_bool_with_time` asks for the next frame only while
        // the value is strictly between 0 and 1, so on the frame the target flips — before
        // any time has passed — nothing is requested, and a pointer that stopped as it
        // crossed the line left the bar at zero with no frame coming to move it. It
        // appeared while the pointer kept moving and not when it came to rest, which is
        // the wrong way round for a control one reaches for.
        if !self.controls_pinned && !reached_for {
            return;
        }

        let current_page = self.page_being_read();
        let frame = egui::Frame::new()
            .fill(colors::paper::WHITE)
            .stroke(egui::Stroke::new(1.0_f32, colors::steel::RULE))
            .corner_radius(super::theme::radius::CONTROL)
            .inner_margin(space::GROUP);
        egui::Area::new(egui::Id::new("view_controls"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -(size::STATUS + space::SECTION)))
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = space::ITEM;
                        self.history_group(ui);
                        ui.add_space(space::SECTION);
                        self.page_group(ui, current_page);
                        ui.add_space(space::SECTION);
                        self.zoom_group(ui);
                        ui.add_space(space::SECTION);
                        self.mode_group(ui);
                        ui.add_space(space::SECTION);
                        self.document_group(ui);
                        ui.add_space(space::SECTION);
                        self.pin_control(ui);
                    });
                });
            });
    }

    /// Holds the controls open, or lets them come and go again.
    fn pin_control(&mut self, ui: &mut egui::Ui) {
        let pinned = self.controls_pinned;
        let icon = if pinned { glyph::PIN } else { glyph::PIN_OFF };
        let tip = self.tr(if pinned { "tooltip_unpin_controls" } else { "tooltip_pin_controls" });
        if icon_action(ui, icon, pinned, true, &tip).clicked() {
            self.controls_pinned = !pinned;
        }
    }

    /// The page the reader is on. The rule is [`crate::view::PDFView::current_page`],
    /// which is also what draws the line under that page's number.
    fn page_being_read(&self) -> usize {
        self.last_viewport_rect.map_or(self.view.active_page, |viewport| {
            self.view.current_page(viewport, &self.page_layouts)
        })
    }

    /// Leftmost of the bar: taking back, and putting back.
    ///
    /// **A shortcut is not an entry point (UI-4).** `Cmd+Z` reaches these two and a
    /// reader who does not already know that would find nothing; drawn disabled rather
    /// than hidden, they also say that the window has a history at all.
    fn history_group(&mut self, ui: &mut egui::Ui) {
        let undo_name = self.tr("tooltip_undo");
        if icon_action(ui, glyph::UNDO, false, self.can_undo, &undo_name).clicked() && self.can_undo
        {
            self.can_undo = false;
            self.begin_rebuild("history_undoing");
            let _ = self.tx_worker.send(crate::worker::WorkerRequest::Undo);
        }

        let redo_name = self.tr("tooltip_redo");
        if icon_action(ui, glyph::REDO, false, self.can_redo, &redo_name).clicked() && self.can_redo
        {
            self.can_redo = false;
            self.begin_rebuild("history_redoing");
            let _ = self.tx_worker.send(crate::worker::WorkerRequest::Redo);
        }
    }

    /// Rightmost: what to do to the document itself.
    fn document_group(&mut self, ui: &mut egui::Ui) {
        let is_r2l = self.view.binding_direction == BindingDirection::RightToLeft;
        let label = egui::RichText::new(self.tr(if is_r2l {
            "btn_binding_vertical"
        } else {
            "btn_binding_horizontal"
        }))
        .size(text::BODY);
        let binding = egui::Button::new(label)
            .min_size(egui::vec2(size::ICON, size::ICON))
            .corner_radius(super::theme::radius::CONTROL)
            .selected(is_r2l);
        let binding_name =
            self.tr(if is_r2l { "tooltip_binding_r2l" } else { "tooltip_binding_ltr" });
        if named(ui.add(binding), true, &binding_name).clicked() {
            self.view.binding_direction =
                if is_r2l { BindingDirection::LeftToRight } else { BindingDirection::RightToLeft };
            self.compute_layouts();
        }

        if icon_action(ui, glyph::ROTATE, false, true, &self.tr("tooltip_rotate_cw")).clicked() {
            self.rotate_selected_pages(fepdf::Quarter::Q90);
        }
    }

    /// How the pages are arranged, reading continuous, single, spread.
    fn mode_group(&mut self, ui: &mut egui::Ui) {
        for (mode, icon, key) in [
            (DisplayMode::Continuous, glyph::PAGE_CONTINUOUS, "tooltip_view_continuous"),
            (DisplayMode::SinglePage, glyph::PAGE_SINGLE, "tooltip_view_single"),
            (DisplayMode::TwoPageSpread, glyph::PAGE_SPREAD, "tooltip_view_spread"),
        ] {
            let selected = self.view.display_mode == mode;
            let tip = self.tr(key);
            if icon_action(ui, icon, selected, true, &tip).clicked() {
                self.view.display_mode = mode;
                self.compute_layouts();
            }
        }
    }

    /// Zoom out, the current factor, zoom in.
    fn zoom_group(&mut self, ui: &mut egui::Ui) {
        let viewport = self.last_viewport_rect.unwrap_or_else(|| ui.max_rect());
        let center = viewport.center();

        if icon_action(ui, glyph::ZOOM_OUT, false, true, &self.tr("tooltip_zoom_out")).clicked() {
            self.view.zoom_at(self.view.zoom_step_down(), center, viewport, &self.page_layouts);
        }

        let label = egui::RichText::new(self.view.zoom_label()).size(text::SMALL);
        let reset = egui::Button::new(label)
            .min_size(egui::vec2(size::ICON * 1.5, size::ICON))
            .corner_radius(super::theme::radius::CONTROL);
        if named(ui.add(reset), true, &self.tr("tooltip_zoom_reset")).clicked() {
            self.view.zoom_at(1.0, center, viewport, &self.page_layouts);
        }

        if icon_action(ui, glyph::ZOOM_IN, false, true, &self.tr("tooltip_zoom_in")).clicked() {
            self.view.zoom_at(self.view.zoom_step_up(), center, viewport, &self.page_layouts);
        }
    }

    /// The page counter and the four buttons around it, reading first, previous, `n/N`,
    /// next, last.
    fn page_group(&mut self, ui: &mut egui::Ui, current_page: usize) {
        let viewport = self.last_viewport_rect.unwrap_or_else(|| ui.max_rect());
        if icon_action(ui, glyph::PAGE_FIRST, false, true, &self.tr("tooltip_page_first")).clicked()
        {
            self.view.scroll_to_page(0, viewport, &self.page_layouts);
        }

        if icon_action(ui, glyph::PAGE_PREV, false, true, &self.tr("tooltip_page_prev")).clicked()
            && current_page > 0
        {
            self.view.scroll_to_page(current_page - 1, viewport, &self.page_layouts);
        }

        ui.label(
            egui::RichText::new(format!("{}/{}", current_page + 1, self.total_pages))
                .size(text::SMALL)
                .color(colors::steel::TEXT),
        );

        if icon_action(ui, glyph::PAGE_NEXT, false, true, &self.tr("tooltip_page_next")).clicked()
            && current_page + 1 < self.total_pages
        {
            self.view.scroll_to_page(current_page + 1, viewport, &self.page_layouts);
        }

        if icon_action(ui, glyph::PAGE_LAST, false, true, &self.tr("tooltip_page_last")).clicked() {
            self.view.scroll_to_page(self.total_pages - 1, viewport, &self.page_layouts);
        }
    }
}
