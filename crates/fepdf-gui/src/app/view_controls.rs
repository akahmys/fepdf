//! The controls that act on the view: floating over the page, and only there when they
//! are being reached for.
//!
//! **Not the status bar, which is next door.** They were its right-hand half, which made
//! one strip out of two things — what the engine is doing, and what the reader is doing to
//! the view. The first belongs on a bar that does not move; the second is wanted while
//! reading and in the way while looking.
//!
//! Floating, they read left to right, and they are written that way. Each group used to be
//! built backwards because `Layout::right_to_left` places the first widget furthest right.

use super::FepdfApp;
use super::icons::{glyph, icon_action, named};
use super::theme::{colors, size, space, text};
use crate::view::{BindingDirection, DisplayMode};

impl FepdfApp {
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
                        ui.add_space(space::GROUP);
                        self.binding_group(ui);
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
        if icon_action(ui, glyph::UNDO, false, self.can_undo, &undo_name).clicked() {
            self.can_undo = false;
            self.begin_rebuild("history_undoing");
            let _ = self.tx_worker.send(crate::worker::WorkerRequest::Undo);
        }

        let redo_name = self.tr("tooltip_redo");
        if icon_action(ui, glyph::REDO, false, self.can_redo, &redo_name).clicked() {
            self.can_redo = false;
            self.begin_rebuild("history_redoing");
            let _ = self.tx_worker.send(crate::worker::WorkerRequest::Redo);
        }
    }

    /// Rightmost: what to do to the document itself, rather than to the view of it.
    fn document_group(&mut self, ui: &mut egui::Ui) {
        if icon_action(ui, glyph::ROTATE, false, true, &self.tr("tooltip_rotate_cw")).clicked() {
            self.rotate_selected_pages(fepdf::Quarter::Q90);
        }
    }

    /// Which edge the document is bound on, which decides the order its pages run in: the
    /// grid's rows, the two halves of a spread, and which way the arrows go.
    ///
    /// **Two buttons, chosen the way the arrangement beside it is chosen.** It was one
    /// button labelled `横` or `縦` — the only text in a bar of icons, naming the *writing*
    /// direction for a control that sets the *binding*, its label saying the state it was
    /// in while its tooltip said the state a click would reach, and sitting with rotate,
    /// which edits the document, when it only changes the view. Two buttons have no state
    /// to disagree about: one is selected, and that is the binding.
    fn binding_group(&mut self, ui: &mut egui::Ui) {
        for (direction, icon, key) in [
            (BindingDirection::LeftToRight, glyph::BOUND_LEFT, "tooltip_bound_left"),
            (BindingDirection::RightToLeft, glyph::BOUND_RIGHT, "tooltip_bound_right"),
        ] {
            let selected = self.view.binding_direction == direction;
            let tip = self.tr(key);
            if icon_action(ui, icon, selected, true, &tip).clicked() {
                self.view.binding_direction = direction;
                self.compute_layouts();
            }
        }
    }

    /// How the pages are arranged, reading continuous, single, spread.
    ///
    /// **Only the continuous arrangement has tiles.** Single-page and spread lay one page
    /// or one pair out and draw nothing else, so pressing either from the grid replaced a
    /// screen of pages with one small sheet and no way back but the zoom. They are
    /// unavailable until the reader is looking at pages again.
    fn mode_group(&mut self, ui: &mut egui::Ui) {
        // **The mode is what the page view will be, and the tiles are not one of them.**
        // Only `Continuous` used to be pressable in the tiles, because the grid was laid
        // out inside that mode's branch — so the button for the arrangement in hand looked
        // like the mode, and the other two looked broken. The grid belongs to the zoom
        // now, so all three stay available and each says what zooming back in will give.
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

        // **With the pages rather than with the zoom.** It answers "where was I", which is
        // the question the four buttons beside it are already about; the zoom's question
        // is how close.
        if icon_action(ui, glyph::CENTRE, false, true, &self.tr("tooltip_centre_page")).clicked() {
            self.view.centre_current_page(viewport, &self.page_layouts);
        }
    }
}
