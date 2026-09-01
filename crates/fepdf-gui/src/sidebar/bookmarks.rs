use crate::locale::LocaleManager;

#[allow(dead_code)]
pub fn show_bookmarks(
    ui: &mut egui::Ui,
    total_pages: usize,
    locale_mgr: &LocaleManager,
    active_lang: &str,
) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(locale_mgr.tr(active_lang, "bookmarks_title")).strong().size(13.0),
        );
        ui.add_space(6.0);

        if total_pages == 0 {
            ui.label(egui::RichText::new(locale_mgr.tr(active_lang, "no_doc_loaded")).weak());
        } else {
            egui::ScrollArea::vertical().id_salt("bookmarks_scroll").show(ui, |ui| {
                for i in 1..=total_pages.min(5) {
                    if ui.selectable_label(false, format!("🔖 Chapter {i}: Page {i}")).clicked() {
                        // Navigator target page trigger
                    }
                }
                if total_pages > 5 {
                    ui.label(egui::RichText::new("...").weak());
                }
            });
        }
    });
}
