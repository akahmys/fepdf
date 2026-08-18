use super::ust_registry::{USTRegistry, collect_figures, update_alt_text};
use crate::locale::LocaleManager;
use crate::worker::WorkerRequest;
use std::sync::mpsc::Sender;

pub fn show_accessibility_audit(
    // RR-15 Limit: GUI - Render accessibility audit findings panel
    ui: &mut egui::Ui,
    registry: &mut USTRegistry,
    locale_mgr: &LocaleManager,
    active_lang: &str,
) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(locale_mgr.tr(active_lang, "audit_title")).strong().size(13.0),
        );
        ui.add_space(6.0);

        let has_doc = registry.root.is_some();
        let audit_findings_count = registry.audit_findings.len();

        ui.vertical(|ui| {
            if has_doc {
                let compliant_pct = if audit_findings_count == 0 {
                    100
                } else {
                    (100 - audit_findings_count * 7).max(10)
                };
                ui.label(
                    locale_mgr
                        .tr(active_lang, "audit_compliant")
                        .replace("{}", &compliant_pct.to_string()),
                );
                ui.label(
                    locale_mgr
                        .tr(active_lang, "audit_findings")
                        .replace("{}", &audit_findings_count.to_string()),
                );
            } else {
                ui.label(locale_mgr.tr(active_lang, "audit_compliant_none"));
                ui.label(locale_mgr.tr(active_lang, "audit_findings_none"));
            }
        });

        ui.add_space(4.0);

        egui::ScrollArea::vertical().id_salt("audit_scroll").max_height(100.0).show(ui, |ui| {
            if !has_doc {
                ui.label(egui::RichText::new(locale_mgr.tr(active_lang, "no_doc_loaded")).weak());
            } else if registry.audit_findings.is_empty() {
                ui.colored_label(
                    egui::Color32::GREEN,
                    locale_mgr.tr(active_lang, "audit_success_100"),
                );
            } else {
                for (checkpoint, severity, message, handle_id) in &registry.audit_findings {
                    let card_resp = ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(egui::Color32::LIGHT_RED, checkpoint);
                            ui.label(format!("({severity})"));
                        });
                        ui.label(message);
                    });

                    let id = ui.id().with(checkpoint).with(message);
                    let response = ui.interact(card_resp.response.rect, id, egui::Sense::click());
                    if response.clicked()
                        && let Some(h_id) = handle_id
                        && let Some(node_id) = registry.find_node_id_by_handle_id(*h_id)
                    {
                        registry.selected_node_id = Some(node_id);
                        registry.pending_center_node_id = Some(node_id);
                    }
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    ui.add_space(3.0);
                }
            }
        });
    });
}

pub fn show_alt_text_gallery(
    // RR-15 Limit: GUI - Renders a carousel list of figure elements and their Alt text cards
    ui: &mut egui::Ui,
    registry: &mut USTRegistry,
    tx_worker: &Sender<WorkerRequest>,
    locale_mgr: &LocaleManager,
    active_lang: &str,
) {
    ui.vertical(|ui| {
        ui.label(
            egui::RichText::new(locale_mgr.tr(active_lang, "alt_text_gallery_title")).strong(),
        );
        ui.add_space(2.0);

        let mut figures = Vec::new();
        if let Some(ref root) = registry.root {
            collect_figures(root, &mut figures);
        }

        if figures.is_empty() {
            ui.label(locale_mgr.tr(active_lang, "alt_text_gallery_none"));
        } else {
            egui::ScrollArea::horizontal().id_salt("figure_gallery_carousel").show(ui, |ui| {
                ui.horizontal(|ui| {
                    for fig in &figures {
                        ui.vertical(|ui| {
                            ui.set_min_width(200.0);
                            ui.vertical(|ui| {
                                let fig_title = locale_mgr
                                    .tr(active_lang, "alt_text_card_fig")
                                    .replace("{}", &fig.id.to_string());
                                ui.colored_label(egui::Color32::LIGHT_BLUE, fig_title);

                                let mut buf = fig.alt_text.clone().unwrap_or_default();
                                let hint = locale_mgr.tr(active_lang, "alt_text_card_no_alt");
                                let response =
                                    ui.add(egui::TextEdit::singleline(&mut buf).hint_text(hint));

                                if response.changed() {
                                    let new_alt = if buf.trim().is_empty() {
                                        None
                                    } else {
                                        Some(buf.clone())
                                    };
                                    if let Some(ref mut root) = registry.root
                                        && update_alt_text(root, fig.id, new_alt.clone())
                                        && let Some(h_id) = fig.handle_id
                                    {
                                        let _ = tx_worker.send(WorkerRequest::UpdateNode {
                                            handle_id: h_id,
                                            tag: "Figure".to_string(),
                                            alt_text: new_alt,
                                        });
                                    }
                                }
                            });
                        });
                        ui.add_space(5.0);
                    }
                });
            });
        }
    });
}
