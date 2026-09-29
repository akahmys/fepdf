use crate::sidebar::USTRegistry;
use crate::worker::{Protection, WorkerRequest};
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Table 22's permissions, each with the key that names it to the reader.
///
/// **The keywords are the engine's**, `fepdf::permission_keywords`; this only adds the
/// names, and `the_permissions_offered_are_the_engines` fails if the two lists part.
const PERMISSIONS: [(&str, &str); 8] = [
    ("print", "export_perm_print"),
    ("modify", "export_perm_modify"),
    ("copy", "export_perm_copy"),
    ("annotate", "export_perm_annotate"),
    ("forms", "export_perm_forms"),
    ("accessibility", "export_perm_accessibility"),
    ("assemble", "export_perm_assemble"),
    ("print-high", "export_perm_print_high"),
];

/// What protects the output, from what the wizard holds.
///
/// **A password or certificates, not both**, and the choice decides which: the other's
/// fields are ignored rather than trusted to be empty. The permissions go with either.
#[must_use]
pub fn protection_of(
    password: Option<&String>,
    owner_password: Option<&String>,
    recipients: Option<&Vec<PathBuf>>,
    denied: &BTreeSet<&'static str>,
) -> Protection {
    let permissions = (!denied.is_empty()).then(|| {
        PERMISSIONS
            .iter()
            .map(|(keyword, _)| *keyword)
            .filter(|keyword| !denied.contains(keyword))
            .collect::<Vec<_>>()
            .join(",")
    });
    match password {
        Some(password) => Protection {
            password: Some(password.clone()),
            owner_password: owner_password.cloned(),
            recipients: Vec::new(),
            permissions,
        },
        None => Protection {
            password: None,
            owner_password: None,
            recipients: recipients.cloned().unwrap_or_default(),
            permissions: recipients.is_some().then_some(permissions).flatten(),
        },
    }
}

