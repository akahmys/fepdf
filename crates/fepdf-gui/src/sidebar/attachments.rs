use crate::locale::LocaleManager;

#[allow(dead_code)]
pub fn show_attachments(ui: &mut egui::Ui, locale_mgr: &LocaleManager, active_lang: &str) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(locale_mgr.tr(active_lang, "attachments_title"))
                .strong()
                .size(13.0),
        );
        ui.add_space(6.0);

        ui.label(egui::RichText::new(locale_mgr.tr(active_lang, "attachments_none")).weak());
        ui.add_space(8.0);
        if ui.button(locale_mgr.tr(active_lang, "attachments_add")).clicked() {
            // Future file dialog attachment trigger
        }
    });
}
