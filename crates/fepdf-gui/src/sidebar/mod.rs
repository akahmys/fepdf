mod accessibility;
pub mod document_info;
pub mod layers;
pub mod structure_tree;
pub mod ust_registry;
pub mod what_it_does;

pub use ust_registry::{DragRelation, USTNode, USTRegistry};

use crate::locale::LocaleManager;
use crate::worker::WorkerRequest;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum ActiveDrawer {
    #[default]
    None,
    DocumentInfo,
    /// What the document does when opened, and what protects it.
    WhatItDoes,
    Accessibility,
    Redaction,
    Caliper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum AccessibilitySubTab {
    #[default]
    Tree,
    AltText,
    Audit,
}

pub struct SidebarPanel {
    pub accessibility_sub_tab: AccessibilitySubTab,
    pub alt_text_edit_buffer: String,
}

impl Default for SidebarPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarPanel {
    pub fn new() -> Self {
        Self {
            accessibility_sub_tab: AccessibilitySubTab::Tree,
            alt_text_edit_buffer: String::new(),
        }
    }

    fn render_accessibility_tab_content(
        &mut self,
        ui: &mut egui::Ui,
        registry: &mut USTRegistry,
        tx_worker: &Sender<WorkerRequest>,
        locale_mgr: &LocaleManager,
        active_lang: &str,
    ) {
        match self.accessibility_sub_tab {
            AccessibilitySubTab::Tree => {
                structure_tree::show_structure_tree(
                    ui,
                    registry,
                    &mut self.alt_text_edit_buffer,
                    tx_worker,
                    locale_mgr,
                    active_lang,
                );
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                structure_tree::show_element_properties(
                    ui,
                    registry,
                    tx_worker,
                    locale_mgr,
                    active_lang,
                );
            }
            AccessibilitySubTab::AltText => {
                accessibility::show_alt_text_gallery(
                    ui,
                    registry,
                    tx_worker,
                    locale_mgr,
                    active_lang,
                );
            }
            AccessibilitySubTab::Audit => {
                accessibility::show_accessibility_audit(ui, registry, locale_mgr, active_lang);
            }
        }
    }

    fn handle_pending_tree_dnd_moves(ui: &egui::Ui, registry: &mut USTRegistry) {
        let pending_move: Option<Option<(usize, usize, DragRelation)>> =
            ui.ctx().data(|d| d.get_temp(egui::Id::new("pending_move")));
        if let Some(Some((drag_id, target_id, relation))) = pending_move {
            registry.move_node(drag_id, target_id, relation);
            ui.ctx().data_mut(|d| {
                d.remove::<Option<(usize, usize, DragRelation)>>(egui::Id::new("pending_move"));
                d.insert_temp::<Option<usize>>(egui::Id::new("dragged_node_id"), None);
            });
        }
        if ui.input(|i| i.pointer.any_released()) {
            ui.ctx().data_mut(|d| {
                d.insert_temp::<Option<usize>>(egui::Id::new("dragged_node_id"), None)
            });
        }
    }

    pub fn show_accessibility_unified(
        &mut self,
        ui: &mut egui::Ui,
        registry: &mut USTRegistry,
        tx_worker: &Sender<WorkerRequest>,
        locale_mgr: &LocaleManager,
        active_lang: &str,
    ) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                // **The label alone, with no glyph in front of it.** These three carried
                // Lucide codepoints inline in a proportional string, which is the shape
                // that let `U+E0FF` be answered by `Ubuntu-Light` elsewhere in this
                // crate; the icon font now lives in a family of its own and is reached
                // through `app::icons`. A tab that is already named does not need a
                // picture of its name.
                let sub_tabs = [
                    (AccessibilitySubTab::Tree, "Tree & Props"),
                    (AccessibilitySubTab::AltText, "Alt Text"),
                    (AccessibilitySubTab::Audit, "Audit"),
                ];
                for (tab, label) in sub_tabs {
                    let is_active = self.accessibility_sub_tab == tab;
                    if ui.selectable_label(is_active, label).clicked() {
                        self.accessibility_sub_tab = tab;
                    }
                }
            });
            ui.separator();
            self.render_accessibility_tab_content(ui, registry, tx_worker, locale_mgr, active_lang);
        });

        Self::handle_pending_tree_dnd_moves(ui, registry);
    }
}
