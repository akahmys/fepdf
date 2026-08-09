#[derive(Clone, Debug)]
pub struct InspectorEntry {
    pub key: String,
    pub val_type: String,
    pub raw_value: String,
    pub compliance_rule: &'static str,
    pub warning: Option<&'static str>,
}

pub struct ArlingtonInspectorPanel {
    pub active_object_name: String,
    pub search_query: String,
}

impl ArlingtonInspectorPanel {
    pub fn new() -> Self {
        Self { active_object_name: "Catalog (Root)".to_string(), search_query: String::new() }
    }

    /// Simulates PDF dictionary structures for the selected tag/node.
    fn get_mock_dictionary_for_node(&self, tag: &str) -> Vec<InspectorEntry> {
        // RR-15 Limit: Dispatcher - Flat mapping of node types to mock dictionary elements
        match tag {
            "Catalog" | "Document" => vec![
                InspectorEntry {
                    key: "Type".to_string(),
                    val_type: "Name".to_string(),
                    raw_value: "/Catalog".to_string(),
                    compliance_rule: "ISO 32000-2: Required. Must be /Catalog.",
                    warning: None,
                },
                InspectorEntry {
                    key: "Version".to_string(),
                    val_type: "Name".to_string(),
                    raw_value: "/2.0".to_string(),
                    compliance_rule: "ISO 32000-2: Specifies PDF 2.0 version.",
                    warning: None,
                },
                InspectorEntry {
                    key: "MarkInfo".to_string(),
                    val_type: "Dictionary".to_string(),
                    raw_value: "<< /Marked true >>".to_string(),
                    compliance_rule: "PDF/UA-2: Required. Must specify /Marked true.",
                    warning: None,
                },
                InspectorEntry {
                    key: "Lang".to_string(),
                    val_type: "String".to_string(),
                    raw_value: "()".to_string(),
                    compliance_rule: "PDF/UA-2: Primary natural language. Must not be empty.",
                    warning: Some(
                        "🚨 Empty language catalog. Natural language string must be defined for text synthesis!",
                    ),
                },
            ],
            "Figure" => vec![
                InspectorEntry {
                    key: "Type".to_string(),
                    val_type: "Name".to_string(),
                    raw_value: "/StructElem".to_string(),
                    compliance_rule: "Required type for structure elements.",
                    warning: None,
                },
                InspectorEntry {
                    key: "S".to_string(),
                    val_type: "Name".to_string(),
                    raw_value: "/Figure".to_string(),
                    compliance_rule: "Tag identifier representing visual figures.",
                    warning: None,
                },
                InspectorEntry {
                    key: "Alt".to_string(),
                    val_type: "String".to_string(),
                    raw_value: "None".to_string(),
                    compliance_rule: "PDF/UA-2 Checkpoint 3.1: Alternative descriptions required for non-text objects.",
                    warning: Some(
                        "🚨 Matterhorn violation: Alternative description (Alt) missing for /Figure structural element!",
                    ),
                },
            ],
            _ => vec![
                InspectorEntry {
                    key: "Type".to_string(),
                    val_type: "Name".to_string(),
                    raw_value: "/StructElem".to_string(),
                    compliance_rule: "Required type.",
                    warning: None,
                },
                InspectorEntry {
                    key: "S".to_string(),
                    val_type: "Name".to_string(),
                    raw_value: format!("/{tag}"),
                    compliance_rule: "Semantic tag designation.",
                    warning: None,
                },
                InspectorEntry {
                    key: "Pg".to_string(),
                    val_type: "IndirectRef".to_string(),
                    raw_value: "3 0 R".to_string(),
                    compliance_rule: "Page object dictionary mapping reference.",
                    warning: None,
                },
            ],
        }
    }

    pub fn show(
        // RR-15 Limit: GUI - Arlington Dictionary Inspector panel show
        &mut self,
        ui: &mut egui::Ui,
        selected_tag: Option<&str>,
        locale_mgr: &crate::locale::LocaleManager,
        active_lang: &str,
    ) {
        let tag = selected_tag.unwrap_or("Catalog");
        self.active_object_name = format!("Dictionary: <{tag}>");

        ui.vertical(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            ui.heading(locale_mgr.tr(active_lang, "inspector_title"));
            ui.add_space(5.0);

            // Filter search input
            ui.horizontal(|ui| {
                ui.label(locale_mgr.tr(active_lang, "inspector_filter"));
                ui.text_edit_singleline(&mut self.search_query);
                if ui.button(locale_mgr.tr(active_lang, "inspector_clear")).clicked() {
                    self.search_query.clear();
                }
            });

            ui.separator();
            ui.colored_label(egui::Color32::LIGHT_BLUE, &self.active_object_name);
            ui.add_space(5.0);

            let entries = self.get_mock_dictionary_for_node(tag);

            egui::ScrollArea::vertical().id_salt("inspector_scroll").show(ui, |ui| {
                for entry in entries {
                    if !self.search_query.is_empty()
                        && !entry.key.to_lowercase().contains(&self.search_query.to_lowercase())
                    {
                        continue;
                    }

                    let frame = egui::Frame::NONE
                        .fill(ui.style().visuals.extreme_bg_color)
                        .stroke(egui::Stroke::new(
                            1.0_f32,
                            ui.style().visuals.widgets.noninteractive.bg_stroke.color,
                        ))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::same(8));

                    frame.show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                // Key name in bold primary color
                                ui.colored_label(
                                    egui::Color32::from_rgb(230, 90, 0),
                                    egui::RichText::new(&entry.key).strong(),
                                );

                                // Type badge
                                let badge_color = match entry.val_type.as_str() {
                                    "Name" => egui::Color32::from_rgb(0, 120, 200),
                                    "Dictionary" => egui::Color32::from_rgb(120, 50, 180),
                                    "String" => egui::Color32::from_rgb(0, 150, 80),
                                    _ => egui::Color32::from_rgb(100, 100, 100),
                                };
                                let badge_frame = egui::Frame::NONE
                                    .fill(badge_color.linear_multiply(0.1))
                                    .stroke(egui::Stroke::new(1.0_f32, badge_color))
                                    .corner_radius(3.0)
                                    .inner_margin(egui::Margin::symmetric(4, 2));
                                badge_frame.show(ui, |ui| {
                                    ui.colored_label(
                                        badge_color,
                                        egui::RichText::new(&entry.val_type).small().strong(),
                                    );
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.monospace(
                                            egui::RichText::new(&entry.raw_value).strong(),
                                        );
                                    },
                                );
                            });

                            ui.add_space(4.0);
                            ui.label(egui::RichText::new(entry.compliance_rule).small().weak());

                            if let Some(warn) = entry.warning {
                                ui.add_space(6.0);
                                let warn_frame = egui::Frame::NONE
                                    .fill(egui::Color32::from_rgba_unmultiplied(255, 200, 200, 30))
                                    .stroke(egui::Stroke::new(
                                        1.0_f32,
                                        egui::Color32::from_rgb(255, 80, 80),
                                    ))
                                    .corner_radius(4.0)
                                    .inner_margin(egui::Margin::same(6));
                                warn_frame.show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.colored_label(
                                            egui::Color32::from_rgb(255, 80, 80),
                                            egui::RichText::new(warn).small(),
                                        );
                                    });
                                });
                            }
                        });
                    });
                    ui.add_space(5.0);
                }
            });
        });
    }
}
