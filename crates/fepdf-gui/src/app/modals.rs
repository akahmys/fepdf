//! Modal dialogs and overlay windows for `FepdfApp`.

use super::FepdfApp;

impl FepdfApp {
    pub(crate) fn show_export_wizard_window(&mut self, ctx: &egui::Context) {
        crate::export_wizard::ExportWizard::show(self, ctx);
    }

    /// Bulk front end for the redaction pipeline: pattern-matched text spans are pushed
    /// into the same `RedactionManager::zones` the manual brush fills, so the export
    /// wizard's "burn redactions" path consumes both identically.
    pub(crate) fn show_redaction_studio_window(&mut self, ctx: &egui::Context) {
        let Self {
            redaction_studio_panel,
            raw_texts,
            page_spans,
            redaction_manager,
            locale_mgr,
            active_language,
            show_redaction_studio,
            ..
        } = self;

        let title = locale_mgr.tr(active_language, "redaction_studio_title");
        egui::Window::new(format!("🔍 {title}"))
            .open(show_redaction_studio)
            .resizable(true)
            .default_width(420.0)
            .default_height(360.0)
            .show(ctx, |ui| {
                redaction_studio_panel.show(
                    ui,
                    raw_texts,
                    page_spans,
                    redaction_manager,
                    locale_mgr,
                    active_language,
                );
            });
    }

    pub(crate) fn show_about_modal_window(&mut self, ctx: &egui::Context) {
        // RR-15 Limit: GUI - Displays the application metadata/about modal
        if self.show_about_modal {
            let mut show_about = true;
            let about_title = self.locale_mgr.tr(&self.active_language, "about_title");
            egui::Window::new(about_title)
                .open(&mut show_about)
                .resizable(false)
                .collapsible(false)
                .default_width(320.0)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(
                                self.locale_mgr.tr(&self.active_language, "about_app_name"),
                            )
                            .strong()
                            .size(18.0),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{} 0.1.0",
                                self.locale_mgr.tr(&self.active_language, "about_version")
                            ))
                            .weak(),
                        );
                        ui.add_space(8.0);
                        ui.label(self.locale_mgr.tr(&self.active_language, "about_description"));
                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(
                                self.locale_mgr.tr(&self.active_language, "about_third_party"),
                            )
                            .strong(),
                        );
                        ui.add_space(4.0);
                    });

                    egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        let credits = [
                            ("pdf-writer", "Apache-2.0 License", "PDF object serialization"),
                            ("vello", "Apache-2.0 / MIT", "GPU vector graphics"),
                            ("egui / eframe", "MIT / Apache-2.0", "GUI library"),
                            ("Lucide Icons", "ISC License", "Icon font asset"),
                        ];
                        for (name, license, purpose) in credits {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(name).strong());
                                ui.label(format!("({license})"));
                            });
                            ui.label(egui::RichText::new(purpose).weak());
                            ui.add_space(4.0);
                        }
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        if ui
                            .button(self.locale_mgr.tr(&self.active_language, "about_close"))
                            .clicked()
                        {
                            self.show_about_modal = false;
                        }
                    });
                });
            if !show_about {
                self.show_about_modal = false;
            }
        }
    }

    pub(crate) fn render_overlay_windows(&mut self, ctx: &egui::Context) {
        // RR-15 Limit: GUI - Renders various overlay windows, tool wizards, and popup alerts
        if self.show_export_wizard {
            self.show_export_wizard_window(ctx);
        }

        if self.show_redaction_studio {
            self.show_redaction_studio_window(ctx);
        }

        // Show Command Palette window overlay
        crate::command_palette::CommandPalette::show(self, ctx);

        // Show interactive Create Semantic Tag popup dialog on visual tag selector brush highlights
        if let Some(req) = self.selection_manager.pending_tag_request.clone() {
            let mut show_popup = true;
            let popup_title = self.locale_mgr.tr(&self.active_language, "tag_popup_title");
            egui::Window::new(popup_title)
                .open(&mut show_popup)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(self.locale_mgr.tr(&self.active_language, "tag_popup_selected"));
                    ui.group(|ui| {
                        ui.label(&req.text);
                    });
                    ui.add_space(5.0);
                    ui.label(self.locale_mgr.tr(&self.active_language, "tag_popup_instruction"));

                    ui.horizontal(|ui| {
                        if ui.button("H1").clicked() {
                            self.inject_tag_to_tree("H1", &req);
                            self.selection_manager.pending_tag_request = None;
                        }
                        if ui.button("H2").clicked() {
                            self.inject_tag_to_tree("H2", &req);
                            self.selection_manager.pending_tag_request = None;
                        }
                        if ui.button("P").clicked() {
                            self.inject_tag_to_tree("P", &req);
                            self.selection_manager.pending_tag_request = None;
                        }
                        if ui.button("Figure").clicked() {
                            self.inject_tag_to_tree("Figure", &req);
                            self.selection_manager.pending_tag_request = None;
                        }
                    });

                    if ui
                        .button(self.locale_mgr.tr(&self.active_language, "tag_popup_cancel"))
                        .clicked()
                    {
                        self.selection_manager.pending_tag_request = None;
                    }
                });
            if !show_popup {
                self.selection_manager.pending_tag_request = None;
            }
        }

        // Show Settings Modal
        if self.show_settings_modal {
            let mut show_settings = true;
            let title = self.locale_mgr.tr(&self.active_language, "settings_title");
            egui::Window::new(title)
                .open(&mut show_settings)
                .resizable(false)
                .collapsible(false)
                .default_width(280.0)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                self.locale_mgr
                                    .tr(&self.active_language, "settings_language_label"),
                            );
                            let current_lang = self.active_language.clone();
                            egui::ComboBox::from_id_salt("settings_lang_combobox")
                                .selected_text(&current_lang)
                                .show_ui(ui, |ui| {
                                    for lang in self.locale_mgr.available_languages() {
                                        ui.selectable_value(
                                            &mut self.active_language,
                                            lang.clone(),
                                            lang,
                                        );
                                    }
                                });
                        });

                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.vertical_centered(|ui| {
                            if ui
                                .button(self.locale_mgr.tr(&self.active_language, "settings_close"))
                                .clicked()
                            {
                                self.show_settings_modal = false;
                            }
                        });
                    });
                });
            if !show_settings {
                self.show_settings_modal = false;
            }
        }

        // Show About Modal
        self.show_about_modal_window(ctx);
    }
}
