//! The document's comments, as a reviewer works through them (ROADMAP AA-2b).
//!
//! **The drawer names what was asked and the window turns it into an `Operation`**
//! (Rule D). What counts as a comment, what answers what, and the state each reviewer has
//! given are the engine's answers (`PdfDocument::comments`). This only filters, sorts and
//! draws them.

use fepdf::AnnotationAt;
use fepdf::AnnotationState;
use fepdf::comments::Comment;
use std::collections::{BTreeMap, BTreeSet};

/// What the reader asked of a comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Show the page it is on.
    GoTo(usize),
    /// Answer it.
    Reply(AnnotationAt, String),
    /// Give it a state, as the author in the settings.
    State(AnnotationAt, AnnotationState),
    /// Replace its words.
    Edit(AnnotationAt, String),
    /// Remove it, with what answers it.
    Remove(AnnotationAt),
}

/// The order the list is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Order {
    /// By page, then by place on it.
    #[default]
    Page,
    /// By author, then by page.
    Author,
    /// Newest first, by `/M` or else `/CreationDate`.
    Date,
}

impl Order {
    const ALL: [Self; 3] = [Self::Page, Self::Author, Self::Date];

    const fn key(self) -> &'static str {
        match self {
            Self::Page => "comments_sort_page",
            Self::Author => "comments_sort_author",
            Self::Date => "comments_sort_date",
        }
    }
}

/// The states a reviewer can give, in the order the menu lists them (Table 174).
const STATES: [AnnotationState; 7] = [
    AnnotationState::Accepted,
    AnnotationState::Rejected,
    AnnotationState::Cancelled,
    AnnotationState::Completed,
    AnnotationState::None,
    AnnotationState::Marked,
    AnnotationState::Unmarked,
];

/// The locale key that names `state`.
pub const fn state_key(state: AnnotationState) -> &'static str {
    match state {
        AnnotationState::Accepted => "comments_state_accepted",
        AnnotationState::Rejected => "comments_state_rejected",
        AnnotationState::Cancelled => "comments_state_cancelled",
        AnnotationState::Completed => "comments_state_completed",
        AnnotationState::None => "comments_state_none",
        AnnotationState::Marked => "comments_state_marked",
        AnnotationState::Unmarked => "comments_state_unmarked",
    }
}

/// What the drawer holds between frames.
#[derive(Default)]
pub struct CommentsPanel {
    /// Only this kind, when one is chosen.
    kind: Option<String>,
    /// Only this author, when one is chosen. `Some("")` is comments with no author.
    author: Option<String>,
    /// Only comments a reviewer has given this state.
    state: Option<AnnotationState>,
    order: Order,
    /// What is being typed in reply, by the comment it answers.
    replies: BTreeMap<AnnotationAt, String>,
    /// The comment whose words are being edited, and the words so far.
    editing: Option<(AnnotationAt, String)>,
}

