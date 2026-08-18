//! Status bar footer rendering for `FepdfApp`.

use super::FepdfApp;

impl FepdfApp {
    pub(crate) fn render_status_bar(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar").default_size(28.0).resizable(false).show_inside(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(self.locale_mgr.tr(&self.active_language, "status_ready"));
                    ui.separator();
                    if self.total_pages > 0 {
                        let current_page = self.view.visible_pages.first().copied().unwrap_or(0);
                        let indicator = self
                            .locale_mgr
                            .tr(&self.active_language, "page_indicator")
                            .replacen("{}", &(current_page + 1).to_string(), 1)
                            .replacen("{}", &self.total_pages.to_string(), 1);
                        ui.label(indicator);
                    } else {
                        ui.label(self.locale_mgr.tr(&self.active_language, "no_doc_loaded"));
                    }
                    ui.separator();
                    if self.show_reading_order {
                        ui.label(
                            self.locale_mgr.tr(&self.active_language, "reading_order_enabled"),
                        );
                    } else {
                        ui.label(
                            self.locale_mgr.tr(&self.active_language, "reading_order_disabled"),
                        );
                    }
                });
            },
        );
    }
}
