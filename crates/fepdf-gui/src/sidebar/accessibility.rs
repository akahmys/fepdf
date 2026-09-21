use super::ust_registry::{USTRegistry, collect_figures, update_alt_text};
use crate::app::theme::colors;
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
            egui::RichText::new(locale_mgr.tr(active_lang, "audit_title"))
                .strong()
                .size(crate::app::theme::text::BODY),
        );
        ui.add_space(crate::app::theme::space::GROUP);

        let has_doc = registry.root.is_some();
        // **A sound condition is not a finding.** Since a checked-and-unbroken condition
        // became a row, `audit_findings.len()` counts them too: a clean document read
        // "Findings: 2" above a section headed "Checked and sound (2)". What this line
        // answers is how much is waiting for the reader.
        let audit_findings_count =
            registry.audit_findings.iter().filter(|r| r.outcome != fepdf::Outcome::Sound).count();

        ui.vertical(|ui| {
            if has_doc {
                // **This said "Matterhorn: 100% Compliant" when nothing was found**, on
                // an audit that looks at a handful of the protocol's 136 failure
                // conditions — and the percentage was `100 - findings * 7`, which is a
                // number with no measurement behind it at all. What a reader is owed is
                // how much was looked at, so that "no findings" means what it means.
                //
                // The protocol has 31 checkpoints comprised of 136 failure conditions;
                // this counts conditions, and the line says so.
                ui.label(
                    locale_mgr
                        .tr(active_lang, "audit_compliant")
                        .replacen("{}", &registry.audit_checked.to_string(), 1)
                        .replacen("{}", &registry.audit_in_protocol.to_string(), 1),
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

        ui.add_space(crate::app::theme::space::ITEM);

        egui::ScrollArea::vertical().id_salt("audit_scroll").show(ui, |ui| {
            if !has_doc {
                ui.label(egui::RichText::new(locale_mgr.tr(active_lang, "no_doc_loaded")).weak());
                return;
            }
            // **Three sections, and this order.** A reader does a different thing with
            // each kind, and one list is read as though they are one kind: the eye takes
            // the first column of every row — a number and a clause — and every row looks
            // like a violation. What must be fixed comes first, then what a reader has to
            // look at, then what came out sound.
            let sections = [
                (fepdf::Outcome::Broken, "audit_section_broken", colors::note::FAIL),
                (fepdf::Outcome::ForAReader, "audit_section_reader", colors::note::WARN),
                (fepdf::Outcome::Sound, "audit_section_sound", colors::note::PASS),
            ];
            for (outcome, title, colour) in sections {
                let rows: Vec<&crate::sidebar::AuditRow> =
                    registry.audit_findings.iter().filter(|r| r.outcome == outcome).collect();
                if rows.is_empty() {
                    continue;
                }
                ui.add_space(crate::app::theme::space::GROUP);
                ui.label(
                    egui::RichText::new(
                        locale_mgr.tr(active_lang, title).replace("{}", &rows.len().to_string()),
                    )
                    .strong(),
                );
                let clicked: Vec<u32> =
                    rows.iter().filter_map(|row| audit_row(ui, row, colour)).collect();
                for handle in clicked {
                    if let Some(node_id) = registry.find_node_id_by_handle_id(handle) {
                        registry.selected_node_id = Some(node_id);
                        registry.pending_center_node_id = Some(node_id);
                    }
                }
            }
            // A document with a structure tree this engine could not walk shows no
            // sections at all, which would otherwise be a blank panel reading as a pass.
            if registry.audit_findings.is_empty() {
                ui.label(
                    locale_mgr
                        .tr(active_lang, "audit_success_100")
                        .replace("{}", &registry.audit_checked.to_string()),
                );
            }
            // **What was never looked at is said, not left out.** A report that listed
            // only what it examined would let a reader believe it covered the protocol.
            ui.add_space(crate::app::theme::space::GROUP);
            let unlooked = registry.audit_in_protocol.saturating_sub(registry.audit_checked);
            ui.label(
                egui::RichText::new(
                    locale_mgr
                        .tr(active_lang, "audit_section_unlooked")
                        .replace("{}", &unlooked.to_string()),
                )
                .weak(),
            );
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
        ui.add_space(crate::app::theme::space::ITEM);

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
                                ui.colored_label(crate::app::theme::colors::note::INFO, fig_title);

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
                        ui.add_space(crate::app::theme::space::ITEM);
                    }
                });
            });
        }
    });
}

/// One row of the report, and the object it names when a reader clicks it.
///
/// **A row carries what it found, not only that it found something.** A suspicion without
/// its evidence tells a reader nothing they can act on, and this engine's part of the
/// bargain — for declining to decide — is handing over the materials for deciding.
fn audit_row(
    ui: &mut egui::Ui,
    row: &crate::sidebar::AuditRow,
    colour: egui::Color32,
) -> Option<u32> {
    let card = ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.colored_label(colour, &row.condition);
        });
        ui.label(&row.message);
    });
    let id = ui.id().with(&row.condition).with(&row.message);
    let response = ui.interact(card.response.rect, id, egui::Sense::click());
    if response.hovered() && row.handle_id.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.add_space(crate::app::theme::space::ITEM);
    response.clicked().then_some(row.handle_id).flatten()
}
