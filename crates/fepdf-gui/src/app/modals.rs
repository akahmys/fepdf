//! Modal dialogs and overlay windows for `FepdfApp`.

use super::FepdfApp;
use super::theme::{size, space, text};

/// The password prompt's words, read from the locale before the document is borrowed.
struct PasswordWords {
    untitled: String,
    refused: String,
    hint: String,
    unlock: String,
    close: String,
}

/// What the reader did with the password prompt this frame.
enum Answer {
    /// Still typing, or the window was left alone.
    Waiting,
    /// Try what is in the field.
    Unlock,
    /// Close the document rather than unlock it.
    GiveUp,
}

/// What the reader can do about edits that have not been written anywhere.
///
/// **Three, because the warning is about the one that used to be missing.** It said
/// the document had edits that had not been exported and then offered to keep it open
/// or to throw them away — so the reader had to dismiss it, find the export button
/// behind it, and close again. The act it names is now one of the answers.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CloseChoice {
    /// Cancel the close and leave everything as it is.
    Keep,
    /// Cancel the close and open the export wizard.
    Export,
    /// Close, losing the edits.
    Discard,
}

impl FepdfApp {
    pub(crate) fn show_export_wizard_window(&mut self, ctx: &egui::Context) {
        crate::export_wizard::ExportWizard::show(self, ctx);
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

                    // **No height of its own.** It was a scroll area capped at 150
                    // points, which fitted three of the four credits and clipped the
                    // fourth with no scrollbar to say so — a licence notice quietly
                    // missing one of the licences it is there to give. Four short entries
                    // do not need to scroll; the window grows to hold them.
                    ui.vertical(|ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        // **The name and the licence are identifiers; the purpose is
                        // prose.** `Apache-2.0` is the same string in every language and
                        // translating it would name a different licence, which is the
                        // same reason UI-5 exempts `H1` and `P`. What each crate is *for*
                        // was English-only, in an array UI-5 does not look inside.
                        let credits = [
                            ("pdf-writer", "Apache-2.0 License", "about_use_pdf_writer"),
                            ("vello", "Apache-2.0 / MIT", "about_use_vello"),
                            ("egui / eframe", "MIT / Apache-2.0", "about_use_egui"),
                            ("Lucide Icons", "ISC License", "about_use_lucide"),
                        ];
                        for (name, license, purpose) in credits {
                            let purpose = self.locale_mgr.tr(&self.active_language, purpose);
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
        // The words come out before `locked` is borrowed, because they are read from the
        // locale and `locked` is a field of the same struct.
        let words = PasswordWords {
            untitled: self.tr("password_untitled"),
            refused: self.tr("password_refused"),
            hint: self.tr("password_hint"),
            unlock: self.tr("password_unlock"),
            close: self.tr("password_close"),
        };
        let Some(locked) = self.locked.as_mut() else { return };
        match Self::password_dialog(ctx, locked, &words) {
            Answer::Waiting => {}
            Answer::GiveUp => self.locked = None,
            Answer::Unlock => self.retry_with_password(ctx),
        }
    }

    /// Sends the document back to the worker with what the reader typed.
    fn retry_with_password(&mut self, ctx: &egui::Context) {
        let Some(locked) = self.locked.take() else { return };
        self.is_loading = true;
        self.loading_message = self.tr("busy_unlocking");
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
    fn password_dialog(
        ctx: &egui::Context,
        locked: &mut super::LockedDocument,
        words: &PasswordWords,
    ) -> Answer {
        let name = locked.name.clone().unwrap_or_else(|| words.untitled.clone());
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
                        egui::RichText::new(&words.refused).color(super::theme::colors::note::WARN),
                    );
                    ui.add_space(space::ITEM);
                }

                let field = ui.add(
                    egui::TextEdit::singleline(&mut locked.attempt)
                        .password(true)
                        .hint_text(&words.hint)
                        .desired_width(f32::INFINITY),
                );
                field.request_focus();
                if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    answer = Answer::Unlock;
                }

                ui.add_space(space::SECTION);
                ui.horizontal(|ui| {
                    if ui.button(&words.unlock).clicked() {
                        answer = Answer::Unlock;
                    }
                    if ui.button(&words.close).clicked() {
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
        let mut chosen = None;

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
                // Left to right by what each costs: nothing, a detour, the edits.
                ui.horizontal(|ui| {
                    if ui.button(tr("close_keep")).clicked() {
                        chosen = Some(CloseChoice::Keep);
                    }
                    if ui.button(tr("close_export")).clicked() {
                        chosen = Some(CloseChoice::Export);
                    }
                    if ui
                        .button(
                            egui::RichText::new(tr("close_discard"))
                                .color(super::theme::colors::note::FAIL),
                        )
                        .clicked()
                    {
                        chosen = Some(CloseChoice::Discard);
                    }
                });
                ui.add_space(space::ITEM);
            });

        if let Some(chosen) = chosen {
            self.act_on_close_choice(chosen, ctx);
        }
    }

    /// Carries out what the reader chose. Every one of the three cancels the dialog;
    /// only the last of them lets the window go.
    fn act_on_close_choice(&mut self, chosen: CloseChoice, ctx: &egui::Context) {
        self.confirming_close = false;
        match chosen {
            CloseChoice::Keep => {}
            CloseChoice::Export => self.open_export_wizard(),
            CloseChoice::Discard => {
                self.close_confirmed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Every window that floats over the document, in the order they stack.
    ///
    /// **A list, and only a list.** Two of the seven used to be written out here — the tag
    /// popup and the settings window, about ninety lines between them — so the one place
    /// that says what the windows *are* was also the place two of them lived, and it
    /// carried a length exemption for the privilege.
    pub(crate) fn render_overlay_windows(&mut self, ctx: &egui::Context) {
        if self.show_export_wizard {
            self.show_export_wizard_window(ctx);
        }
        crate::command_palette::CommandPalette::show(self, ctx);
        self.show_tag_popup(ctx);
        self.show_settings_window(ctx);
        self.show_about_modal_window(ctx);
        self.show_close_confirmation(ctx);
        // Last, so it draws over everything: a locked document has nothing behind this
        // worth interacting with.
        self.show_password_prompt(ctx);
    }

    /// Asks what a brushed selection should be tagged as.
    fn show_tag_popup(&mut self, ctx: &egui::Context) {
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
    }

    /// The settings window.
    fn show_settings_window(&mut self, ctx: &egui::Context) {
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
    }
}
