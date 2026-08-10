use serde::{Deserialize, Serialize};

/// Presentation node for structure tree hierarchy in GUI.
pub use fepdf_sdk::StructureTreeNode as USTNode;

#[derive(Serialize, Deserialize)]
pub struct USTRegistry {
    pub root: Option<USTNode>,
    pub selected_node_id: Option<usize>,
    pub next_node_id: usize,
    pub audit_findings: Vec<(String, String, String, Option<u32>)>, // (checkpoint, severity, message, handle_id)
    pub pending_center_node_id: Option<usize>,
}

impl Default for USTRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DragRelation {
    Above,
    Below,
    AsChild,
}

impl USTRegistry {
    pub fn new() -> Self {
        Self {
            root: None,
            selected_node_id: None,
            next_node_id: 1,
            audit_findings: Vec::new(),
            pending_center_node_id: None,
        }
    }

    pub fn clear(&mut self) {
        self.root = None;
        self.selected_node_id = None;
        self.next_node_id = 1;
        self.audit_findings.clear();
        self.pending_center_node_id = None;
    }

    pub fn find_node_id_by_handle_id(&self, handle_id: u32) -> Option<usize> {
        self.root.as_ref().and_then(|r| Self::find_node_id_by_handle_recursive(r, handle_id))
    }

    fn find_node_id_by_handle_recursive(node: &USTNode, handle_id: u32) -> Option<usize> {
        if node.handle_index == Some(handle_id) {
            return Some(node.id);
        }
        for child in &node.children {
            if let Some(id) = Self::find_node_id_by_handle_recursive(child, handle_id) {
                return Some(id);
            }
        }
        None
    }

    /// Resolves a node to the page it sits on and its bounding box in PDF user space.
    ///
    /// Nodes with no resolved `/Pg` fall back to the first page, which is what the
    /// viewport did unconditionally before `USTNode::page_index` existed.
    pub fn find_placement_by_id(&self, id: usize) -> Option<(usize, [f32; 4])> {
        let root = self.root.as_ref()?;
        let (page_index, rect) = Self::find_placement_recursive(root, id)?;
        Some((page_index.unwrap_or(0), rect))
    }

    fn find_placement_recursive(node: &USTNode, id: usize) -> Option<(Option<usize>, [f32; 4])> {
        if node.id == id {
            return node.rect.map(|r| (node.page_index, r));
        }
        for child in &node.children {
            if let Some(found) = Self::find_placement_recursive(child, id) {
                return Some(found);
            }
        }
        None
    }

    pub fn remove_node(&mut self, id: usize) -> Option<USTNode> {
        if let Some(ref mut root) = self.root {
            if root.id == id {
                return None;
            }
            return Self::remove_node_recursive(root, id);
        }
        None
    }

    fn remove_node_recursive(node: &mut USTNode, id: usize) -> Option<USTNode> {
        for idx in 0..node.children.len() {
            if node.children[idx].id == id {
                return Some(node.children.remove(idx));
            }
        }
        for child in &mut node.children {
            if let Some(removed) = Self::remove_node_recursive(child, id) {
                return Some(removed);
            }
        }
        None
    }

    pub fn move_node(
        &mut self,
        dragged_id: usize,
        target_id: usize,
        relation: DragRelation,
    ) -> bool {
        if dragged_id == target_id {
            return false;
        }

        if let Some(ref root) = self.root
            && let Some(dragged_node) = Self::find_node_by_id_recursive(root, dragged_id)
            && Self::is_descendant(dragged_node, target_id)
        {
            return false;
        }

        if let Some(dragged_node) = self.remove_node(dragged_id)
            && let Some(ref mut root) = self.root
            && Self::insert_node_recursive(root, target_id, dragged_node, relation).is_ok()
        {
            return true;
        }
        false
    }

    pub fn find_node_by_id_recursive(current: &USTNode, id: usize) -> Option<&USTNode> {
        if current.id == id {
            return Some(current);
        }
        for child in &current.children {
            if let Some(found) = Self::find_node_by_id_recursive(child, id) {
                return Some(found);
            }
        }
        None
    }

    pub fn is_descendant(parent: &USTNode, target_id: usize) -> bool {
        if parent.id == target_id {
            return true;
        }
        for child in &parent.children {
            if Self::is_descendant(child, target_id) {
                return true;
            }
        }
        false
    }

