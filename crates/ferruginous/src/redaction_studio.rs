use crate::interaction::TextSpan;
use crate::redaction::{RedactionManager, RedactionZone};
use regex::Regex;
use std::collections::BTreeMap;

pub struct SearchMatch {
    pub page_index: usize,
    pub term: String,
    pub rect: egui::Rect,
    pub checked: bool,
}

pub struct RedactionStudioPanel {
    pub search_query: String,
    pub error_msg: Option<String>,
    pub matches: Vec<SearchMatch>,
    pub case_sensitive: bool,
    pub use_regex: bool,
}

impl Default for RedactionStudioPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl RedactionStudioPanel {
    pub fn new() -> Self {
        Self {
            search_query: String::new(),
            error_msg: None,
            matches: Vec::new(),
            case_sensitive: false,
            use_regex: false,
        }
    }

    pub fn show( // RR-15 Limit: GUI - Sequential egui declarations for Redaction Studio window layout
        &mut self,
        ui: &mut egui::Ui,
        raw_texts: &BTreeMap<usize, String>,
        page_spans: &BTreeMap<usize, Vec<TextSpan>>,
        redaction_manager: &mut RedactionManager,
    ) {
        ui.vertical(|ui| {
            ui.heading("🔍 Regex Redaction Studio");
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label("Pattern:");
                if ui.text_edit_singleline(&mut self.search_query).changed() {
                    self.perform_search(raw_texts, page_spans);
                }
            });

            ui.horizontal(|ui| {
                if ui.checkbox(&mut self.use_regex, "Regex").changed() {
                    self.perform_search(raw_texts, page_spans);
                }
                if ui.checkbox(&mut self.case_sensitive, "Match Case").changed() {
                    self.perform_search(raw_texts, page_spans);
                }
            });

            if let Some(err) = &self.error_msg {
                ui.colored_label(egui::Color32::RED, err);
            }

            ui.separator();

            if !self.matches.is_empty() {
                ui.horizontal(|ui| {
                    if ui.button("Select All").clicked() {
                        for m in &mut self.matches {
                            m.checked = true;
                        }
                    }
                    if ui.button("Clear Selection").clicked() {
                        for m in &mut self.matches {
                            m.checked = false;
                        }
                    }
                    if ui.button("🔏 Redact Selected").clicked() {
                        for m in &self.matches {
                            if m.checked {
                                redaction_manager.zones.push(RedactionZone {
                                    page_index: m.page_index,
                                    rect: m.rect,
                                });
                            }
                        }
                        self.matches.clear();
                        self.search_query.clear();
                    }
                });

                ui.separator();

                egui::ScrollArea::vertical().id_salt("regex_matches_scroll").show(ui, |ui| {
                    let mut to_toggle = Vec::new();
                    for (idx, m) in self.matches.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let mut checked = m.checked;
                            if ui.checkbox(&mut checked, "").changed() {
                                to_toggle.push((idx, checked));
                            }
                            ui.label(format!("Page {}: {}", m.page_index + 1, m.term));
                        });
                    }
                    for (idx, state) in to_toggle {
                        self.matches[idx].checked = state;
                    }
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("No active search findings. Input search terms above.");
                });
            }
        });
    }

    fn perform_regex_search(
        &mut self,
        raw_texts: &BTreeMap<usize, String>,
        page_spans: &BTreeMap<usize, Vec<TextSpan>>,
        pattern: &str,
    ) {
        match Regex::new(pattern) {
            Ok(re) => {
                for (&page_idx, text) in raw_texts {
                    for m in re.find_iter(text) {
                        let matched_str = m.as_str();
                        if let Some(spans) = page_spans.get(&page_idx) {
                            for span in spans {
                                if span.text.contains(matched_str)
                                    || matched_str.contains(&span.text)
                                {
                                    self.matches.push(SearchMatch {
                                        page_index: page_idx,
                                        term: span.text.clone(),
                                        rect: span.rect,
                                        checked: true,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                self.error_msg = Some(format!("Invalid regex pattern: {}", e));
            }
        }
    }

    fn perform_simple_search(
        &mut self,
        page_spans: &BTreeMap<usize, Vec<TextSpan>>,
        search_term: &str,
    ) {
        for (&page_idx, spans) in page_spans {
            for span in spans {
                let text_to_check =
                    if self.case_sensitive { span.text.clone() } else { span.text.to_lowercase() };

                if text_to_check.contains(search_term) {
                    self.matches.push(SearchMatch {
                        page_index: page_idx,
                        term: span.text.clone(),
                        rect: span.rect,
                        checked: true,
                    });
                }
            }
        }
    }

    fn perform_search(
        &mut self,
        raw_texts: &BTreeMap<usize, String>,
        page_spans: &BTreeMap<usize, Vec<TextSpan>>,
    ) {
        self.matches.clear();
        self.error_msg = None;

        if self.search_query.trim().is_empty() {
            return;
        }

        if self.use_regex {
            let pattern = if self.case_sensitive {
                self.search_query.clone()
            } else {
                format!("(?i){}", self.search_query)
            };
            self.perform_regex_search(raw_texts, page_spans, &pattern);
        } else {
            let search_term = if self.case_sensitive {
                self.search_query.clone()
            } else {
                self.search_query.to_lowercase()
            };
            self.perform_simple_search(page_spans, &search_term);
        }
    }
}
