//! The runs of a page, and the four things a reader does to one.
//!
//! **A run is what an edit names, and nothing groups runs.** Which of them belong together
//! is a question about meaning that a content stream does not answer: characters drawn
//! next to each other may be a word, or a label and its value, or two columns set in one
//! stream ([ADR-0091](../../../../docs/adr/0091-paragraphs-are-not-inferred-and-overflow-is-shown.md)).
//! So this drawer lists what the page draws and the reader points at one of them; it does
//! not offer to find a word, because a word is usually several runs and joining them would
//! be this window deciding what the document means.
//!
//! The drawer says so in as many words. A reader who expected find-and-replace should
//! learn why they are not getting it from the thing itself rather than from a manual.

use crate::interaction::RunBox;

/// What the reader has asked for, for the caller to turn into an `Operation`.
///
/// **The drawer builds no operation itself.** It says what was asked and the caller —
/// which holds the page number and the channel — sends it, so the drawer stays a drawer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Replace the named run's text.
    Replace(String),
    /// Cut it after this many of its codes.
    Cut(usize),
    /// Take it off the page.
    Delete,
    /// Join it with the run after it.
    Join,
}

/// What the drawer is holding between frames.
#[derive(Default)]
pub struct TextRunsPanel {
    /// The replacement being typed, and the run it belongs to.
    ///
    /// **Kept with its run.** A draft that outlived the selection would be typed into one
    /// run and applied to another.
    draft: Option<(usize, String)>,
    /// Where the reader has asked to cut, in codes.
    cut_after: usize,
}

impl TextRunsPanel {
    /// Draws the drawer, and answers what the reader asked for.
    pub fn show(
        // RR-15 Limit: GUI - Sequential egui declarations for the text runs drawer
        &mut self,
        ui: &mut egui::Ui,
        runs: &[RunBox],
        named: Option<usize>,
        words: &dyn Fn(&str) -> String,
    ) -> Option<Asked> {
        ui.label(words("runs_not_grouped"));
        ui.add_space(crate::app::theme::space::ITEM);

        let Some(named) = named else {
            ui.label(words("runs_none_named"));
            return None;
        };
        let Some(run) = runs.iter().find(|run| run.index == named) else {
            ui.label(words("runs_none_named"));
            return None;
        };

        ui.horizontal(|ui| {
            ui.label(words("runs_font"));
            ui.monospace(&run.font);
            ui.label(format!("{} {}", run.pieces.len(), words("runs_codes")));
        });
        ui.add_space(crate::app::theme::space::ITEM);

        let draft = match &mut self.draft {
            Some((for_run, text)) if *for_run == named => text,
            slot => {
                *slot = Some((named, run.text.clone()));
                self.cut_after = 1;
                let Some((_, text)) = slot else { return None };
                text
            }
        };

        ui.label(words("runs_reads"));
        let typed = ui.add(egui::TextEdit::multiline(draft).desired_rows(2));
        let replace = ui.button(words("runs_replace")).clicked()
            || (typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        let asked = replace.then(|| Asked::Replace(draft.clone()));
        ui.add_space(crate::app::theme::space::ITEM);

        let mut asked = asked;
        ui.horizontal(|ui| {
            ui.label(words("runs_split_after"));
            // A cut names a code, not a character: reading a run and writing it back is
            // not always the identity, so a cut made on the reading would lose what the
            // reading lost. The bounds are the cut's own — a cut at nought or at the end
            // divides a run into itself and nothing.
            let last = run.pieces.len().saturating_sub(1).max(1);
            ui.add(egui::DragValue::new(&mut self.cut_after).range(1..=last));
            if ui.button(words("runs_split")).clicked() {
                asked = Some(Asked::Cut(self.cut_after));
            }
        });
        ui.add_space(crate::app::theme::space::ITEM);

        ui.horizontal(|ui| {
            if ui.button(words("runs_delete")).clicked() {
                asked = Some(Asked::Delete);
            }
            // The last run has nothing after it, so the door is shut rather than opening
            // onto a refusal.
            let has_next = runs.iter().any(|other| other.index == named + 1);
            if ui.add_enabled(has_next, egui::Button::new(words("runs_merge"))).clicked() {
                asked = Some(Asked::Join);
            }
        });
        asked
    }

    /// Forgets the draft, for when the page has changed under it.
    pub fn forget(&mut self) {
        self.draft = None;
    }
}
