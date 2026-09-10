//! Modal dialogs and overlay windows for `FepdfApp`.

use super::FepdfApp;
use super::theme::{size, space, text};

/// What the reader did with the password prompt this frame.
enum Answer {
    /// Still typing, or the window was left alone.
    Waiting,
    /// Try what is in the field.
    Unlock,
    /// Close the document rather than unlock it.
    GiveUp,
}

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
            .default_width(size::TABLE_W)
            .default_height(size::TABLE_W * 0.75)
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
                .default_width(size::FORM_W)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(
                                self.locale_mgr.tr(&self.active_language, "about_app_name"),
                            )
                            .strong()
                            .size(text::TITLE),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{} 0.1.0",
                                self.locale_mgr.tr(&self.active_language, "about_version")
                            ))
                            .weak(),
                        );
                        ui.add_space(space::GROUP);
                        ui.label(self.locale_mgr.tr(&self.active_language, "about_description"));
                        ui.add_space(space::SECTION);
                        ui.separator();
                        ui.add_space(space::GROUP);
                        ui.label(
                            egui::RichText::new(
                                self.locale_mgr.tr(&self.active_language, "about_third_party"),
                            )
                            .strong(),
                        );
                        ui.add_space(space::ITEM);
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
                            ui.add_space(space::ITEM);
                        }
                    });

                    ui.add_space(space::GROUP);
                    ui.separator();
                    ui.add_space(space::GROUP);
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

    /// Asks for the password of a document that opened encrypted (7.6.4.4).
    ///
    /// **Modal, and it has to be.** The engine reads a locked document's structure and
    /// not its content, so every panel behind this would show a page count and a
    /// catalogue over blank pages. Answering the question is the only thing to do next.
    ///
    /// There is no Cancel that leaves the document half-open: closing it is what cancel
    /// means, and the reader keeps whatever was open before.
    pub(crate) fn show_password_prompt(&mut self, ctx: &egui::Context) {
        let Some(locked) = self.locked.as_mut() else { return };
        match Self::password_dialog(ctx, locked) {
            Answer::Waiting => {}
            Answer::GiveUp => self.locked = None,
            Answer::Unlock => self.retry_with_password(ctx),
        }
    }

    /// Sends the document back to the worker with what the reader typed.
    fn retry_with_password(&mut self, ctx: &egui::Context) {
        let Some(locked) = self.locked.take() else { return };
        self.is_loading = true;
        self.loading_message = "Unlocking...".to_string();
        let _ = self.tx_worker.send(crate::worker::WorkerRequest::Open {
            data: locked.data,
            name: locked.name,
            password: Some(locked.attempt),
        });
        ctx.request_repaint();
    }

    /// Draws the prompt and says what the reader did with it.
    ///
    /// **Modal, and it has to be.** The engine reads a locked document's structure and not
    /// its content, so every panel behind this would show a page count and a catalogue
    /// over blank pages. Answering the question is the only thing to do next, and there is
    /// no Cancel that leaves the document half-open: closing it is what cancel means.
    fn password_dialog(ctx: &egui::Context, locked: &mut super::LockedDocument) -> Answer {
        let name = locked.name.clone().unwrap_or_else(|| "This document".to_string());
        let mut answer = Answer::Waiting;

        egui::Window::new("🔒")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(size::FORM_W)
            .show(ctx, |ui| {
                ui.add_space(space::GROUP);
                ui.heading(&name);
                ui.label(egui::RichText::new(&locked.method).weak());
                ui.add_space(space::GROUP);

                if locked.refused {
                    ui.label(
                        egui::RichText::new("That password did not unlock it.")
                            .color(super::theme::colors::note::WARN),
                    );
                    ui.add_space(space::ITEM);
                }

                let field = ui.add(
                    egui::TextEdit::singleline(&mut locked.attempt)
                        .password(true)
                        .hint_text("Password")
                        .desired_width(f32::INFINITY),
                );
                field.request_focus();
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    answer = Answer::Unlock;
                }

                ui.add_space(space::SECTION);
                ui.horizontal(|ui| {
                    if ui.button("Unlock").clicked() {
                        answer = Answer::Unlock;
                    }
                    if ui.button("Close the document").clicked() {
                        answer = Answer::GiveUp;
                    }
                });
                ui.add_space(space::ITEM);
            });

        answer
    }

    /// Asks before a close takes unexported edits with it.
    ///
    /// **No title bar and no `open`**, for the reason the password prompt has neither:
    /// the question is the only thing to answer, and a dialog about losing work that can
    /// be dismissed by missing it is not a guard. `Keep it open` is the default and the
    /// first button, because it is the reversible one (principle P1).
    fn show_close_confirmation(&mut self, ctx: &egui::Context) {
        if !self.confirming_close {
            return;
        }
        let tr = |key: &str| self.locale_mgr.tr(&self.active_language, key);
        let mut decision = None;

        egui::Window::new("close_confirmation")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(size::FORM_W)
            .show(ctx, |ui| {
                ui.add_space(space::GROUP);
                ui.heading(egui::RichText::new(tr("close_edited_title")).size(text::HEAD));
                ui.add_space(space::GROUP);
                ui.label(egui::RichText::new(tr("close_edited_body")).size(text::BODY));
                ui.add_space(space::PANE);
                ui.horizontal(|ui| {
                    if ui.button(tr("close_keep")).clicked() {
                        decision = Some(false);
                    }
                    if ui
                        .button(
                            egui::RichText::new(tr("close_discard"))
                                .color(super::theme::colors::note::FAIL),
                        )
                        .clicked()
                    {
                        decision = Some(true);
                    }
                });
                ui.add_space(space::ITEM);
            });

        match decision {
            Some(true) => {
                self.confirming_close = false;
                self.close_confirmed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(false) => self.confirming_close = false,
            None => {}
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
                    ui.add_space(space::ITEM);
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
                .default_width(size::FORM_W)
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

                        ui.add_space(space::SECTION);
                        ui.separator();
                        ui.add_space(space::GROUP);
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

        crate::document_tools::show(self, ctx);

        // Show About Modal
        self.show_about_modal_window(ctx);

        self.show_close_confirmation(ctx);

        // Last, so it draws over everything: a locked document has nothing behind this
        // worth interacting with.
        self.show_password_prompt(ctx);
    }
}