/// Which protection the wizard is set to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protect {
    Nothing,
    Password,
    Certificates,
}

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
        egui::Window::new(window_title)
            .open(&mut open)
            .resizable(false)
            .default_width(crate::app::theme::size::FORM_W)
            .show(ctx, |ui| {
                Self::render_compliance_checkboxes(app, ui);
                Self::render_encryption_section(app, ui);
                Self::render_signature_section(app, ui);
                Self::render_draft_management_section(app, ui);

                ui.separator();

                // Certificate protection with no certificate would write the document
                // unprotected, under a choice that says otherwise; the section above
                // already says none is chosen.
                let ready = app.export_recipients.as_ref().is_none_or(|r| !r.is_empty());
                ui.vertical_centered_justified(|ui| {
                    if ui.add_enabled(ready, egui::Button::new(confirm_text)).clicked() {
                        should_close = Self::handle_confirm_export_pdf(app);
                    }
                });
            });

        app.show_export_wizard = open && !should_close;
    }

    fn render_compliance_checkboxes(app: &mut crate::app::FepdfApp, ui: &mut egui::Ui) {
        ui.heading(app.locale_mgr.tr(&app.active_language, "export_options_heading"));
        ui.add_space(crate::app::theme::space::ITEM);

        ui.checkbox(
            &mut app.export_upgrade_pdf20,
            app.locale_mgr.tr(&app.active_language, "export_opt_upgrade"),
        );
        ui.checkbox(
            &mut app.export_linearize,
            app.locale_mgr.tr(&app.active_language, "export_opt_linearize"),
        );
        ui.checkbox(
            &mut app.export_compress,
            app.locale_mgr.tr(&app.active_language, "export_opt_compress"),
        );
        ui.checkbox(
            &mut app.export_burn_redactions,
            app.locale_mgr.tr(&app.active_language, "export_opt_burn_redactions"),
        );
        ui.checkbox(
            &mut app.export_strip,
            app.locale_mgr.tr(&app.active_language, "export_opt_strip"),
        );
    }

    /// What protects the saved document (7.6.4).
    ///
    /// **This crate could sign a document and not protect one** until 2026-09-09: the
    /// signature section below has stood for phases while `SaveOptions::password` was
    /// never set from here, so the engine's whole encryption path was unreachable from
    /// the GUI. `fepdf-cli` has taken `--password` throughout.
    ///
    /// One scheme and no choice of it: PDF 2.0 deprecates everything but AES-256, and
    /// this engine's rule is not to write what 2.0 deprecates (ADR-0015). A radio button
    /// offering RC4 would be offering a document this engine will not produce.
    fn render_encryption_section(app: &mut crate::app::FepdfApp, ui: &mut egui::Ui) {
        let tr = |key: &str| app.locale_mgr.tr(&app.active_language, key);
        ui.add_space(crate::app::theme::space::GROUP);
        ui.heading(tr("export_encryption_heading"));
        ui.add_space(crate::app::theme::space::ITEM);

        let was = if app.export_password.is_some() {
            Protect::Password
        } else if app.export_recipients.is_some() {
            Protect::Certificates
        } else {
            Protect::Nothing
        };
        let mut chosen = was;
        ui.radio_value(&mut chosen, Protect::Nothing, tr("export_enc_none"));
        ui.radio_value(&mut chosen, Protect::Password, tr("export_enc_password"));
        ui.radio_value(&mut chosen, Protect::Certificates, tr("export_enc_certificates"));
        if chosen != was {
            // Dropping what the other choice held, rather than keeping it hidden: a
            // password still in the struct is one that gets written by the next save.
            app.export_password = (chosen == Protect::Password).then(String::new);
            app.export_owner_password = None;
            app.export_recipients = (chosen == Protect::Certificates).then(Vec::new);
        }

        match chosen {
            Protect::Nothing => {}
            Protect::Password => Self::render_password_fields(app, ui),
            Protect::Certificates => Self::render_certificate_fields(app, ui),
        }
        if chosen != Protect::Nothing {
            Self::render_permissions(app, ui);
        }
    }

    /// The two passwords of the standard handler (7.6.4).
    fn render_password_fields(app: &mut crate::app::FepdfApp, ui: &mut egui::Ui) {
        let tr = |key: &str| app.locale_mgr.tr(&app.active_language, key);
        let Some(password) = app.export_password.as_mut() else { return };
        ui.add_space(crate::app::theme::space::ITEM);
        ui.label(tr("export_enc_user_password"));
        ui.add(egui::TextEdit::singleline(password).password(true).desired_width(f32::INFINITY));

        let mut owner = app.export_owner_password.clone().unwrap_or_default();
        ui.add_space(crate::app::theme::space::ITEM);
        ui.label(tr("export_enc_owner_password"));
        if ui
            .add(egui::TextEdit::singleline(&mut owner).password(true).desired_width(f32::INFINITY))
            .changed()
        {
            app.export_owner_password = (!owner.is_empty()).then_some(owner);
        }
        ui.add_space(crate::app::theme::space::ITEM);
        ui.label(
            egui::RichText::new(tr("export_enc_note")).size(crate::app::theme::text::SMALL).weak(),
        );
    }

    /// The recipients of the public-key handler (7.6.5): whose certificate, not what
    /// password, decides who opens it.
    fn render_certificate_fields(app: &mut crate::app::FepdfApp, ui: &mut egui::Ui) {
        let tr = |key: &str| app.locale_mgr.tr(&app.active_language, key);
        let Some(recipients) = app.export_recipients.as_mut() else { return };
        ui.add_space(crate::app::theme::space::ITEM);
        if ui.button(tr("export_enc_pick_certs")).clicked()
            && let Some(picked) =
                rfd::FileDialog::new().add_filter("DER", &["der", "cer", "crt"]).pick_files()
        {
            recipients.extend(picked);
            recipients.sort();
            recipients.dedup();
        }
        if recipients.is_empty() {
            ui.label(tr("export_enc_no_certs"));
        }
        for path in recipients.iter() {
            ui.label(path.file_name().unwrap_or(path.as_os_str()).to_string_lossy());
        }
        ui.add_space(crate::app::theme::space::ITEM);
        ui.label(
            egui::RichText::new(tr("export_enc_cert_note"))
                .size(crate::app::theme::text::SMALL)
                .weak(),
        );
    }

    /// What may be done with the document once it is open (Table 22).
    ///
    /// **Everything is ticked to start with**, because encrypting a document is not on
    /// its own a statement about what may be done with it; the reader takes away what they
    /// mean to.
    fn render_permissions(app: &mut crate::app::FepdfApp, ui: &mut egui::Ui) {
        let tr = |key: &str| app.locale_mgr.tr(&app.active_language, key);
        ui.add_space(crate::app::theme::space::ITEM);
        ui.label(tr("export_perm_heading"));
        for (keyword, key) in PERMISSIONS {
            let mut granted = !app.export_denied.contains(keyword);
            if ui.checkbox(&mut granted, tr(key)).changed() {
                if granted {
                    app.export_denied.remove(keyword);
                } else {
                    app.export_denied.insert(keyword);
                }
            }
        }
        ui.label(
            egui::RichText::new(tr("export_perm_note")).size(crate::app::theme::text::SMALL).weak(),
        );
    }

    fn render_signature_section(app: &mut crate::app::FepdfApp, ui: &mut egui::Ui) {
        // RR-15 Limit: GUI - signature UI layout section declaration
        ui.separator();
        ui.heading(app.locale_mgr.tr(&app.active_language, "export_signature_heading"));
        ui.add_space(crate::app::theme::space::ITEM);

        // The engine takes a DER certificate and a DER PKCS#8 key. This asked for a
        // PKCS#12 bundle and a password, then handed the bundle to the SDK as both the
        // certificate *and* the key — which nothing noticed while signing refused
        // outright. It asks for what it uses now, and the password box is gone with the
        // format that needed one.
        let der = ["der", "cer", "crt", "key", "pk8", "p8"];
        ui.horizontal(|ui| {
            if ui
                .button(app.locale_mgr.tr(&app.active_language, "export_sig_select_cert"))
                .clicked()
                && let Some(p) = rfd::FileDialog::new().add_filter("DER", &der).pick_file()
            {
                app.cert_path = Some(p);
            }
            if let Some(path) = &app.cert_path {
                ui.label(path.file_name().unwrap_or(path.as_os_str()).to_string_lossy());
            } else {
                ui.label(app.locale_mgr.tr(&app.active_language, "export_sig_no_cert"));
            }
        });

        ui.horizontal(|ui| {
            if ui.button(app.locale_mgr.tr(&app.active_language, "export_sig_select_key")).clicked()
                && let Some(p) = rfd::FileDialog::new().add_filter("DER", &der).pick_file()
            {
                app.key_path = Some(p);
            }
            if let Some(path) = &app.key_path {
                ui.label(path.file_name().unwrap_or(path.as_os_str()).to_string_lossy());
            } else {
                ui.label(app.locale_mgr.tr(&app.active_language, "export_sig_no_key"));
            }
        });

        if app.cert_path.is_some() && app.key_path.is_some() {
            ui.horizontal(|ui| {
                // Placing the field is done on the page, so it cannot be started from the
                // grid: the wizard would close onto a view that throws the clicks away.
                let on_a_page = app.view.does(crate::view::Act::DrawOnPage);
                let label = app.locale_mgr.tr(&app.active_language, "export_sig_place_field");
                let label = if on_a_page {
                    label
                } else {
                    app.locale_mgr
                        .tr(&app.active_language, "tooltip_page_view_only")
                        .replacen("{}", &label, 1)
                };
                if ui
                    .add_enabled(
                        on_a_page,
                        egui::Button::selectable(app.is_placing_signature, label),
                    )
                    .clicked()
                {
                    app.is_placing_signature = !app.is_placing_signature;
                    if app.is_placing_signature {
                        app.show_export_wizard = false;
                    }
                }
                // The page, and only the page: the field is invisible, so where on the
                // page it was placed decides nothing.
                if let Some((page, _)) = &app.signature_position {
                    ui.label(
                        app.locale_mgr
                            .tr(&app.active_language, "export_sig_placed")
                            .replace("{}", &(page + 1).to_string()),
                    );
                } else {
                    ui.label(app.locale_mgr.tr(&app.active_language, "export_sig_not_placed"));
                }
            });
        }
    }

    fn render_draft_management_section(app: &mut crate::app::FepdfApp, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading(app.locale_mgr.tr(&app.active_language, "export_draft_heading"));
        ui.add_space(crate::app::theme::space::ITEM);

        ui.horizontal(|ui| {
            if ui.button(app.locale_mgr.tr(&app.active_language, "export_draft_save")).clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("JSON", &["json"])
                    .set_file_name("ust_draft.json")
                    .save_file()
                && let Ok(json_str) = serde_json::to_string_pretty(&app.ust_registry)
            {
                if std::fs::write(&p, json_str).is_ok() {
                    // The two-shaped substitution here — `{file}` if present, otherwise
                    // a literal `{:?}` — was working around a locale value that had been
                    // written both ways. `Notice` takes the key and one `{}`.
                    let file_label = p.file_name().unwrap_or(p.as_os_str()).display().to_string();
                    app.notice =
                        Some(crate::app::Notice::done("export_draft_saved").about(file_label));
                } else {
                    app.notice = Some(crate::app::Notice::failed("export_draft_save_fail"));
                }
            }

            if ui.button(app.locale_mgr.tr(&app.active_language, "export_draft_load")).clicked()
                && let Some(p) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file()
                && let Ok(bytes) = std::fs::read(&p)
            {
                if let Ok(draft) = serde_json::from_slice::<USTRegistry>(&bytes) {
                    app.ust_registry = draft;
                    app.notice = Some(crate::app::Notice::done("export_draft_loaded"));
                } else {
                    app.notice = Some(crate::app::Notice::failed("export_draft_load_fail"));
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
            }
            let redaction_zones =
                app.redaction_manager.zones_for_export(app.export_burn_redactions);

            let sig_pos =
                app.signature_position.map(|(idx, r)| (idx, [r.min.x, r.min.y, r.max.x, r.max.y]));
            let _ = app.tx_worker.send(WorkerRequest::Save {
                path: p,
                protection: protection_of(
                    app.export_password.as_ref(),
                    app.export_owner_password.as_ref(),
                    app.export_recipients.as_ref(),
                    &app.export_denied,
                ),
                compress: app.export_compress,
                strip: app.export_strip,
                linearize: app.export_linearize,
                upgrade_pdf20: app.export_upgrade_pdf20,
                redaction_zones,
                cert_path: app.cert_path.clone(),
                key_path: app.key_path.clone(),
                signature_position: sig_pos,
            });
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod protection {
    use super::{PERMISSIONS, protection_of};
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    #[test]
    fn the_permissions_offered_are_the_engines() {
        let offered: Vec<&str> = PERMISSIONS.iter().map(|(keyword, _)| *keyword).collect();
        assert_eq!(offered, fepdf::permission_keywords());
    }

    /// Nothing taken away is no `/P` restriction at all, not a list of everything.
    #[test]
    fn granting_everything_asks_for_nothing() {
        let password = "open".to_string();
        let protection = protection_of(Some(&password), None, None, &BTreeSet::new());
        assert_eq!(protection.permissions, None);
        assert_eq!(protection.password.as_deref(), Some("open"));
    }

    /// What is taken away is left out of the list the engine grants from.
    #[test]
    fn what_is_taken_away_is_not_granted() {
        let password = "open".to_string();
        let denied = BTreeSet::from(["copy", "modify"]);
        let protection = protection_of(Some(&password), None, None, &denied);
        assert_eq!(
            protection.permissions.as_deref(),
            Some("print,annotate,forms,accessibility,assemble,print-high")
        );
    }

    /// **Certificates and a password are one choice.** A password left over from the
    /// other choice is not written beside certificates.
    #[test]
    fn certificates_carry_no_password() {
        let recipients = vec![PathBuf::from("alice.der")];
        let denied = BTreeSet::from(["print"]);
        let protection = protection_of(None, None, Some(&recipients), &denied);
        assert_eq!(protection.recipients, recipients);
        assert_eq!(protection.password, None);
        assert!(protection.permissions.is_some(), "the permissions did not go with them");
    }

    /// And with neither, the permissions have no handler to live in and are not sent.
    #[test]
    fn unprotected_carries_no_permissions() {
        let denied = BTreeSet::from(["print"]);
        let protection = protection_of(None, None, None, &denied);
        assert_eq!(protection, crate::worker::Protection::default());
    }
}
