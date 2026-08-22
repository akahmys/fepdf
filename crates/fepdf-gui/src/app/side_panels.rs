//! Left navigation and context side panels for `FepdfApp`.

use super::FepdfApp;

impl FepdfApp {
    pub(crate) fn render_left_side_panels(&mut self, ui: &mut egui::Ui) {
        // RR-15 Limit: GUI - Renders left sidebar icon bar, context panels, and inspector panel
        // 1. Left Icon Bar (Vertical column, full height)
        let ctx = ui.ctx().clone();

        egui::Panel::left("left_icon_bar").resizable(false).default_size(50.0).show_inside(
            ui,
            |ui| {
                ui.vertical_centered(|ui| {
                    ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 8.0);
                    ui.add_space(8.0);

                    // 1. Load PDF
                    let load_btn = egui::Button::new(egui::RichText::new("\u{e247}").size(16.0))
                        .min_size(egui::vec2(36.0, 36.0));
                    if ui
                        .add(load_btn)
                        .on_hover_text(
                            self.locale_mgr.tr(&self.active_language, "tooltip_load_pdf"),
                        )
                        .clicked()
                        && let Some(p) =
                            rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file()
                    {
                        if self.total_pages > 0 {
                            if let Ok(exe) = std::env::current_exe() {
                                let _ = std::process::Command::new(exe).arg(p).spawn();
                            }
                        } else {
                            self.open_file(p, &ctx);
                        }
                    }

                    // 2. Export PDF & Inspector (Enabled only when doc loaded)
                    let has_doc = self.total_pages > 0;
                    ui.add_enabled_ui(has_doc, |ui| {
                        let export_btn =
                            egui::Button::new(egui::RichText::new("\u{e14d}").size(16.0))
                                .min_size(egui::vec2(36.0, 36.0));
                        if ui
                            .add(export_btn)
                            .on_hover_text(
                                self.locale_mgr.tr(&self.active_language, "tooltip_export_pdf"),
                            )
                            .clicked()
                        {
                            self.show_export_wizard = true;
                        }

                        let mut inspector_btn =
                            egui::Button::new(egui::RichText::new("\u{e151}").size(16.0))
                                .min_size(egui::vec2(36.0, 36.0));
                        if self.show_inspector {
                            inspector_btn = inspector_btn
                                .stroke(egui::Stroke::new(1.5_f32, egui::Color32::from_gray(80)));
                        }
                        if ui
                            .add(inspector_btn)
                            .on_hover_text(
                                self.locale_mgr.tr(&self.active_language, "tooltip_inspector"),
                            )
                            .clicked()
                        {
                            self.show_inspector = !self.show_inspector;
                        }
                    });

                    ui.separator();

                    // 3. 7 Sidebar Navigation Tabs (DocumentInfo, Bookmarks, Attachments, Structure, Properties, AltText, Audit)
                    self.sidebar_panel.show_icon_bar(ui, &self.locale_mgr, &self.active_language);

                    let current_height = ui.available_height();
                    if current_height > 100.0 {
                        ui.add_space(current_height - 90.0);
                    }

                    // 4. Settings Button
                    let settings_btn =
                        egui::Button::new(egui::RichText::new("\u{e30b}").size(16.0))
                            .min_size(egui::vec2(36.0, 36.0));
                    if ui
                        .add(settings_btn)
                        .on_hover_text(
                            self.locale_mgr.tr(&self.active_language, "tooltip_settings"),
                        )
                        .clicked()
                    {
                        self.show_settings_modal = true;
                    }

                    // 5. About (Help) Button
                    let about_btn = egui::Button::new(egui::RichText::new("\u{e082}").size(16.0))
                        .min_size(egui::vec2(36.0, 36.0));
                    if ui
                        .add(about_btn)
                        .on_hover_text(self.locale_mgr.tr(&self.active_language, "tooltip_about"))
                        .clicked()
                    {
                        self.show_about_modal = true;
                    }
                });
            },
        );

        let locale_mgr = &self.locale_mgr;
        let active_lang = &self.active_language;

        // 2. Context Panel (resizable, automatic size adjusting)
        if self.sidebar_panel.context_panel_open {
            egui::Panel::left("context_panel")
                .resizable(true)
                .show_separator_line(true)
                .size_range(260.0..=900.0)
                .show_inside(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    egui::Frame::NONE.inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                        self.sidebar_panel.show(
                            ui,
                            &mut self.ust_registry,
                            &self.tx_worker,
                            &self.pdf_name,
                            self.total_pages,
                            &self.doc_metadata,
                            self.doc_file_size,
                            &self.doc_version,
                            &self.doc_security_method,
                            self.doc_permissions,
                            &self.doc_page_sizes,
                            &self.doc_fonts,
                            &self.layers,
                            locale_mgr,
                            active_lang,
                        );
                    });
                });
        }

        // 3. Arlington Dictionary Inspector (Left side, next to context panel)
        if self.show_inspector {
            let selected_tag = self.ust_registry.selected_node_id.and_then(|id| {
                if let Some(ref root) = self.ust_registry.root {
                    crate::sidebar::USTRegistry::find_node_by_id_recursive(root, id)
                        .map(|n| n.tag.as_str())
                } else {
                    None
                }
            });
            egui::Panel::left("inspector_panel")
                .resizable(true)
                .show_separator_line(true)
                .default_size(280.0)
                .size_range(200.0..=450.0)
                .show_inside(ui, |ui| {
                    egui::Frame::NONE.inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                        self.arlington_inspector.show(ui, selected_tag, locale_mgr, active_lang);
                    });
                });
        }
    }
}
