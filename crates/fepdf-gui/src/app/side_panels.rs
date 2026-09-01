//! Left vertical icon bar and collapsible utility drawer for `FepdfApp`.

use super::FepdfApp;
use super::icons::{VectorIcon, icon_bar_btn, vector_icon_bar_btn};
use crate::sidebar::ActiveDrawer;

impl FepdfApp {
    /// Renders the slim vertical icon bar docked to the leftmost edge.
    pub(crate) fn render_left_icon_bar(&mut self, ui: &mut egui::Ui) {
        // RR-15 Limit: GUI - Left vertical icon bar for file ops, view modes, and drawer toggles
        let ctx = ui.ctx().clone();
        let has_doc = self.total_pages > 0;

        let (
            tip_import,
            tip_export,
            tip_info,
            tip_acc,
            tip_insp,
            tip_redact,
            tip_caliper,
            tip_about,
            tip_settings,
        ) = {
            let mgr = &self.locale_mgr;
            let l = &self.active_language;
            (
                mgr.tr(l, "tooltip_import_pdf"),
                mgr.tr(l, "tooltip_export_pdf"),
                mgr.tr(l, "tab_doc_info_decisions"),
                mgr.tr(l, "tab_accessibility"),
                mgr.tr(l, "tooltip_inspector"),
                mgr.tr(l, "tooltip_redact_brush"),
                mgr.tr(l, "tooltip_caliper_brush"),
                mgr.tr(l, "tooltip_about"),
                mgr.tr(l, "tooltip_settings"),
            )
        };

        egui::Panel::left("left_icon_bar").resizable(false).exact_size(44.0).show_inside(
            ui,
            |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(6.0);

                    // 1. File Actions: Import & Export
                    if vector_icon_bar_btn(ui, VectorIcon::Import, false, true)
                        .on_hover_text(tip_import)
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

                    ui.add_space(2.0);

                    if vector_icon_bar_btn(ui, VectorIcon::Export, false, has_doc)
                        .on_hover_text(tip_export)
                        .clicked()
                        && has_doc
                    {
                        self.show_export_wizard = true;
                    }

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // 2. Drawers & Inspection Tools
                    ui.add_enabled_ui(has_doc, |ui| {
                        // Info & Decisions
                        let is_info = self.active_drawer == ActiveDrawer::DocumentInfo;
                        let info_btn = icon_bar_btn("\u{e0cc}", is_info);
                        if ui.add(info_btn).on_hover_text(tip_info).clicked() {
                            self.active_drawer = if is_info {
                                ActiveDrawer::None
                            } else {
                                ActiveDrawer::DocumentInfo
                            };
                            self.caliper_tool.is_active = false;
                        }

                        ui.add_space(2.0);

                        // Accessibility & Tags
                        let is_acc = self.active_drawer == ActiveDrawer::Accessibility;
                        let acc_btn = icon_bar_btn("\u{e33c}", is_acc);
                        if ui.add(acc_btn).on_hover_text(tip_acc).clicked() {
                            self.active_drawer = if is_acc {
                                ActiveDrawer::None
                            } else {
                                ActiveDrawer::Accessibility
                            };
                            self.caliper_tool.is_active = false;
                        }

                        ui.add_space(2.0);

                        // Arlington Inspector
                        let is_insp = self.active_drawer == ActiveDrawer::Inspector;
                        let insp_btn = icon_bar_btn("\u{e151}", is_insp);
                        if ui.add(insp_btn).on_hover_text(tip_insp).clicked() {
                            self.active_drawer =
                                if is_insp { ActiveDrawer::None } else { ActiveDrawer::Inspector };
                            self.caliper_tool.is_active = false;
                        }

                        ui.add_space(2.0);

                        // Redact Studio
                        let is_redact = self.active_drawer == ActiveDrawer::Redaction;
                        let redact_btn = icon_bar_btn("\u{e28f}", is_redact);
                        if ui.add(redact_btn).on_hover_text(tip_redact).clicked() {
                            self.active_drawer = if is_redact {
                                ActiveDrawer::None
                            } else {
                                ActiveDrawer::Redaction
                            };
                            self.caliper_tool.is_active = false;
                        }

                        ui.add_space(2.0);

                        // Caliper Measurement
                        let is_caliper = self.active_drawer == ActiveDrawer::Caliper;
                        let caliper_btn = icon_bar_btn("\u{e15a}", is_caliper);
                        if ui.add(caliper_btn).on_hover_text(tip_caliper).clicked() {
                            self.active_drawer =
                                if is_caliper { ActiveDrawer::None } else { ActiveDrawer::Caliper };
                            self.caliper_tool.is_active = !is_caliper;
                        }
                    });

                    // 4. Bottom Aligned Utilities: Command Palette, Settings, About
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        ui.add_space(6.0);

                        // About
                        let about_btn = icon_bar_btn("\u{e082}", false);
                        if ui.add(about_btn).on_hover_text(tip_about).clicked() {
                            self.show_about_modal = true;
                        }

                        ui.add_space(2.0);

                        // Settings
                        let settings_btn = icon_bar_btn("\u{e30b}", false);
                        if ui.add(settings_btn).on_hover_text(tip_settings).clicked() {
                            self.show_settings_modal = true;
                        }

                        ui.add_space(2.0);

                        // Command Palette (Cmd+K / Ctrl+K)
                        let palette_btn = egui::Button::new(egui::RichText::new("⌘K").size(12.0))
                            .min_size(egui::vec2(32.0, 32.0));
                        if ui
                            .add(palette_btn)
                            .on_hover_text("コマンドパレット (Ctrl+K / ⌘K)")
                            .clicked()
                        {
                            self.show_command_palette = !self.show_command_palette;
                        }

                        ui.add_space(4.0);
                        ui.separator();
                    });
                });
            },
        );
    }

    /// Renders the collapsible utility drawer panel next to the left icon bar.
    pub(crate) fn render_side_drawer(&mut self, ui: &mut egui::Ui) {
        // RR-15 Limit: GUI - Render active utility drawer on the left side of the main pane
        if self.active_drawer == ActiveDrawer::None {
            return;
        }

        let locale_mgr = &self.locale_mgr;
        let active_lang = &self.active_language;

        egui::Panel::left("active_side_drawer")
            .resizable(true)
            .show_separator_line(true)
            .default_size(320.0)
            .size_range(260.0..=600.0)
            .show_inside(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);

                // Drawer Header with Title and Close button
                ui.horizontal(|ui| {
                    let title = match self.active_drawer {
                        ActiveDrawer::None => String::new(),
                        ActiveDrawer::DocumentInfo => {
                            locale_mgr.tr(active_lang, "tab_doc_info_decisions")
                        }
                        ActiveDrawer::Accessibility => {
                            locale_mgr.tr(active_lang, "tab_accessibility")
                        }
                        ActiveDrawer::Inspector => locale_mgr.tr(active_lang, "tooltip_inspector"),
                        ActiveDrawer::Redaction => {
                            locale_mgr.tr(active_lang, "tooltip_redact_brush")
                        }
                        ActiveDrawer::Caliper => {
                            locale_mgr.tr(active_lang, "tooltip_caliper_brush")
                        }
                    };
                    ui.heading(egui::RichText::new(&title).size(14.0));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✕").on_hover_text("閉じる (Close)").clicked() {
                            self.active_drawer = ActiveDrawer::None;
                            self.caliper_tool.is_active = false;
                        }
                    });
                });
                ui.separator();

                egui::ScrollArea::vertical().id_salt("side_drawer_scroll").show(
                    ui,
                    |ui| match self.active_drawer {
                        ActiveDrawer::None => {}
                        ActiveDrawer::DocumentInfo => {
                            self.sidebar_panel.show_document_info_unified(
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
                                locale_mgr,
                                active_lang,
                            );
                        }
                        ActiveDrawer::Accessibility => {
                            self.sidebar_panel.show_accessibility_unified(
                                ui,
                                &mut self.ust_registry,
                                &self.tx_worker,
                                locale_mgr,
                                active_lang,
                            );
                        }
                        ActiveDrawer::Inspector => {
                            let selected_tag = self.ust_registry.selected_node_id.and_then(|id| {
                                if let Some(ref root) = self.ust_registry.root {
                                    crate::sidebar::USTRegistry::find_node_by_id_recursive(root, id)
                                        .map(|n| n.tag.as_str())
                                } else {
                                    None
                                }
                            });
                            self.arlington_inspector.show(
                                ui,
                                selected_tag,
                                locale_mgr,
                                active_lang,
                            );
                        }
                        ActiveDrawer::Redaction => {
                            self.redaction_studio_panel.show(
                                ui,
                                &self.raw_texts,
                                &self.page_spans,
                                &mut self.redaction_manager,
                                locale_mgr,
                                active_lang,
                            );
                        }
                        ActiveDrawer::Caliper => {
                            self.caliper_tool.is_active = true;
                            self.caliper_tool.show_panel(ui);
                        }
                    },
                );
            });
    }
}
