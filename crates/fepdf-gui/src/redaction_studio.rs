use crate::finding::{Found, Query};
use crate::redaction::{RedactionManager, RedactionZone};
use crate::worker::WorkerRequest;
use std::sync::mpsc::Sender;

/// One place the search found, and whether the reader wants it redacted.
pub struct SearchMatch {
    /// Where it is.
    pub found: Found,
    /// Whether it goes into the redaction when the reader asks.
    pub checked: bool,
}

pub struct RedactionStudioPanel {
    pub search_query: String,
    pub error_msg: Option<String>,
    pub matches: Vec<SearchMatch>,
    pub case_sensitive: bool,
    pub use_regex: bool,
    /// The number of the last search asked for. An answer to an earlier one is dropped:
    /// the worker answers in order, and a reader typing a word asks once a letter.
    pub search: u64,
}

impl Default for RedactionStudioPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl RedactionStudioPanel {
    pub const fn new() -> Self {
        Self {
            search_query: String::new(),
            error_msg: None,
            matches: Vec::new(),
            case_sensitive: false,
            use_regex: false,
            search: 0,
        }
    }

    pub fn show(
        // RR-15 Limit: GUI - Sequential egui declarations for Redaction Studio window layout
        &mut self,
        ui: &mut egui::Ui,
        tx: &Sender<WorkerRequest>,
        redaction_manager: &mut RedactionManager,
        locale_mgr: &crate::locale::LocaleManager,
        lang: &str,
    ) {
        let tr = |key: &str| locale_mgr.tr(lang, key);
        ui.vertical(|ui| {
            ui.add_space(crate::app::theme::space::ITEM);

            ui.horizontal(|ui| {
                ui.label(tr("redaction_studio_pattern"));
                if ui.text_edit_singleline(&mut self.search_query).changed() {
                    self.ask(tx, locale_mgr, lang);
                }
            });

            ui.horizontal(|ui| {
                if ui.checkbox(&mut self.use_regex, tr("redaction_studio_regex")).changed() {
                    self.ask(tx, locale_mgr, lang);
                }
                if ui
                    .checkbox(&mut self.case_sensitive, tr("redaction_studio_match_case"))
                    .changed()
                {
                    self.ask(tx, locale_mgr, lang);
                }
            });

            if let Some(err) = &self.error_msg {
                ui.colored_label(crate::app::theme::colors::note::FAIL, err);
            }

            ui.separator();

            if !self.matches.is_empty() {
                ui.horizontal(|ui| {
                    if ui.button(tr("redaction_studio_select_all")).clicked() {
                        for m in &mut self.matches {
                            m.checked = true;
                        }
                    }
                    if ui.button(tr("redaction_studio_clear_selection")).clicked() {
                        for m in &mut self.matches {
                            m.checked = false;
                        }
                    }
                    if ui.button(format!("🔏 {}", tr("redaction_studio_redact_selected"))).clicked()
                    {
                        for m in self.matches.iter().filter(|m| m.checked) {
                            for rect in &m.found.rects {
                                redaction_manager
                                    .zones
                                    .push(RedactionZone { page_index: m.found.page, rect: *rect });
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
                            ui.label(format!(
                                "{} {}: {}",
                                tr("redaction_studio_page_label"),
                                m.found.page + 1,
                                m.found.term
                            ));
                        });
                    }
                    for (idx, state) in to_toggle {
                        if let Some(found) = self.matches.get_mut(idx) {
                            found.checked = state;
                        }
                    }
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label(tr("redaction_studio_no_results"));
                });
            }
        });
    }

    /// Asks the worker for what the query finds, or says why it cannot be asked.
    fn ask(
        &mut self,
        tx: &Sender<WorkerRequest>,
        locale_mgr: &crate::locale::LocaleManager,
        lang: &str,
    ) {
        self.matches.clear();
        self.error_msg = None;
        if self.search_query.trim().is_empty() {
            return;
        }
        let query = Query {
            text: self.search_query.clone(),
            regex: self.use_regex,
            case_sensitive: self.case_sensitive,
        };
        if let Err(e) = query.pattern() {
            let label = locale_mgr.tr(lang, "redaction_studio_invalid_regex");
            self.error_msg = Some(format!("{label} {e}"));
            return;
        }
        self.search += 1;
        let _ = tx.send(WorkerRequest::Find { query, search: self.search });
    }

    /// What search `search` found. Checked, because a reader who searched for a name to
    /// redact wants every one of them unless they say otherwise.
    pub fn found(&mut self, search: u64, found: Vec<Found>) {
        if search != self.search {
            return;
        }
        self.matches =
            found.into_iter().map(|found| SearchMatch { found, checked: true }).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::LocaleManager;
    use std::sync::mpsc::{Receiver, channel};

    fn ask(panel: &mut RedactionStudioPanel) -> Receiver<WorkerRequest> {
        let (tx, rx) = channel();
        panel.ask(&tx, &LocaleManager::new(), "en");
        rx
    }

    fn one_found(term: &str) -> Vec<Found> {
        vec![Found { page: 3, term: term.to_string(), rects: vec![egui::Rect::NOTHING] }]
    }

    #[test]
    fn a_query_goes_to_the_worker_as_typed() {
        let mut panel = RedactionStudioPanel::new();
        panel.search_query = "acme".to_string();
        panel.case_sensitive = true;
        let rx = ask(&mut panel);
        let Ok(WorkerRequest::Find { query, search }) = rx.try_recv() else {
            panic!("nothing was asked");
        };
        assert_eq!(query.text, "acme");
        assert!(query.case_sensitive && !query.regex, "the options were not carried");
        assert_eq!(search, panel.search);
    }

    #[test]
    fn invalid_regex_reports_an_error_instead_of_asking() {
        let mut panel = RedactionStudioPanel::new();
        panel.use_regex = true;
        panel.search_query = "[unclosed".to_string();
        let rx = ask(&mut panel);
        assert!(rx.try_recv().is_err(), "a pattern that cannot match was sent");
        assert!(panel.error_msg.is_some());
    }

    #[test]
    fn an_empty_query_clears_previous_results_and_asks_nothing() {
        let mut panel = RedactionStudioPanel::new();
        panel.search_query = "acme".to_string();
        ask(&mut panel);
        panel.found(panel.search, one_found("ACME"));
        assert_eq!(panel.matches.len(), 1);

        panel.search_query = "   ".to_string();
        let rx = ask(&mut panel);
        assert!(panel.matches.is_empty());
        assert!(rx.try_recv().is_err(), "an empty query was sent");
    }

    /// **An answer to a search the reader has typed past is dropped.** The worker answers
    /// in order, so the answer to `ac` can arrive after `acme` was asked, and showing it
    /// would list matches for a query nobody is looking at.
    #[test]
    fn an_answer_to_an_earlier_search_is_dropped() {
        let mut panel = RedactionStudioPanel::new();
        panel.search_query = "ac".to_string();
        ask(&mut panel);
        let earlier = panel.search;
        panel.search_query = "acme".to_string();
        ask(&mut panel);

        panel.found(earlier, one_found("ac"));
        assert!(panel.matches.is_empty(), "the answer to `ac` was shown for `acme`");
        panel.found(panel.search, one_found("ACME"));
        assert_eq!(panel.matches.len(), 1, "the answer to `acme` was not shown");
        assert!(panel.matches[0].checked, "a match arrives unchecked");
    }
}