impl CommentsPanel {
    /// Draws the drawer, and answers what the reader asked.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        comments: &[Comment],
        words: &dyn Fn(&str) -> String,
    ) -> Option<Asked> {
        let roots: Vec<&Comment> = comments.iter().filter(|c| c.markup && !c.is_reply()).collect();
        if roots.is_empty() {
            ui.label(words("comments_none"));
            return None;
        }
        self.filters(ui, &roots, words);
        ui.separator();
        let mut shown: Vec<&Comment> = roots.into_iter().filter(|c| self.keeps(c)).collect();
        self.sort(&mut shown);
        ui.label(words("comments_count").replace("{}", &shown.len().to_string()));
        let mut asked = None;
        for comment in shown {
            ui.add_space(crate::app::theme::space::ITEM);
            if let Some(a) = self.entry(ui, comment, comments, words) {
                asked = Some(a);
            }
        }
        asked
    }

    /// The three filters and the order.
    fn filters(&mut self, ui: &mut egui::Ui, roots: &[&Comment], words: &dyn Fn(&str) -> String) {
        let kinds: BTreeSet<String> = roots.iter().map(|c| c.subtype.clone()).collect();
        let authors: BTreeSet<String> =
            roots.iter().map(|c| c.author.clone().unwrap_or_default()).collect();
        let all = words("comments_all");
        let nobody = words("comments_no_author");
        ui.horizontal_wrapped(|ui| {
            choose(ui, "comments_kind", &mut self.kind, &kinds, &all, &|k| k.to_owned());
            choose(ui, "comments_author", &mut self.author, &authors, &all, &|a| {
                if a.is_empty() { nobody.clone() } else { a.to_owned() }
            });
            let shown = self.state.map_or_else(|| all.clone(), |s| words(state_key(s)));
            egui::ComboBox::from_id_salt("comments_state").selected_text(shown).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.state, None, &all);
                for state in STATES {
                    ui.selectable_value(&mut self.state, Some(state), words(state_key(state)));
                }
            });
            egui::ComboBox::from_id_salt("comments_order")
                .selected_text(words(self.order.key()))
                .show_ui(ui, |ui| {
                    for order in Order::ALL {
                        ui.selectable_value(&mut self.order, order, words(order.key()));
                    }
                });
        });
    }

    fn keeps(&self, comment: &Comment) -> bool {
        self.kind.as_ref().is_none_or(|k| &comment.subtype == k)
            && self.author.as_ref().is_none_or(|a| comment.author.clone().unwrap_or_default() == *a)
            && self.state.is_none_or(|s| comment.states.iter().any(|m| m.state == s))
    }

    fn sort(&self, shown: &mut [&Comment]) {
        let place = |c: &Comment| (c.at.page, c.at.index);
        let when =
            |c: &Comment| c.modified.clone().or_else(|| c.created.clone()).unwrap_or_default();
        match self.order {
            Order::Page => shown.sort_by_key(|c| place(c)),
            Order::Author => {
                shown.sort_by_key(|c| (c.author.clone().unwrap_or_default(), place(c)));
            }
            Order::Date => shown.sort_by_key(|c| std::cmp::Reverse((when(c), place(c)))),
        }
    }

    /// One comment, its replies, and what can be done to it.
    fn entry(
        &mut self,
        ui: &mut egui::Ui,
        comment: &Comment,
        all: &[Comment],
        words: &dyn Fn(&str) -> String,
    ) -> Option<Asked> {
        let mut asked = None;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            if heading(ui, comment, words) {
                asked = Some(Asked::GoTo(comment.at.page));
            }
            if let Some(a) = self.words_of(ui, comment, words) {
                asked = Some(a);
            }
            for mark in &comment.states {
                let who = if mark.author.is_empty() {
                    words("comments_no_author")
                } else {
                    mark.author.clone()
                };
                ui.label(
                    egui::RichText::new(format!("{who}: {}", words(state_key(mark.state)))).weak(),
                );
            }
            for reply in replies_to(comment, all) {
                ui.indent(("reply", reply.at.page, reply.at.index), |ui| {
                    let who = reply.author.clone().unwrap_or_else(|| words("comments_no_author"));
                    ui.label(egui::RichText::new(who).strong());
                    ui.label(reply.contents.clone().unwrap_or_default());
                });
            }
            if let Some(a) = self.actions(ui, comment, words) {
                asked = Some(a);
            }
        });
        asked
    }

    /// The comment's words, or the field they are being edited in.
    fn words_of(
        &mut self,
        ui: &mut egui::Ui,
        comment: &Comment,
        words: &dyn Fn(&str) -> String,
    ) -> Option<Asked> {
        let Some((at, draft)) = self.editing.as_mut().filter(|(at, _)| *at == comment.at) else {
            ui.label(comment.contents.clone().unwrap_or_default());
            return None;
        };
        ui.add(egui::TextEdit::multiline(draft).desired_rows(2).desired_width(f32::INFINITY));
        let (at, draft) = (*at, draft.clone());
        let mut asked = None;
        ui.horizontal(|ui| {
            if ui.button(words("comments_save")).clicked() {
                asked = Some(Asked::Edit(at, draft));
            }
            if ui.button(words("comments_cancel")).clicked() || asked.is_some() {
                self.editing = None;
            }
        });
        asked
    }

    /// Reply, state, edit and remove.
    fn actions(
        &mut self,
        ui: &mut egui::Ui,
        comment: &Comment,
        words: &dyn Fn(&str) -> String,
    ) -> Option<Asked> {
        let at = comment.at;
        let mut asked = None;
        let draft = self.replies.entry(at).or_default();
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(draft).hint_text(words("comments_reply_hint")));
            if ui.button(words("comments_reply")).clicked() && !draft.trim().is_empty() {
                asked = Some(Asked::Reply(at, std::mem::take(draft)));
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.menu_button(words("comments_set_state"), |ui| {
                for state in STATES {
                    if ui.button(words(state_key(state))).clicked() {
                        asked = Some(Asked::State(at, state));
                        ui.close();
                    }
                }
            });
            // A free text annotation draws its words, and the engine refuses to change
            // them; the button is not offered rather than offered and refused.
            if comment.subtype != "FreeText" && ui.button(words("comments_edit")).clicked() {
                self.editing = Some((at, comment.contents.clone().unwrap_or_default()));
            }
            if ui.button(words("comments_remove")).clicked() {
                asked = Some(Asked::Remove(at));
            }
        });
        asked
    }
}

/// The comment's first line: page, kind, author, date. Says whether it was clicked.
fn heading(ui: &mut egui::Ui, comment: &Comment, words: &dyn Fn(&str) -> String) -> bool {
    let page = words("comments_page").replace("{}", &(comment.at.page + 1).to_string());
    let who = comment.author.clone().unwrap_or_else(|| words("comments_no_author"));
    let when = comment.modified.as_deref().or(comment.created.as_deref()).map(shown_date);
    let line = match when {
        Some(when) => format!("{page} · {} · {who} · {when}", comment.subtype),
        None => format!("{page} · {} · {who}", comment.subtype),
    };
    ui.add(egui::Label::new(egui::RichText::new(line).strong()).sense(egui::Sense::click()))
        .clicked()
}

