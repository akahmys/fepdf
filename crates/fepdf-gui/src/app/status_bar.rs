//! The status bar: what the engine is doing, and which document it is doing it to.
//!
//! **It does not move and it holds nothing that acts on the view.** Those went to
//! `view_controls`, which floats: the two were one strip, and a bar that says what is
//! happening is a different thing from a set of controls for changing what you are
//! looking at.

use super::FepdfApp;
use super::icons::{glyph, icon_action};
use super::theme::{colors, size, space, text};

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
}
