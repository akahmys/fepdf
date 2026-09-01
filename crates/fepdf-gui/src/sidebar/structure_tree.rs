use super::ust_registry::{DragRelation, USTNode, USTRegistry};
use crate::locale::LocaleManager;
use crate::worker::WorkerRequest;
use std::sync::mpsc::Sender;

pub fn show_structure_tree(
    ui: &mut egui::Ui,
    registry: &mut USTRegistry,
    alt_text_edit_buffer: &mut String,
    tx_worker: &Sender<WorkerRequest>,
    locale_mgr: &LocaleManager,
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
        egui::ScrollArea::vertical().id_salt("tag_tree_scroll").max_height(160.0).show(ui, |ui| {
            if let Some(ref mut root) = registry.root {
                render_node_recursive(
                    ui,
                    root,
                    &mut selected_node_id,
                    alt_text_edit_buffer,
                    tx_worker,
                );
            } else {
                ui.label(
                    egui::RichText::new(locale_mgr.tr(active_lang, "structure_tree_none")).weak(),
                );
            }
        });
        registry.selected_node_id = selected_node_id;
    });
}

pub fn show_element_properties(
    // RR-15 Limit: GUI - Render properties grid for selected UST node
    ui: &mut egui::Ui,
    registry: &mut USTRegistry,
    tx_worker: &Sender<WorkerRequest>,
    locale_mgr: &LocaleManager,
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
            && let Some(node) = find_node_mut_recursive(root, id)
        {
            node_found = true;
            egui::Grid::new("properties_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(locale_mgr.tr(active_lang, "element_prop_tag")).weak(),
                    );
                    let old_tag = node.tag.clone();
                    egui::ComboBox::from_id_salt("properties_tag_combobox")
                        .selected_text(&node.tag)
                        .show_ui(ui, |ui| {
                            for t in
                                &["H1", "H2", "P", "Figure", "Table", "List", "Part", "Document"]
                            {
                                ui.selectable_value(&mut node.tag, t.to_string(), *t);
                            }
                        });
                    if node.tag != old_tag
                        && let Some(h_id) = node.handle_index
                    {
                        let _ = tx_worker.send(WorkerRequest::UpdateNode {
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
                        egui::RichText::new(locale_mgr.tr(active_lang, "element_prop_bbox")).weak(),
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
                        egui::RichText::new(locale_mgr.tr(active_lang, "element_prop_lang")).weak(),
                    );
                    ui.label("en-US");
                    ui.end_row();

                    ui.label(
                        egui::RichText::new(locale_mgr.tr(active_lang, "element_prop_role_map"))
                            .weak(),
                    );
                    ui.label("Default Mapping");
                    ui.end_row();

                    ui.label(
                        egui::RichText::new(locale_mgr.tr(active_lang, "element_prop_alt_text"))
                            .weak(),
                    );
                    let mut buf = node.alt_text.clone().unwrap_or_default();
                    let text_resp = ui.text_edit_singleline(&mut buf);
                    if text_resp.changed() {
                        node.alt_text = if buf.trim().is_empty() { None } else { Some(buf) };
                        if let Some(h_id) = node.handle_index {
                            let _ = tx_worker.send(WorkerRequest::UpdateNode {
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
                egui::RichText::new(locale_mgr.tr(active_lang, "element_properties_none")).weak(),
            );
        }
    });
}

pub fn find_node_mut_recursive(node: &mut USTNode, id: usize) -> Option<&mut USTNode> {
    if node.id == id {
        return Some(node);
    }
    for child in &mut node.children {
        if let Some(found) = find_node_mut_recursive(child, id) {
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
        if resp_above.clicked() || (resp_above.hovered() && ui.input(|i| i.pointer.any_released()))
        {
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    egui::Id::new("pending_move"),
                    Some((drag_id, node_id, DragRelation::Above)),
                )
            });
        }
        let resp_child = ui.button("Child");
        if resp_child.clicked() || (resp_child.hovered() && ui.input(|i| i.pointer.any_released()))
        {
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    egui::Id::new("pending_move"),
                    Some((drag_id, node_id, DragRelation::AsChild)),
                )
            });
        }
        let resp_below = ui.button("Below");
        if resp_below.clicked() || (resp_below.hovered() && ui.input(|i| i.pointer.any_released()))
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
    tx_worker: &Sender<WorkerRequest>,
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
            let _ = tx_worker.send(WorkerRequest::UpdateNode {
                handle_id: h_id,
                tag: node.tag.clone(),
                alt_text: node.alt_text.clone(),
            });
        }
    }
}

pub fn render_node_recursive(
    // RR-15 Limit: GUI - Renders accessibility tag node tree recursively
    ui: &mut egui::Ui,
    node: &mut USTNode,
    selected_node_id: &mut Option<usize>,
    alt_edit_buf: &mut String,
    tx_worker: &Sender<WorkerRequest>,
) {
    let is_selected = *selected_node_id == Some(node.id);
    let header_label = format!("<{}> {}", node.tag, node.title);

    ui.vertical(|ui| {
        let id = ui.make_persistent_id(node.id);
        let mut collapsing =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);

        let header_response = ui
            .horizontal(|ui| {
                render_drag_drop_controls(ui, node.id, node);

                let is_open = collapsing.is_open();
                let symbol = if is_open { "⏷" } else { "⏵" };
                if ui.small_button(symbol).clicked() {
                    collapsing.toggle(ui);
                }

                let rich_text = if is_selected {
                    egui::RichText::new(&header_label)
                        .color(crate::app::theme::colors::RUST_PRIMARY)
                        .strong()
                } else {
                    egui::RichText::new(&header_label)
                };

                if ui.selectable_label(is_selected, rich_text).clicked() {
                    *selected_node_id = Some(node.id);
                    *alt_edit_buf = node.alt_text.clone().unwrap_or_default();
                }

                if is_selected {
                    render_node_buttons(ui, node, selected_node_id, alt_edit_buf, tx_worker);
                }
            })
            .response;

        collapsing.show_body_indented(&header_response, ui, |ui| {
            let children_len = node.children.len();
            for idx in 0..children_len {
                let child = &mut node.children[idx];
                render_node_recursive(ui, child, selected_node_id, alt_edit_buf, tx_worker);
            }
        });
    });
}
