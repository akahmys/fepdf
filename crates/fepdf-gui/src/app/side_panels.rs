//! The left icon rail, and the drawer that opens beside it.

use super::FepdfApp;
use super::icons::{glyph, icon_button, icon_button_disabled};
use super::theme::{size, space, text};
use crate::sidebar::ActiveDrawer;

impl FepdfApp {
    /// The slim vertical rail docked to the leftmost edge.
    ///
    /// **One button constructor for all ten.** The file pair used to be drawn by a
    /// second one that painted a background only on hover, so the same column held two
    /// kinds of object.
    pub(crate) fn render_left_icon_bar(&mut self, ui: &mut egui::Ui) {
        // RR-15 Limit: GUI - Left vertical icon bar for file ops, view modes, and drawer toggles
        let ctx = ui.ctx().clone();
        let has_doc = self.total_pages > 0;

        egui::Panel::left("left_icon_bar").resizable(false).exact_size(size::RAIL).show_inside(
            ui,
            |ui| {
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = space::ITEM;
                    ui.add_space(space::GROUP);

                    self.file_buttons(ui, has_doc, &ctx);

                    ui.add_space(space::GROUP);
                    ui.separator();
                    ui.add_space(space::GROUP);

                    self.drawer_buttons(ui, has_doc);

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        ui.add_space(space::GROUP);
                        self.utility_buttons(ui);
                        ui.add_space(space::GROUP);
                        ui.separator();
                    });
                });
            },
        );
    }

    /// Opening a document, and writing one out.
    fn file_buttons(&mut self, ui: &mut egui::Ui, has_doc: bool, ctx: &egui::Context) {
        let tip_import = self.locale_mgr.tr(&self.active_language, "tooltip_import_pdf");
        let tip_export = self.locale_mgr.tr(&self.active_language, "tooltip_export_pdf");

        if ui.add(icon_button(glyph::OPEN, false)).on_hover_text(tip_import).clicked()
            && let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file()
        {
            if has_doc {
                if let Ok(exe) = std::env::current_exe()
                    && let Err(e) = std::process::Command::new(exe).arg(p).spawn()
                {
                    log::warn!("the external viewer would not start: {e}");
                }
            } else {
                self.open_file(p, ctx);
            }
        }

        // Drawn rather than `add_enabled`: egui fades a disabled widget towards its
        // background, and the fade took this button to 1.23:1 — below the point at which
        // it says the feature exists at all. `icon_button_disabled` holds 3.20:1.
        let export = if has_doc {
            icon_button(glyph::EXPORT, false)
        } else {
            icon_button_disabled(glyph::EXPORT)
        };
        if ui.add(export).on_hover_text(tip_export).clicked() && has_doc {
            self.show_export_wizard = true;
        }
    }

    /// The five drawers, one of which may be open.
    fn drawer_buttons(&mut self, ui: &mut egui::Ui, has_doc: bool) {
        let entries = [
            (ActiveDrawer::DocumentInfo, glyph::INFO, "tab_doc_info_decisions"),
            (ActiveDrawer::WhatItDoes, glyph::SURVEY, "tooltip_what_it_does"),
            (ActiveDrawer::Accessibility, glyph::STRUCTURE, "tab_accessibility"),
            (ActiveDrawer::Redaction, glyph::REDACT, "tooltip_redact_brush"),
            (ActiveDrawer::Caliper, glyph::CALIPER, "tooltip_caliper_brush"),
            (ActiveDrawer::Tools, glyph::TOOLS, "tools_title"),
        ];

        for (drawer, icon, key) in entries {
            let is_open = self.active_drawer == drawer;
            let tip = self.locale_mgr.tr(&self.active_language, key);
            let button =
                if has_doc { icon_button(icon, is_open) } else { icon_button_disabled(icon) };
            if ui.add(button).on_hover_text(tip).clicked() && has_doc {
                self.active_drawer = if is_open { ActiveDrawer::None } else { drawer };
                self.caliper_tool.is_active = !is_open && drawer == ActiveDrawer::Caliper;
            }
        }
    }

    /// The bottom cluster, added bottom-up: the palette, settings, about.
    fn utility_buttons(&mut self, ui: &mut egui::Ui) {
        let tip_about = self.locale_mgr.tr(&self.active_language, "tooltip_about");
        let tip_settings = self.locale_mgr.tr(&self.active_language, "tooltip_settings");

        if ui.add(icon_button(glyph::ABOUT, false)).on_hover_text(tip_about).clicked() {
            self.show_about_modal = true;
        }
        if ui.add(icon_button(glyph::SETTINGS, false)).on_hover_text(tip_settings).clicked() {
            self.show_settings_modal = true;
        }
        if ui
            .add(icon_button(glyph::PALETTE, self.show_command_palette))
            .on_hover_text(self.tr("tooltip_command_palette"))
            .clicked()
        {
            self.show_command_palette = !self.show_command_palette;
        }
    }

    /// The name of the drawer that is open.
    fn drawer_title(&self) -> String {
        let tr = |key: &str| self.locale_mgr.tr(&self.active_language, key);
        match self.active_drawer {
            ActiveDrawer::None => String::new(),
            ActiveDrawer::DocumentInfo => tr("tab_doc_info_decisions"),
            ActiveDrawer::WhatItDoes => tr("tab_what_it_does"),
            ActiveDrawer::Accessibility => tr("tab_accessibility"),
            ActiveDrawer::Redaction => tr("tooltip_redact_brush"),
            ActiveDrawer::Caliper => tr("tooltip_caliper_brush"),
            ActiveDrawer::Tools => tr("tools_title"),
        }
    }

    /// The utility drawer, when one is open.
    pub(crate) fn render_side_drawer(&mut self, ui: &mut egui::Ui) {
        // RR-15 Limit: GUI - Render active utility drawer on the left side of the main pane
        if self.active_drawer == ActiveDrawer::None {
            return;
        }

        // **The title is taken before the panel opens**, rather than through a borrow of
        // the locale that lives as long as the drawer does: `ActiveDrawer::Tools` draws
        // through `&mut FepdfApp`, and a shared borrow of one of its fields held across
        // that is a borrow of the whole thing.
        let title = self.drawer_title();
        let close_label = self.tr("btn_close");

        egui::Panel::left("active_side_drawer")
            .resizable(true)
            .show_separator_line(true)
            .default_size(size::DRAWER_W)
            .size_range(size::DRAWER_MIN..=size::DRAWER_MAX)
            .show_inside(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);

                ui.horizontal(|ui| {
                    ui.heading(egui::RichText::new(&title).size(text::HEAD));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(icon_button(glyph::CLOSE, false))
                            .on_hover_text(&close_label)
                            .clicked()
                        {
                            self.active_drawer = ActiveDrawer::None;
                            self.caliper_tool.is_active = false;
                        }
                    });
                });
                ui.add_space(space::ITEM);
                ui.separator();
                ui.add_space(space::PANE);

                egui::ScrollArea::vertical().id_salt("side_drawer_scroll").show(
                    ui,
                    |ui| match self.active_drawer {
                        ActiveDrawer::None => {}
                        ActiveDrawer::WhatItDoes => {
                            let locale = &self.locale_mgr;
                            let lang = &self.active_language;
                            let tx = self.tx_worker.clone();
                            crate::sidebar::what_it_does::show(
                                ui,
                                &mut self.survey,
                                self.doc_security_method.as_ref(),
                                self.doc_permissions,
                                &|key| locale.tr(lang, key),
                                &|| {
                                    let _ = tx.send(crate::worker::WorkerRequest::Survey);
                                },
                            );
                        }
                        ActiveDrawer::DocumentInfo => {
                            crate::sidebar::document_info::show_document_info(
                                ui,
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
                                &self.doc_decisions,
                                &self.locale_mgr,
                                &self.active_language,
                            );
                        }
                        ActiveDrawer::Accessibility => {
                            self.sidebar_panel.show_accessibility_unified(
                                ui,
                                &mut self.ust_registry,
                                &self.tx_worker,
                                &self.locale_mgr,
                                &self.active_language,
                            );
                        }
                        ActiveDrawer::Redaction => {
                            self.redaction_studio_panel.show(
                                ui,
                                &self.raw_texts,
                                &self.page_spans,
                                &mut self.redaction_manager,
                                &self.locale_mgr,
                                &self.active_language,
                            );
                        }
                        ActiveDrawer::Caliper => {
                            self.caliper_tool.is_active = true;
                            self.caliper_tool.show_panel(ui);
                        }
                        ActiveDrawer::Tools => crate::document_tools::show(self, ui),
                    },
                );
            });
    }
}
