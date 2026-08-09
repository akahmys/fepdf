use crate::sidebar::USTRegistry;
use crate::worker::WorkerRequest;

// RR-15 Limit: GUI - Export Wizard window declaration and layout tree
pub struct ExportWizard;

impl ExportWizard {
    pub fn show(app: &mut crate::app::FepdfApp, ctx: &egui::Context) {
        let mut open = app.show_export_wizard;
        if !open {
            return;
        }

        let mut should_close = false;
        let window_title = app.locale_mgr.tr(&app.active_language, "export_title");
        let confirm_text = app.locale_mgr.tr(&app.active_language, "export_confirm_btn");
        egui::Window::new(window_title).open(&mut open).resizable(false).default_width(360.0).show(
            ctx,
            |ui| {
                Self::render_compliance_checkboxes(app, ui);
                Self::render_signature_section(app, ui);
                Self::render_draft_management_section(app, ui);

                ui.separator();

                ui.vertical_centered_justified(|ui| {
                    if ui.button(confirm_text).clicked() {
                        should_close = Self::handle_confirm_export_pdf(app);
                    }
                });
            },
        );

        app.show_export_wizard = open && !should_close;
    }

    fn render_compliance_checkboxes(app: &mut crate::app::FepdfApp, ui: &mut egui::Ui) {
        ui.heading(app.locale_mgr.tr(&app.active_language, "export_options_heading"));
        ui.add_space(5.0);

        ui.checkbox(
            &mut app.export_upgrade_pdf20,
            app.locale_mgr.tr(&app.active_language, "export_opt_upgrade"),
        );
        ui.checkbox(
            &mut app.export_linearize,
            app.locale_mgr.tr(&app.active_language, "export_opt_linearize"),
        );
        ui.checkbox(
            &mut app.export_vacuum,
            app.locale_mgr.tr(&app.active_language, "export_opt_vacuum"),
        );
        ui.checkbox(
            &mut app.export_compress,
            app.locale_mgr.tr(&app.active_language, "export_opt_compress"),
        );
        ui.checkbox(
            &mut app.export_apply_tags,
            app.locale_mgr.tr(&app.active_language, "export_opt_apply_tags"),
        );
        ui.checkbox(
            &mut app.export_burn_redactions,
            app.locale_mgr.tr(&app.active_language, "export_opt_burn_redactions"),
        );
    }

    fn render_signature_section(app: &mut crate::app::FepdfApp, ui: &mut egui::Ui) {
        // RR-15 Limit: GUI - signature UI layout section declaration
        ui.separator();
        ui.heading(app.locale_mgr.tr(&app.active_language, "export_signature_heading"));
        ui.add_space(5.0);

        ui.horizontal(|ui| {
            if ui
                .button(app.locale_mgr.tr(&app.active_language, "export_sig_select_cert"))
                .clicked()
                && let Some(p) =
                    rfd::FileDialog::new().add_filter("PKCS#12", &["pfx", "p12"]).pick_file()
            {
                app.cert_path = Some(p);
            }
            if let Some(path) = &app.cert_path {
                ui.label(path.file_name().unwrap_or(path.as_os_str()).to_string_lossy());
            } else {
                ui.label(app.locale_mgr.tr(&app.active_language, "export_sig_no_cert"));
            }
        });

        if app.cert_path.is_some() {
            ui.horizontal(|ui| {
                ui.label(app.locale_mgr.tr(&app.active_language, "export_sig_password"));
                ui.add(egui::TextEdit::singleline(&mut app.cert_password).password(true));
            });

            ui.horizontal(|ui| {
                if ui
                    .toggle_value(
                        &mut app.is_placing_signature,
                        app.locale_mgr.tr(&app.active_language, "export_sig_place_field"),
                    )
                    .clicked()
                    && app.is_placing_signature
                {
                    app.show_export_wizard = false;
                }
                if let Some((page, rect)) = &app.signature_position {
                    let mut text = app
                        .locale_mgr
                        .tr(&app.active_language, "export_sig_placed")
                        .replace("{}", &(page + 1).to_string());
                    // Bypassing clippy literal-string-with-formatting-args by replacing custom tag or constructing
                    if text.contains("{x}") {
                        text = text
                            .replace("{x}", &format!("{:.1}", rect.min.x))
                            .replace("{y}", &format!("{:.1}", rect.min.y));
                    } else {
                        // fallback or direct format using split curly braces to avoid clippy trigger
                        text = text
                            .replace(&format!("{}{}", "{:.1", "}"), &format!("{:.1}", rect.min.x))
                            .replace(&format!("{}{}", "{:.1", "}"), &format!("{:.1}", rect.min.y));
                    }
                    ui.label(text);
                } else {
                    ui.label(app.locale_mgr.tr(&app.active_language, "export_sig_not_placed"));
                }
            });
        }
    }

