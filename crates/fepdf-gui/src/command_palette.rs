// RR-15 Limit: GUI - Command Palette window declaration and dispatch
pub struct CommandPalette;

impl CommandPalette {
    pub fn show(app: &mut crate::app::FepdfApp, ctx: &egui::Context) {
        // RR-15 Limit: GUI - Command Palette window declaration and dispatch
        let mut show_palette = app.show_command_palette;
        if !show_palette {
            return;
        }

        let mut close_palette = false;
        let title = app.locale_mgr.tr(&app.active_language, "cmd_palette_title");
        egui::Window::new(title)
            .open(&mut show_palette)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 100.0))
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    let text_resp = ui.text_edit_singleline(&mut app.command_palette_search);
                    text_resp.request_focus();
                });
                ui.separator();

                let query = app.command_palette_search.to_lowercase();

                // Get localized command names and descriptions
                let cmd_load_pdf = app.locale_mgr.tr(&app.active_language, "cmd_load_pdf");
                let cmd_load_pdf_desc =
                    app.locale_mgr.tr(&app.active_language, "cmd_load_pdf_desc");
                let cmd_reset_view = app.locale_mgr.tr(&app.active_language, "cmd_reset_view");
                let cmd_reset_view_desc =
                    app.locale_mgr.tr(&app.active_language, "cmd_reset_view_desc");
                let cmd_redact_brush = app.locale_mgr.tr(&app.active_language, "cmd_redact_brush");
                let cmd_redact_brush_desc =
                    app.locale_mgr.tr(&app.active_language, "cmd_redact_brush_desc");
                let cmd_tagging_brush =
                    app.locale_mgr.tr(&app.active_language, "cmd_tagging_brush");
                let cmd_tagging_brush_desc =
                    app.locale_mgr.tr(&app.active_language, "cmd_tagging_brush_desc");
                let cmd_caliper_brush =
                    app.locale_mgr.tr(&app.active_language, "cmd_caliper_brush");
                let cmd_caliper_brush_desc =
                    app.locale_mgr.tr(&app.active_language, "cmd_caliper_brush_desc");
                let cmd_inspector = app.locale_mgr.tr(&app.active_language, "cmd_inspector");
                let cmd_inspector_desc =
                    app.locale_mgr.tr(&app.active_language, "cmd_inspector_desc");
                let cmd_export_pdf = app.locale_mgr.tr(&app.active_language, "cmd_export_pdf");
                let cmd_export_pdf_desc =
                    app.locale_mgr.tr(&app.active_language, "cmd_export_pdf_desc");
                let cmd_reading_order =
                    app.locale_mgr.tr(&app.active_language, "cmd_reading_order");
                let cmd_reading_order_desc =
                    app.locale_mgr.tr(&app.active_language, "cmd_reading_order_desc");
                let cmd_redaction_studio =
                    app.locale_mgr.tr(&app.active_language, "cmd_redaction_studio");
                let cmd_redaction_studio_desc =
                    app.locale_mgr.tr(&app.active_language, "cmd_redaction_studio_desc");

                let commands = vec![
                    (&cmd_load_pdf, &cmd_load_pdf_desc, "Load PDF"),
                    (&cmd_reset_view, &cmd_reset_view_desc, "Reset View"),
                    (&cmd_redact_brush, &cmd_redact_brush_desc, "Redact Brush"),
                    (&cmd_tagging_brush, &cmd_tagging_brush_desc, "Tagging Brush"),
                    (&cmd_caliper_brush, &cmd_caliper_brush_desc, "Caliper Brush"),
                    (&cmd_inspector, &cmd_inspector_desc, "Inspector"),
                    (&cmd_export_pdf, &cmd_export_pdf_desc, "Export PDF"),
                    (&cmd_reading_order, &cmd_reading_order_desc, "Reading Order"),
                    (&cmd_redaction_studio, &cmd_redaction_studio_desc, "Redaction Studio"),
                ];

                for (cmd_name, cmd_desc, cmd_action) in commands {
                    if (query.is_empty()
                        || cmd_name.to_lowercase().contains(&query)
                        || cmd_desc.to_lowercase().contains(&query))
                        && ui.selectable_label(false, format!("{cmd_name} — {cmd_desc}")).clicked()
                    {
                        match cmd_action {
                            "Load PDF" => {
                                if let Some(p) =
                                    rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file()
                                {
                                    app.open_file(p, ctx);
                                }
                            }
                            "Reset View" => app.reset_view(),
                            "Redact Brush" => {
                                app.redaction_manager.is_active = !app.redaction_manager.is_active;
                                if app.redaction_manager.is_active {
                                    app.selection_manager.clear();
                                    app.selection_manager.is_tagging_brush_active = false;
                                    app.caliper_tool.is_active = false;
                                }
                            }
                            "Tagging Brush" => {
                                app.selection_manager.is_tagging_brush_active =
                                    !app.selection_manager.is_tagging_brush_active;
                                if app.selection_manager.is_tagging_brush_active {
                                    app.selection_manager.clear();
                                    app.redaction_manager.is_active = false;
                                    app.caliper_tool.is_active = false;
                                }
                            }
                            "Caliper Brush" => {
                                let is_caliper =
                                    app.active_drawer == crate::sidebar::ActiveDrawer::Caliper;
                                app.active_drawer = if is_caliper {
                                    crate::sidebar::ActiveDrawer::None
                                } else {
                                    crate::sidebar::ActiveDrawer::Caliper
                                };
                                app.caliper_tool.is_active = !is_caliper;
                            }
                            "Inspector" => {
                                app.active_drawer = if app.active_drawer
                                    == crate::sidebar::ActiveDrawer::Inspector
                                {
                                    crate::sidebar::ActiveDrawer::None
                                } else {
                                    crate::sidebar::ActiveDrawer::Inspector
                                };
                            }
                            "Export PDF" => app.show_export_wizard = true,
                            "Reading Order" => app.show_reading_order = !app.show_reading_order,
                            "Redaction Studio" => {
                                app.active_drawer = if app.active_drawer
                                    == crate::sidebar::ActiveDrawer::Redaction
                                {
                                    crate::sidebar::ActiveDrawer::None
                                } else {
                                    crate::sidebar::ActiveDrawer::Redaction
                                };
                            }
                            _ => {}
                        }
                        close_palette = true;
                    }
                }
            });

        if close_palette {
            app.show_command_palette = false;
            app.command_palette_search.clear();
        } else {
            app.show_command_palette = show_palette;
        }
    }
}
