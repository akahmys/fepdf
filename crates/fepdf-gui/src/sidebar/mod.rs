mod accessibility;
pub mod bookmarks;
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
    /// The operations that act on the whole document — page labels, Bates numbering,
    /// retagging, the standard it declares, attachments, geography, a portfolio.
    ///
    /// **A drawer rather than a window, because the other five are drawers.** It was a
    /// floating `egui::Window` whose only entry point was the command palette, which is
    /// to say that seven of this window's twelve document operations could be reached
    /// only by a reader who already knew they existed (UI-4).
    Tools,
    /// The bookmark tree, and the draft the reader is making of it (12.3.3).
    Bookmarks,
}

impl ActiveDrawer {
    /// Every drawer, in the order the rail lists them.
    ///
    /// **The rail iterates this rather than naming its buttons**, so a drawer that
    /// exists has a door by construction (UI-4). `scripts/audit/reachability.py` holds
    /// this list against the enum, because an array cannot be exhaustive on its own.
    pub const ALL: [Self; 7] = [
        Self::DocumentInfo,
        Self::WhatItDoes,
        Self::Accessibility,
        Self::Redaction,
        Self::Caliper,
        Self::Tools,
        Self::Bookmarks,
    ];

    /// The glyph the rail draws for it, and the locale key that names it.
    ///
    /// **No wildcard arm**, so a new variant does not compile until it has both — which
    /// is the half of reachability a script does not have to be trusted with (Rule 5).
    pub const fn face(self) -> Option<(&'static str, &'static str)> {
        use crate::app::icons::glyph;
        match self {
            Self::None => None,
            Self::DocumentInfo => Some((glyph::INFO, "tab_doc_info_decisions")),
            Self::WhatItDoes => Some((glyph::SURVEY, "tooltip_what_it_does")),
            Self::Accessibility => Some((glyph::STRUCTURE, "tab_accessibility")),
            Self::Redaction => Some((glyph::REDACT, "tooltip_redact_brush")),
            Self::Caliper => Some((glyph::CALIPER, "tooltip_caliper_brush")),
            Self::Tools => Some((glyph::TOOLS, "tools_title")),
            Self::Bookmarks => Some((glyph::MARKS, "marks_title")),
        }
    }
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
                ui.add_space(crate::app::theme::space::GROUP);
                ui.separator();
                ui.add_space(crate::app::theme::space::GROUP);
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

    /// Sends a finished drag to the document, and waits for the tree to come back.
    ///
    /// **It used to rearrange the window's own copy and stop there.** The tree redrew in
    /// the new order, nothing reached the file, the undo history never heard of it, and
    /// the dot that says a document is unsaved stayed off — so the reader fixed a reading
    /// order, saved, and got the old one. Nothing is moved here now: the worker applies
    /// `MoveStructElem` and sends back the tree as the file holds it, which is also what
    /// shows a refused move as refused.
    fn handle_pending_tree_dnd_moves(
        ui: &egui::Ui,
        registry: &USTRegistry,
        tx_worker: &Sender<WorkerRequest>,
    ) {
        let pending_move: Option<Option<(usize, usize, DragRelation)>> =
            ui.ctx().data(|d| d.get_temp(egui::Id::new("pending_move")));
        if let Some(Some((drag_id, target_id, relation))) = pending_move {
            if let Some(request) = registry.move_request(drag_id, target_id, relation) {
                let _ = tx_worker.send(request);
            }
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

        Self::handle_pending_tree_dnd_moves(ui, registry, tx_worker);
    }
}