/// A date string (7.9.4) as `YYYY-MM-DD HH:MM`, or as written when it is not one.
fn shown_date(date: &str) -> String {
    let digits: String = date.trim_start_matches("D:").chars().take(12).collect();
    let part = |from: usize, to: usize| digits.get(from..to).unwrap_or("");
    if digits.len() < 8 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return date.to_owned();
    }
    let time = if digits.len() >= 12 {
        format!(" {}:{}", part(8, 10), part(10, 12))
    } else {
        String::new()
    };
    format!("{}-{}-{}{time}", part(0, 4), part(4, 6), part(6, 8))
}

/// Everything that answers `root` on its page, at any depth, in `/Annots` order; state
/// changes are left out, since they are shown as states.
fn replies_to<'a>(root: &Comment, all: &'a [Comment]) -> Vec<&'a Comment> {
    let mut reached = BTreeSet::from([root.at.index]);
    // `/Annots` order puts a reply after what it answers in what this engine writes; a
    // file that does otherwise is walked again until nothing more is reached (Rule 6).
    for _ in 0..all.len() {
        let before = reached.len();
        for c in all {
            if c.is_reply() && c.reply_to.is_some_and(|to| reached.contains(&to)) {
                reached.insert(c.at.index);
            }
        }
        if reached.len() == before {
            break;
        }
    }
    all.iter()
        .filter(|c| {
            c.at.index != root.at.index && reached.contains(&c.at.index) && c.sets.is_none()
        })
        .collect()
}

/// A combo box choosing one of `options`, or all of them.
fn choose(
    ui: &mut egui::Ui,
    id: &str,
    chosen: &mut Option<String>,
    options: &BTreeSet<String>,
    all: &str,
    name: &dyn Fn(&str) -> String,
) {
    let shown = chosen.as_deref().map_or_else(|| all.to_owned(), name);
    egui::ComboBox::from_id_salt(id).selected_text(shown).show_ui(ui, |ui| {
        ui.selectable_value(chosen, None, all);
        for option in options {
            ui.selectable_value(chosen, Some(option.clone()), name(option));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{CommentsPanel, replies_to, shown_date};
    use fepdf::comments::{Comment, StateMark};
    use fepdf::{AnnotationAt, AnnotationState};

    fn comment(index: usize, reply_to: Option<usize>, sets: Option<AnnotationState>) -> Comment {
        Comment {
            at: AnnotationAt { page: 0, index },
            subtype: "Text".into(),
            markup: true,
            name: None,
            author: None,
            contents: Some(format!("c{index}")),
            subject: None,
            modified: None,
            created: None,
            rect: [0.0; 4],
            reply_to,
            grouped: false,
            sets,
            states: Vec::new(),
        }
    }

    /// **A reply to a reply is still under the comment it began at**, and a state change
    /// is not listed as a reply: it is shown as the state it sets.
    #[test]
    fn replies_are_gathered_at_any_depth_and_states_left_out() {
        let all = vec![
            comment(0, None, None),
            comment(1, Some(0), None),
            comment(2, Some(1), None),
            comment(3, Some(0), Some(AnnotationState::Accepted)),
            comment(4, None, None),
        ];
        let under: Vec<usize> = replies_to(&all[0], &all).iter().map(|c| c.at.index).collect();
        assert_eq!(under, vec![1, 2]);
        assert!(replies_to(&all[4], &all).is_empty());

        // A file can list a reply before what it answers; one pass would miss it.
        let out_of_order =
            vec![comment(0, None, None), comment(1, Some(2), None), comment(2, Some(0), None)];
        let under: Vec<usize> =
            replies_to(&out_of_order[0], &out_of_order).iter().map(|c| c.at.index).collect();
        assert_eq!(under, vec![1, 2]);
    }

    /// Filtering by state keeps a comment any reviewer gave that state.
    #[test]
    fn a_state_filter_keeps_what_some_reviewer_gave_it() {
        let mut given = comment(0, None, None);
        given.states = vec![StateMark { author: "Bo".into(), state: AnnotationState::Rejected }];
        let bare = comment(1, None, None);
        let panel =
            CommentsPanel { state: Some(AnnotationState::Rejected), ..CommentsPanel::default() };
        assert!(panel.keeps(&given));
        assert!(!panel.keeps(&bare));
    }

    #[test]
    fn a_pdf_date_is_shown_as_a_date() {
        assert_eq!(shown_date("D:20261010120000+09'00"), "2026-10-10 12:00");
        assert_eq!(shown_date("D:20261010"), "2026-10-10");
        assert_eq!(shown_date("last Tuesday"), "last Tuesday", "the clause permits any text");
    }
}