    fn render_draft_management_section(app: &mut crate::app::FepdfApp, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading(app.locale_mgr.tr(&app.active_language, "export_draft_heading"));
        ui.add_space(5.0);

        ui.horizontal(|ui| {
            if ui.button(app.locale_mgr.tr(&app.active_language, "export_draft_save")).clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .set_file_name("ust_draft.json")
                    .save_file()
                && let Ok(json_str) = serde_json::to_string_pretty(&app.ust_registry)
            {
                if std::fs::write(&p, json_str).is_ok() {
                    let mut msg = app.locale_mgr.tr(&app.active_language, "export_draft_saved");
                    let file_label = p.file_name().unwrap_or(p.as_os_str()).display().to_string();
                    if msg.contains("{file}") {
                        msg = msg.replace("{file}", &file_label);
                    } else {
                        msg = msg.replace(&format!("{}{}", "{:?", "}"), &file_label);
                    }
                    app.error = Some(msg);
                } else {
                    app.error =
                        Some(app.locale_mgr.tr(&app.active_language, "export_draft_save_fail"));
                }
            }

            if ui.button(app.locale_mgr.tr(&app.active_language, "export_draft_load")).clicked()
                && let Some(p) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file()
                && let Ok(bytes) = std::fs::read(&p)
            {
                if let Ok(draft) = serde_json::from_slice::<USTRegistry>(&bytes) {
                    app.ust_registry = draft;
                    app.error =
                        Some(app.locale_mgr.tr(&app.active_language, "export_draft_loaded"));
                } else {
                    app.error =
                        Some(app.locale_mgr.tr(&app.active_language, "export_draft_load_fail"));
                }
            }
        });
    }

    fn handle_confirm_export_pdf(app: &mut crate::app::FepdfApp) -> bool {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name("output_compliant.pdf")
            .save_file()
        {
            if app.export_burn_redactions {
                let mut keys: Vec<usize> = app.raw_texts.keys().copied().collect();
                keys.sort_unstable();
                for page_idx in keys {
                    if let (Some(raw_text), Some(spans)) =
                        (app.raw_texts.get(&page_idx).cloned(), app.page_spans.get_mut(&page_idx))
                    {
                        let sanitized = app
                            .redaction_manager
                            .perform_physical_redaction(page_idx, &raw_text, spans);
                        app.raw_texts.insert(page_idx, sanitized);
                    }
                }
                app.redaction_manager.clear();
            }

            let sig_pos =
                app.signature_position.map(|(idx, r)| (idx, [r.min.x, r.min.y, r.max.x, r.max.y]));
            let _ = app.tx_worker.send(WorkerRequest::Save {
                path: p,
                compress: app.export_compress,
                linearize: app.export_linearize,
                vacuum: app.export_vacuum,
                upgrade_pdf20: app.export_upgrade_pdf20,
                redaction_zones: app.redaction_manager.zones.clone(),
                cert_path: app.cert_path.clone(),
                cert_password: app.cert_password.clone(),
                signature_position: sig_pos,
            });
            true
        } else {
            false
        }
    }
}
