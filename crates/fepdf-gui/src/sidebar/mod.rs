mod accessibility;
mod attachments;
mod bookmarks;
mod document_info;
mod layers;
mod structure_tree;
pub mod ust_registry;

pub use ust_registry::{DragRelation, USTNode, USTRegistry};

use crate::locale::LocaleManager;
use crate::worker::WorkerRequest;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LeftTab {
    Layers,
    DocumentInfo,
    Bookmarks,
    Attachments,
    Structure,
    Properties,
    AltText,
    Audit,
}

pub struct SidebarPanel {
    pub active_left_tab: LeftTab,
    pub alt_text_edit_buffer: String,
    pub context_panel_open: bool,
}

impl Default for SidebarPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarPanel {
    pub fn new() -> Self {
        Self {
            active_left_tab: LeftTab::DocumentInfo,
            alt_text_edit_buffer: String::new(),
            context_panel_open: false,
        }
    }

    pub fn show_icon_bar(
        &mut self,
        ui: &mut egui::Ui,
        locale_mgr: &LocaleManager,
        active_lang: &str,
    ) {
        ui.vertical_centered(|ui| {
            ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 8.0);

            let tabs = [
                (LeftTab::DocumentInfo, "\u{e0cc}", locale_mgr.tr(active_lang, "tab_doc_info")),
                (LeftTab::Bookmarks, "\u{e060}", locale_mgr.tr(active_lang, "tab_bookmarks")),
                (LeftTab::Layers, "\u{e21b}", locale_mgr.tr(active_lang, "tab_layers")),
                (LeftTab::Attachments, "\u{e12d}", locale_mgr.tr(active_lang, "tab_attachments")),
                (LeftTab::Structure, "\u{e33c}", locale_mgr.tr(active_lang, "tab_structure")),
                (LeftTab::Properties, "\u{e29a}", locale_mgr.tr(active_lang, "tab_properties")),
                (LeftTab::AltText, "\u{e0f6}", locale_mgr.tr(active_lang, "tab_alt_text")),
                (LeftTab::Audit, "\u{e1fe}", locale_mgr.tr(active_lang, "tab_audit")),
            ];

            for (tab, icon, tooltip) in tabs {
                let is_active = self.active_left_tab == tab && self.context_panel_open;
                let mut btn = egui::Button::new(egui::RichText::new(icon).size(16.0))
                    .min_size(egui::vec2(36.0, 36.0));
                if is_active {
                    btn = btn.stroke(egui::Stroke::new(1.5_f32, egui::Color32::from_gray(80)));
                }
                if ui.add(btn).on_hover_text(tooltip).clicked() {
                    if self.active_left_tab == tab {
                        self.context_panel_open = !self.context_panel_open;
                    } else {
                        self.active_left_tab = tab;
                        self.context_panel_open = true;
                    }
                }
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        // RR-15 Limit: GUI - sidebar main routing and sub-panel egui layout tree
        &mut self,
        ui: &mut egui::Ui,
        registry: &mut USTRegistry,
        tx_worker: &Sender<WorkerRequest>,
        pdf_name: &Option<String>,
        total_pages: usize,
        metadata: &Option<fepdf::MetadataInfo>,
        file_size: Option<usize>,
        pdf_version: &Option<String>,
        security_method: &Option<String>,
        permissions: Option<i32>,
        page_sizes: &[(f64, f64)],
        fonts: &[fepdf::FontSummary],
        layers: &[fepdf::LayerRow],
        locale_mgr: &LocaleManager,
        active_lang: &str,
    ) {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.vertical(|ui| match self.active_left_tab {
            LeftTab::Layers => {
                layers::show_layers(ui, layers, tx_worker, locale_mgr, active_lang);
            }
            LeftTab::DocumentInfo => {
                document_info::show_document_info(
                    ui,
                    pdf_name,
                    total_pages,
                    metadata,
                    file_size,
                    pdf_version,
                    security_method,
                    permissions,
                    page_sizes,
                    fonts,
                    locale_mgr,
                    active_lang,
                );
            }
            LeftTab::Bookmarks => {
                bookmarks::show_bookmarks(ui, total_pages, locale_mgr, active_lang);
            }
            LeftTab::Attachments => {
                attachments::show_attachments(ui, locale_mgr, active_lang);
            }
            LeftTab::Structure => {
                structure_tree::show_structure_tree(
                    ui,
                    registry,
                    &mut self.alt_text_edit_buffer,
                    tx_worker,
                    locale_mgr,
                    active_lang,
                );
            }
            LeftTab::Properties => {
                structure_tree::show_element_properties(
                    ui,
                    registry,
                    tx_worker,
                    locale_mgr,
                    active_lang,
                );
            }
            LeftTab::AltText => {
                accessibility::show_alt_text_gallery(
                    ui,
                    registry,
                    tx_worker,
                    locale_mgr,
                    active_lang,
                );
            }
            LeftTab::Audit => {
                accessibility::show_accessibility_audit(ui, registry, locale_mgr, active_lang);
            }
        });

        // Apply pending moves
        let pending_move: Option<Option<(usize, usize, DragRelation)>> =
            ui.ctx().data(|d| d.get_temp(egui::Id::new("pending_move")));
        if let Some(Some((drag_id, target_id, relation))) = pending_move {
            registry.move_node(drag_id, target_id, relation);
            ui.ctx().data_mut(|d| {
                d.remove::<Option<(usize, usize, DragRelation)>>(egui::Id::new("pending_move"));
                d.insert_temp::<Option<usize>>(egui::Id::new("dragged_node_id"), None);
            });
        }

        // Clear dragged node ID on release
        if ui.input(|i| i.pointer.any_released()) {
            ui.ctx().data_mut(|d| {
                d.insert_temp::<Option<usize>>(egui::Id::new("dragged_node_id"), None)
            });
        }
    }
}