    fn insert_node_recursive(
        current: &mut USTNode,
        target_id: usize,
        node_to_insert: USTNode,
        relation: DragRelation,
    ) -> Result<(), USTNode> {
        if relation == DragRelation::AsChild && current.id == target_id {
            current.children.push(node_to_insert);
            return Ok(());
        }

        for idx in 0..current.children.len() {
            if current.children[idx].id == target_id {
                match relation {
                    DragRelation::Above => {
                        current.children.insert(idx, node_to_insert);
                        return Ok(());
                    }
                    DragRelation::Below => {
                        current.children.insert(idx + 1, node_to_insert);
                        return Ok(());
                    }
                    DragRelation::AsChild => {
                        current.children[idx].children.push(node_to_insert);
                        return Ok(());
                    }
                }
            }
        }

        let mut temp = Some(node_to_insert);
        for child in &mut current.children {
            if let Some(n) = temp.take() {
                match Self::insert_node_recursive(child, target_id, n, relation) {
                    Ok(()) => return Ok(()),
                    Err(n) => {
                        temp = Some(n);
                    }
                }
            }
        }

        if let Some(n) = temp { Err(n) } else { Ok(()) }
    }
}

#[derive(Clone)]
pub struct FigureInfo {
    pub id: usize,
    pub alt_text: Option<String>,
    pub handle_id: Option<u32>,
}

fn collect_figures(node: &USTNode, figures: &mut Vec<FigureInfo>) {
    if node.tag == "Figure" {
        figures.push(FigureInfo {
            id: node.id,
            alt_text: node.alt_text.clone(),
            handle_id: node.handle_index,
        });
    }
    for child in &node.children {
        collect_figures(child, figures);
    }
}

fn update_alt_text(node: &mut USTNode, id: usize, new_alt: Option<String>) -> bool {
    if node.id == id {
        node.alt_text = new_alt;
        return true;
    }
    for child in &mut node.children {
        if update_alt_text(child, id, new_alt.clone()) {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LeftTab {
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
        locale_mgr: &crate::locale::LocaleManager,
        active_lang: &str,
    ) {
        ui.vertical_centered(|ui| {
            ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 8.0);
            ui.add_space(8.0);

            let tabs = [
                (LeftTab::DocumentInfo, "\u{e0cc}", locale_mgr.tr(active_lang, "tab_doc_info")),
                (LeftTab::Bookmarks, "\u{e060}", locale_mgr.tr(active_lang, "tab_bookmarks")),
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

    pub fn show(
        // RR-15 Limit: GUI - sidebar main routing and sub-panel egui layout tree
        &mut self,
        ui: &mut egui::Ui,
        registry: &mut USTRegistry,
        tx_worker: &std::sync::mpsc::Sender<crate::worker::WorkerRequest>,
        pdf_name: &Option<String>,
        total_pages: usize,
        metadata: &Option<fepdf_core::metadata::MetadataInfo>,
        file_size: Option<usize>,
        pdf_version: &Option<String>,
        security_method: &Option<String>,
        permissions: Option<i32>,
        page_sizes: &[(f64, f64)],
        fonts: &[fepdf_core::font::FontSummary],
        locale_mgr: &crate::locale::LocaleManager,
        active_lang: &str,
    ) {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.vertical(|ui| match self.active_left_tab {
            LeftTab::DocumentInfo => {
                self.show_document_info(
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
                self.show_bookmarks(ui, total_pages, locale_mgr, active_lang);
            }
            LeftTab::Attachments => {
                self.show_attachments(ui, locale_mgr, active_lang);
            }
            LeftTab::Structure => {
                self.show_structure_tree(ui, registry, tx_worker, locale_mgr, active_lang);
            }
            LeftTab::Properties => {
                Self::show_element_properties(ui, registry, tx_worker, locale_mgr, active_lang);
            }
            LeftTab::AltText => {
                self.show_alt_text_gallery(ui, registry, tx_worker, locale_mgr, active_lang);
            }
            LeftTab::Audit => {
                Self::show_accessibility_audit(ui, registry, locale_mgr, active_lang);
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

    #[allow(clippy::too_many_arguments)]
    fn show_document_info(
        &self,
        ui: &mut egui::Ui,
        pdf_name: &Option<String>,
        total_pages: usize,
        metadata: &Option<fepdf_core::metadata::MetadataInfo>,
        file_size: Option<usize>,
        pdf_version: &Option<String>,
        security_method: &Option<String>,
        permissions: Option<i32>,
        page_sizes: &[(f64, f64)],
        fonts: &[fepdf_core::font::FontSummary],
        locale_mgr: &crate::locale::LocaleManager,
        active_lang: &str,
    ) {
        fn format_pdf_date(date_str: &str) -> String {
            if let Some(clean) = date_str.strip_prefix("D:")
                && clean.len() >= 14
            {
                let year = &clean[0..4];
                let month = &clean[4..6];
                let day = &clean[6..8];
                let hour = &clean[8..10];
                let min = &clean[10..12];
                let sec = &clean[12..14];
                return format!("{year}/{month}/{day} {hour}:{min}:{sec}");
            }
            date_str.to_string()
        }

        let format_file_size = |bytes: usize| -> String {
            fn format_num(n: usize) -> String {
                let s = n.to_string();
                let mut result = String::new();
                let len = s.len();
                for (i, c) in s.chars().enumerate() {
                    result.push(c);
                    if (len - i - 1).is_multiple_of(3) && i != len - 1 {
                        result.push(',');
                    }
                }
                result
            }

            if bytes >= 1_048_576 {
                let mb = format!("{:.2}", bytes as f64 / 1_048_576.0);
                if active_lang == "ja" {
                    format!("{} MB ({} バイト)", mb, format_num(bytes))
                } else {
                    format!("{} MB ({} bytes)", mb, format_num(bytes))
                }
            } else if bytes >= 1024 {
                let kb = format!("{:.2}", bytes as f64 / 1024.0);
                if active_lang == "ja" {
                    format!("{} KB ({} バイト)", kb, format_num(bytes))
                } else {
                    format!("{} KB ({} bytes)", kb, format_num(bytes))
                }
            } else if active_lang == "ja" {
                format!("{} バイト", format_num(bytes))
            } else {
                format!("{} bytes", format_num(bytes))
            }
        };

        let format_page_size = |w: f64, h: f64| -> String {
            let w_mm = w * 25.4 / 72.0;
            let h_mm = h * 25.4 / 72.0;

            let is_a4 = (w_mm - 210.0).abs() < 2.0 && (h_mm - 297.0).abs() < 2.0;
            let is_a4_landscape = (w_mm - 297.0).abs() < 2.0 && (h_mm - 210.0).abs() < 2.0;
            let is_letter = (w_mm - 215.9).abs() < 2.0 && (h_mm - 279.4).abs() < 2.0;
            let is_letter_landscape = (w_mm - 279.4).abs() < 2.0 && (h_mm - 215.9).abs() < 2.0;

            let format_name = if is_a4 {
                " (A4)"
            } else if is_a4_landscape {
                if active_lang == "ja" { " (A4 横)" } else { " (A4 Landscape)" }
            } else if is_letter {
                " (Letter)"
            } else if is_letter_landscape {
                if active_lang == "ja" { " (Letter 横)" } else { " (Letter Landscape)" }
            } else {
                ""
            };

            format!("{w_mm:.1} x {h_mm:.1} mm{format_name}")
        };

        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(locale_mgr.tr(active_lang, "info_title")).strong().size(16.0),
            );
            egui::ScrollArea::vertical().id_salt("doc_info_scroll").show(ui, |ui| {
                let render_row = |ui: &mut egui::Ui, key: &str, val: &str, is_val_strong: bool| {
                    ui.label(egui::RichText::new(key).weak());
                    let text = egui::RichText::new(val);
                    let text = if is_val_strong { text.strong() } else { text };
                    ui.add(egui::Label::new(text).truncate());
                    ui.add_space(4.0);
                };

                // 1. 概要 (Description)
                egui::CollapsingHeader::new(
                    egui::RichText::new(locale_mgr.tr(active_lang, "info_summary"))
                        .strong()
                        .size(13.0),
                )
                .default_open(true)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        let filename_key = locale_mgr.tr(active_lang, "info_filename");
                        let filename_val_fallback;
                        let filename_val = match pdf_name.as_deref() {
                            Some(name) => name,
                            None => {
                                filename_val_fallback = "-".to_string();
                                &filename_val_fallback
                            }
                        };
                        render_row(ui, &filename_key, filename_val, true);

                        let empty = "-".to_string();
                        let title = metadata
                            .as_ref()
                            .and_then(|m| m.title.clone())
                            .unwrap_or_else(|| empty.clone());
                        let author = metadata
                            .as_ref()
                            .and_then(|m| m.author.clone())
                            .unwrap_or_else(|| empty.clone());
                        let subject = metadata
                            .as_ref()
                            .and_then(|m| m.subject.clone())
                            .unwrap_or_else(|| empty.clone());
                        let keywords = metadata
                            .as_ref()
                            .and_then(|m| m.keywords.clone())
                            .unwrap_or_else(|| empty.clone());
                        let creator = metadata
                            .as_ref()
                            .and_then(|m| m.creator.clone())
                            .unwrap_or_else(|| empty.clone());
                        let producer = metadata
                            .as_ref()
                            .and_then(|m| m.producer.clone())
                            .unwrap_or_else(|| empty.clone());
                        let created = metadata
                            .as_ref()
                            .and_then(|m| m.creation_date.as_ref().map(|d| format_pdf_date(d)))
                            .unwrap_or_else(|| empty.clone());
                        let modified = metadata
                            .as_ref()
                            .and_then(|m| m.mod_date.as_ref().map(|d| format_pdf_date(d)))
                            .unwrap_or_else(|| empty.clone());

                        render_row(ui, &locale_mgr.tr(active_lang, "info_doc_title"), &title, true);
                        render_row(ui, &locale_mgr.tr(active_lang, "info_author"), &author, true);
                        render_row(ui, &locale_mgr.tr(active_lang, "info_subject"), &subject, true);
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_keywords"),
                            &keywords,
                            true,
                        );
                        render_row(ui, &locale_mgr.tr(active_lang, "info_created"), &created, true);
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_modified"),
                            &modified,
                            true,
                        );
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_application"),
                            &creator,
                            true,
                        );
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_producer"),
                            &producer,
                            true,
                        );
                    });
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // 2. ファイル仕様 (File Specification)
                egui::CollapsingHeader::new(
                    egui::RichText::new(locale_mgr.tr(active_lang, "info_file_spec"))
                        .strong()
                        .size(13.0),
                )
                .default_open(true)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        let ver = if pdf_name.is_none() {
                            "-"
                        } else {
                            pdf_version.as_deref().unwrap_or("1.7")
                        };
                        render_row(ui, &locale_mgr.tr(active_lang, "info_pdf_version"), ver, true);

                        let size_str =
                            file_size.map_or_else(|| "-".to_string(), |s| format_file_size(s));
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_file_size"),
                            &size_str,
                            true,
                        );

                        let count_str = if pdf_name.is_none() {
                            "-".to_string()
                        } else {
                            total_pages.to_string()
                        };
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_page_count"),
                            &count_str,
                            true,
                        );

                        let page_size_str = if let Some(first_size) = page_sizes.first() {
                            let formatted = format_page_size(first_size.0, first_size.1);
                            if page_sizes.len() > 1 {
                                format!(
                                    "{} ({})",
                                    formatted,
                                    locale_mgr.tr(active_lang, "info_other_sizes")
                                )
                            } else {
                                formatted
                            }
                        } else {
                            "-".to_string()
                        };
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_page_size"),
                            &page_size_str,
                            true,
                        );
                    });
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // 3. セキュリティと制限事項 (Security & Restrictions)
                egui::CollapsingHeader::new(
                    egui::RichText::new(locale_mgr.tr(active_lang, "info_security"))
                        .strong()
                        .size(13.0),
                )
                .default_open(true)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        let method_fallback;
                        let method = match security_method.as_deref() {
                            Some(m) => m,
                            None => {
                                if pdf_name.is_none() {
                                    "-"
                                } else {
                                    method_fallback = locale_mgr.tr(active_lang, "info_sec_none");
                                    &method_fallback
                                }
                            }
                        };
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_security_method"),
                            method,
                            true,
                        );

                        // Permissions bits helper
                        let has_perm = |bit: i32| -> String {
                            if pdf_name.is_none() {
                                "-".to_string()
                            } else if let Some(p) = permissions {
                                if (p & bit) != 0 {
                                    locale_mgr.tr(active_lang, "info_perm_allowed")
                                } else {
                                    locale_mgr.tr(active_lang, "info_perm_not_allowed")
                                }
                            } else {
                                locale_mgr.tr(active_lang, "info_perm_allowed")
                            }
                        };

                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_print"),
                            &has_perm(4),
                            true,
                        );
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_modify"),
                            &has_perm(8),
                            true,
                        );
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_copy"),
                            &has_perm(16),
                            true,
                        );
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_accessibility_copy"),
                            &has_perm(512),
                            true,
                        );
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_extract"),
                            &has_perm(16),
                            true,
                        );
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_annotation"),
                            &has_perm(32),
                            true,
                        );
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_form"),
                            &has_perm(256),
                            true,
                        );
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_sign"),
                            &has_perm(256),
                            true,
                        );
                        render_row(
                            ui,
                            &locale_mgr.tr(active_lang, "info_assembly"),
                            &has_perm(1024),
                            true,
                        );
                    });
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // 4. フォント情報 (Fonts)
                let fonts_title = locale_mgr
                    .tr(active_lang, "info_fonts_in_use")
                    .replace("{}", &fonts.len().to_string());
                egui::CollapsingHeader::new(egui::RichText::new(fonts_title).strong().size(13.0))
                    .default_open(false)
                    .show(ui, |ui| {
                        if fonts.is_empty() {
                            ui.label(
                                egui::RichText::new(locale_mgr.tr(active_lang, "info_no_fonts"))
                                    .weak(),
                            );
                        } else {
                            for font in fonts {
                                ui.vertical(|ui| {
                                    let embed_status = if font.is_embedded {
                                        if font.is_subset {
                                            locale_mgr.tr(active_lang, "info_font_embedded_subset")
                                        } else {
                                            locale_mgr.tr(active_lang, "info_font_embedded")
                                        }
                                    } else {
                                        String::new()
                                    };
                                    let label_text = format!("🔠 {}{}", font.name, embed_status);
                                    ui.add(
                                        egui::Label::new(egui::RichText::new(label_text).strong())
                                            .truncate(),
                                    );
                                    ui.indent("font_details", |ui| {
                                        let type_text = locale_mgr
                                            .tr(active_lang, "info_font_type")
                                            .replace("{}", &font.font_type);
                                        ui.add(
                                            egui::Label::new(egui::RichText::new(type_text).weak())
                                                .truncate(),
                                        );
                                        let enc_text = locale_mgr
                                            .tr(active_lang, "info_font_encoding")
                                            .replace("{}", &font.encoding);
                                        ui.add(
                                            egui::Label::new(egui::RichText::new(enc_text).weak())
                                                .truncate(),
                                        );
                                    });
                                    ui.add_space(4.0);
                                });
                            }
                        }
                    });
            });
        });
    }

    fn show_bookmarks(
        &self,
        ui: &mut egui::Ui,
        total_pages: usize,
        locale_mgr: &crate::locale::LocaleManager,
        active_lang: &str,
    ) {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(locale_mgr.tr(active_lang, "bookmarks_title"))
                    .strong()
                    .size(13.0),
            );
            ui.add_space(6.0);

            if total_pages == 0 {
                ui.label(egui::RichText::new(locale_mgr.tr(active_lang, "no_doc_loaded")).weak());
            } else {
                egui::ScrollArea::vertical().id_salt("bookmarks_scroll").show(ui, |ui| {
                    for i in 1..=total_pages.min(5) {
                        if ui.selectable_label(false, format!("🔖 Chapter {i}: Page {i}")).clicked()
                        {
                            // Navigator target page trigger
                        }
                    }
                    if total_pages > 5 {
                        ui.label(egui::RichText::new("...").weak());
                    }
                });
            }
        });
    }

    fn show_attachments(
        &self,
        ui: &mut egui::Ui,
        locale_mgr: &crate::locale::LocaleManager,
        active_lang: &str,
    ) {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(locale_mgr.tr(active_lang, "attachments_title"))
                    .strong()
                    .size(13.0),
            );
            ui.add_space(6.0);

            ui.label(egui::RichText::new(locale_mgr.tr(active_lang, "attachments_none")).weak());
            ui.add_space(8.0);
            if ui.button(locale_mgr.tr(active_lang, "attachments_add")).clicked() {
                // Future file dialog attachment trigger
            }
        });
    }

    fn show_structure_tree(
        &mut self,
        ui: &mut egui::Ui,
        registry: &mut USTRegistry,
        tx_worker: &std::sync::mpsc::Sender<crate::worker::WorkerRequest>,
        locale_mgr: &crate::locale::LocaleManager,
        active_lang: &str,
    ) {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(locale_mgr.tr(active_lang, "structure_tree_title"))
                    .strong()
                    .size(13.0),
            );
            ui.add_space(4.0);

            let mut selected_node_id = registry.selected_node_id;
            egui::ScrollArea::vertical().id_salt("tag_tree_scroll").max_height(160.0).show(
                ui,
                |ui| {
                    if let Some(ref mut root) = registry.root {
                        Self::render_node_recursive(
                            ui,
                            root,
                            &mut selected_node_id,
                            &mut self.alt_text_edit_buffer,
                            tx_worker,
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(locale_mgr.tr(active_lang, "structure_tree_none"))
                                .weak(),
                        );
                    }
                },
            );
            registry.selected_node_id = selected_node_id;
        });
    }

    fn show_element_properties(
        // RR-15 Limit: GUI - Render properties grid for selected UST node
        ui: &mut egui::Ui,
        registry: &mut USTRegistry,
        tx_worker: &std::sync::mpsc::Sender<crate::worker::WorkerRequest>,
        locale_mgr: &crate::locale::LocaleManager,
        active_lang: &str,
    ) {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(locale_mgr.tr(active_lang, "element_properties_title"))
                    .strong()
                    .size(13.0),
            );
            ui.add_space(6.0);

            let selected_id = registry.selected_node_id;
            let mut node_found = false;

            if let Some(id) = selected_id
                && let Some(ref mut root) = registry.root
                && let Some(node) = Self::find_node_mut_recursive(root, id)
            {
                node_found = true;
                egui::Grid::new("properties_grid")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(locale_mgr.tr(active_lang, "element_prop_tag"))
                                .weak(),
                        );
                        let old_tag = node.tag.clone();
                        egui::ComboBox::from_id_salt("properties_tag_combobox")
                            .selected_text(&node.tag)
                            .show_ui(ui, |ui| {
                                for t in &[
                                    "H1", "H2", "P", "Figure", "Table", "List", "Part", "Document",
                                ] {
                                    ui.selectable_value(&mut node.tag, t.to_string(), *t);
                                }
                            });
                        if node.tag != old_tag
                            && let Some(h_id) = node.handle_index
                        {
                            let _ = tx_worker.send(crate::worker::WorkerRequest::UpdateNode {
                                handle_id: h_id,
                                tag: node.tag.clone(),
                                alt_text: node.alt_text.clone(),
                            });
                        }
                        ui.end_row();

                        ui.label(
                            egui::RichText::new(locale_mgr.tr(active_lang, "element_prop_title"))
                                .weak(),
                        );
                        ui.label(egui::RichText::new(&node.title).strong());
                        ui.end_row();

                        ui.label(
                            egui::RichText::new(locale_mgr.tr(active_lang, "element_prop_bbox"))
                                .weak(),
                        );
                        if let Some(rect) = node.rect {
                            ui.monospace(format!(
                                "[{:.1}, {:.1}, {:.1}, {:.1}]",
                                rect[0], rect[1], rect[2], rect[3]
                            ));
                        } else {
                            ui.monospace("None");
                        }
                        ui.end_row();

                        ui.label(
                            egui::RichText::new(locale_mgr.tr(active_lang, "element_prop_lang"))
                                .weak(),
                        );
                        ui.label("en-US");
                        ui.end_row();

                        ui.label(
                            egui::RichText::new(
                                locale_mgr.tr(active_lang, "element_prop_role_map"),
                            )
                            .weak(),
                        );
                        ui.label("Default Mapping");
                        ui.end_row();

                        ui.label(
                            egui::RichText::new(
                                locale_mgr.tr(active_lang, "element_prop_alt_text"),
                            )
                            .weak(),
                        );
                        let mut buf = node.alt_text.clone().unwrap_or_default();
                        let text_resp = ui.text_edit_singleline(&mut buf);
                        if text_resp.changed() {
                            node.alt_text = if buf.trim().is_empty() { None } else { Some(buf) };
                            if let Some(h_id) = node.handle_index {
                                let _ = tx_worker.send(crate::worker::WorkerRequest::UpdateNode {
                                    handle_id: h_id,
                                    tag: node.tag.clone(),
                                    alt_text: node.alt_text.clone(),
                                });
                            }
                        }
                        ui.end_row();
                    });
            }

            if !node_found {
                ui.label(
                    egui::RichText::new(locale_mgr.tr(active_lang, "element_properties_none"))
                        .weak(),
                );
            }
        });
    }

    fn show_accessibility_audit(
        // RR-15 Limit: GUI - Render accessibility audit findings panel
        ui: &mut egui::Ui,
        registry: &mut USTRegistry,
        locale_mgr: &crate::locale::LocaleManager,
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
                    ui.label(
                        egui::RichText::new(locale_mgr.tr(active_lang, "no_doc_loaded")).weak(),
                    );
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
                        let response =
                            ui.interact(card_resp.response.rect, id, egui::Sense::click());
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

    fn find_node_mut_recursive(node: &mut USTNode, id: usize) -> Option<&mut USTNode> {
        if node.id == id {
            return Some(node);
        }
        for child in &mut node.children {
            if let Some(found) = Self::find_node_mut_recursive(child, id) {
                return Some(found);
            }
        }
        None
    }

    fn render_drag_drop_controls(ui: &mut egui::Ui, node_id: usize, node: &USTNode) {
        let handle_resp = ui.add(egui::Label::new("Drag").sense(egui::Sense::drag()));
        if handle_resp.drag_started() {
            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("dragged_node_id"), Some(node_id)));
        }

        let dragged_id: Option<Option<usize>> =
            ui.ctx().data(|d| d.get_temp(egui::Id::new("dragged_node_id")));
        if let Some(Some(drag_id)) = dragged_id
            && drag_id != node_id
            && !USTRegistry::is_descendant(node, drag_id)
        {
            let resp_above = ui.button("Above");
            if resp_above.clicked()
                || (resp_above.hovered() && ui.input(|i| i.pointer.any_released()))
            {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new("pending_move"),
                        Some((drag_id, node_id, DragRelation::Above)),
                    )
                });
            }
            let resp_child = ui.button("Child");
            if resp_child.clicked()
                || (resp_child.hovered() && ui.input(|i| i.pointer.any_released()))
            {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new("pending_move"),
                        Some((drag_id, node_id, DragRelation::AsChild)),
                    )
                });
            }
            let resp_below = ui.button("Below");
            if resp_below.clicked()
                || (resp_below.hovered() && ui.input(|i| i.pointer.any_released()))
            {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new("pending_move"),
                        Some((drag_id, node_id, DragRelation::Below)),
                    )
                });
            }
        }
    }

    fn render_node_buttons(
        ui: &mut egui::Ui,
        node: &mut USTNode,
        selected_node_id: &mut Option<usize>,
        alt_edit_buf: &mut String,
        tx_worker: &std::sync::mpsc::Sender<crate::worker::WorkerRequest>,
    ) {
        if ui.button("Edit").clicked() {
            *selected_node_id = Some(node.id);
            *alt_edit_buf = node.alt_text.clone().unwrap_or_default();
        }

        if ui.button("Cycle").clicked() {
            node.tag = match node.tag.as_str() {
                "H1" => "H2".to_string(),
                "H2" => "P".to_string(),
                "P" => "H1".to_string(),
                _ => "P".to_string(),
            };
            if let Some(h_id) = node.handle_index {
                let _ = tx_worker.send(crate::worker::WorkerRequest::UpdateNode {
                    handle_id: h_id,
                    tag: node.tag.clone(),
                    alt_text: node.alt_text.clone(),
                });
            }
        }
    }

    fn render_node_recursive(
        // RR-15 Limit: GUI - Renders accessibility tag node tree recursively
        ui: &mut egui::Ui,
        node: &mut USTNode,
        selected_node_id: &mut Option<usize>,
        alt_edit_buf: &mut String,
        tx_worker: &std::sync::mpsc::Sender<crate::worker::WorkerRequest>,
    ) {
        let is_selected = *selected_node_id == Some(node.id);
        let header_label = format!("<{}> {}", node.tag, node.title);

        ui.vertical(|ui| {
            let id = ui.make_persistent_id(node.id);
            let mut collapsing = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                false,
            );

            let header_response = ui
                .horizontal(|ui| {
                    Self::render_drag_drop_controls(ui, node.id, node);

                    let is_open = collapsing.is_open();
                    let symbol = if is_open { "⏷" } else { "⏵" };
                    if ui.small_button(symbol).clicked() {
                        collapsing.toggle(ui);
                    }

                    let rich_text = if is_selected {
                        egui::RichText::new(&header_label)
                            .color(egui::Color32::from_rgb(240, 165, 0))
                            .strong()
                    } else {
                        egui::RichText::new(&header_label)
                    };

                    if ui.selectable_label(is_selected, rich_text).clicked() {
                        *selected_node_id = Some(node.id);
                        *alt_edit_buf = node.alt_text.clone().unwrap_or_default();
                    }

                    if is_selected {
                        Self::render_node_buttons(
                            ui,
                            node,
                            selected_node_id,
                            alt_edit_buf,
                            tx_worker,
                        );
                    }
                })
                .response;

            collapsing.show_body_indented(&header_response, ui, |ui| {
                let children_len = node.children.len();
                for idx in 0..children_len {
                    let child = &mut node.children[idx];
                    Self::render_node_recursive(
                        ui,
                        child,
                        selected_node_id,
                        alt_edit_buf,
                        tx_worker,
                    );
                }
            });
        });
    }

    fn show_alt_text_gallery(
        // RR-15 Limit: GUI - Renders a carousel list of figure elements and their Alt text cards
        &mut self,
        ui: &mut egui::Ui,
        registry: &mut USTRegistry,
        tx_worker: &std::sync::mpsc::Sender<crate::worker::WorkerRequest>,
        locale_mgr: &crate::locale::LocaleManager,
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
                                    let response = ui
                                        .add(egui::TextEdit::singleline(&mut buf).hint_text(hint));

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
                                            let _ = tx_worker.send(
                                                crate::worker::WorkerRequest::UpdateNode {
                                                    handle_id: h_id,
                                                    tag: "Figure".to_string(),
                                                    alt_text: new_alt,
                                                },
                                            );
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_node() {
        let mut registry = USTRegistry::new();
        let doc_node = USTNode {
            id: 0,
            tag: "Document".to_string(),
            title: "PDF Document Catalog".to_string(),
            alt_text: None,
            rect: None,
            page_index: None,
            handle_index: None,
            children: vec![USTNode {
                id: 1,
                tag: "Part".to_string(),
                title: "Page 1 Section".to_string(),
                alt_text: None,
                rect: None,
                page_index: None,
                handle_index: None,
                children: vec![
                    USTNode {
                        id: 2,
                        tag: "H1".to_string(),
                        title: "Heading of Page 1".to_string(),
                        alt_text: None,
                        rect: None,
                        page_index: None,
                        handle_index: None,
                        children: Vec::new(),
                    },
                    USTNode {
                        id: 3,
                        tag: "P".to_string(),
                        title: "Paragraph content for page 1".to_string(),
                        alt_text: None,
                        rect: None,
                        page_index: None,
                        handle_index: None,
                        children: Vec::new(),
                    },
                    USTNode {
                        id: 4,
                        tag: "Figure".to_string(),
                        title: "Illustration on page 1".to_string(),
                        alt_text: None,
                        rect: None,
                        page_index: None,
                        handle_index: None,
                        children: Vec::new(),
                    },
                ],
            }],
        };
        registry.root = Some(doc_node);
        registry.next_node_id = 5;

        // Move Paragraph (id 3) Above Heading (id 2)
        assert!(registry.move_node(3, 2, DragRelation::Above));

        let root = registry.root.as_ref().unwrap();
        let page = &root.children[0];
        assert_eq!(page.children[0].id, 3);
        assert_eq!(page.children[1].id, 2);

        // Move Illustration (id 4) As Child of Paragraph (id 3)
        assert!(registry.move_node(4, 3, DragRelation::AsChild));

        let root = registry.root.as_ref().unwrap();
        let page = &root.children[0];
        let para = &page.children[0];
        assert_eq!(para.children[0].id, 4);

        // Invalid moves: dragging parent to child should fail
        assert!(!registry.move_node(3, 4, DragRelation::Above));
    }

    fn node(id: usize, page_index: Option<usize>, rect: Option<[f32; 4]>) -> USTNode {
        USTNode {
            id,
            tag: "P".to_string(),
            title: format!("node {id}"),
            alt_text: None,
            rect,
            page_index,
            handle_index: None,
            children: Vec::new(),
        }
    }

    fn registry_with(children: Vec<USTNode>) -> USTRegistry {
        let mut registry = USTRegistry::new();
        let mut root = node(0, None, None);
        root.children = children;
        registry.root = Some(root);
        registry
    }

    #[test]
    fn find_placement_reports_the_node_own_page() {
        // Regression: the viewport used to hardcode page 0, so selecting a tag on a
        // later page highlighted and scrolled to the first page instead.
        let registry = registry_with(vec![
            node(1, Some(0), Some([10.0, 20.0, 30.0, 40.0])),
            node(2, Some(4), Some([50.0, 60.0, 70.0, 80.0])),
        ]);

        assert_eq!(registry.find_placement_by_id(1), Some((0, [10.0, 20.0, 30.0, 40.0])));
        assert_eq!(registry.find_placement_by_id(2), Some((4, [50.0, 60.0, 70.0, 80.0])));
    }

    #[test]
    fn find_placement_falls_back_to_first_page_when_pg_unresolved() {
        // Tags parsed from a PDF whose /Pg could not be resolved keep the old
        // behaviour rather than disappearing from the viewport.
        let registry = registry_with(vec![node(1, None, Some([1.0, 2.0, 3.0, 4.0]))]);
        assert_eq!(registry.find_placement_by_id(1), Some((0, [1.0, 2.0, 3.0, 4.0])));
    }

    #[test]
    fn find_placement_searches_nested_nodes() {
        let mut branch = node(1, Some(1), None);
        branch.children = vec![node(2, Some(7), Some([5.0, 5.0, 6.0, 6.0]))];
        let registry = registry_with(vec![branch]);
        assert_eq!(registry.find_placement_by_id(2), Some((7, [5.0, 5.0, 6.0, 6.0])));
    }

    #[test]
    fn find_placement_returns_none_without_a_rect_or_a_match() {
        let registry = registry_with(vec![node(1, Some(3), None)]);
        // A node carrying no bounding box has nothing to highlight.
        assert_eq!(registry.find_placement_by_id(1), None);
        assert_eq!(registry.find_placement_by_id(99), None);
    }

    #[test]
    fn ust_node_page_index_defaults_when_absent_from_a_draft() {
        // UST drafts written before page_index existed must still deserialize.
        let legacy = r#"{
            "id": 3,
            "tag": "H1",
            "title": "legacy",
            "alt_text": null,
            "rect": null,
            "handle_id": null,
            "children": []
        }"#;
        let parsed: USTNode = serde_json::from_str(legacy).expect("legacy draft should load");
        assert_eq!(parsed.page_index, None);
        assert_eq!(parsed.id, 3);
    }
}
