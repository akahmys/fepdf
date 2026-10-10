//! Offering back what a window that did not exit normally left behind
//! ([ADR-0114](../../../../docs/adr/0114-the-window-journals-its-acts-and-a-crash-replays-them.md)).
//!
//! **Asked once, at start, and never assumed.** Recovering replaces whatever this window
//! has open, and a session can belong to a document the reader has since exported. So
//! the reader chooses: recover it, discard it, or decide later, which leaves it on disk
//! to be offered again at the next start.

use super::theme::{size, space};
use super::{FepdfApp, Notice};
use crate::recovery::Found;
use crate::worker::WorkerRequest;

/// The sessions waiting to be offered, and where the one on screen stands.
#[derive(Default)]
pub struct Offers {
    /// Still to be offered, oldest first, so the newest is the one popped next.
    waiting: Vec<Found>,
    /// The one being offered now.
    current: Option<Offer>,
}

struct Offer {
    found: Found,
    /// What the reader has typed, for a sealed journal.
    attempt: String,
    stage: Stage,
}

/// Where an offer stands. **One state, not three flags** (Rule 8).
enum Stage {
    /// The question is on screen.
    Asking,
    /// The journal is sealed, and the document's password is wanted.
    Password { refused: bool },
    /// Sent to the worker; hidden until it answers.
    Sent,
}

enum Choice {
    Recover,
    Discard,
    Later,
}

impl Offers {
    /// The sessions under `root` that a window left behind, newest offered first.
    pub fn at(root: Option<&std::path::Path>) -> Self {
        let mut waiting = root.map(crate::recovery::find).unwrap_or_default();
        waiting.reverse();
        let current = waiting.pop().map(Offer::new);
        Self { waiting, current }
    }

    /// The worker answered a recovery that was sent: whatever it said, this offer is
    /// done, and the next one comes up.
    pub fn answered(&mut self) {
        if matches!(self.current.as_ref().map(|o| &o.stage), Some(Stage::Sent)) {
            self.next();
        }
    }

    /// The journal is sealed and the password did not open it.
    pub fn locked(&mut self, retried: bool) {
        if let Some(offer) = self.current.as_mut() {
            offer.stage = Stage::Password { refused: retried };
            offer.attempt.clear();
        }
    }

    fn next(&mut self) {
        self.current = self.waiting.pop().map(Offer::new);
    }
}

impl Offer {
    const fn new(found: Found) -> Self {
        Self { found, attempt: String::new(), stage: Stage::Asking }
    }
}

/// The words the dialog says, read from the locale before the offer is borrowed.
struct Words {
    title: String,
    untitled: String,
    acts: String,
    recover: String,
    discard: String,
    later: String,
    sealed: String,
    refused: String,
    hint: String,
}

impl FepdfApp {
    /// Draws the offer on screen, if there is one, and carries out what the reader chose.
    pub(crate) fn show_recovery_offer(&mut self, ctx: &egui::Context) {
        let words = Words {
            title: self.tr("recovery_title"),
            untitled: self.tr("password_untitled"),
            acts: self.tr("recovery_acts"),
            recover: self.tr("recovery_recover"),
            discard: self.tr("recovery_discard"),
            later: self.tr("recovery_later"),
            sealed: self.tr("recovery_sealed"),
            refused: self.tr("password_refused"),
            hint: self.tr("password_hint"),
        };
        let Some(offer) = self.offers.current.as_mut() else { return };
        if matches!(offer.stage, Stage::Sent) {
            return;
        }
        if let Some(choice) = offer_dialog(ctx, offer, &words) {
            self.act_on_offer(choice, ctx);
        }
    }

    fn act_on_offer(&mut self, choice: Choice, ctx: &egui::Context) {
        let busy = self.tr("busy_recovering");
        let Some(offer) = self.offers.current.as_mut() else { return };
        match choice {
            Choice::Later => self.offers.next(),
            Choice::Discard => {
                if let Err(why) = crate::recovery::discard(&offer.found.dir) {
                    self.notice =
                        Some(Notice::failed("notice_recovery_failed").about(why.to_string()));
                }
                self.offers.next();
            }
            Choice::Recover if offer.found.sealed && matches!(offer.stage, Stage::Asking) => {
                offer.stage = Stage::Password { refused: false };
            }
            Choice::Recover => {
                let password = offer.found.sealed.then(|| std::mem::take(&mut offer.attempt));
                offer.stage = Stage::Sent;
                let dir = offer.found.dir.clone();
                self.forget_the_document();
                self.loading_message = busy;
                let _ = self.tx_worker.send(WorkerRequest::Recover { dir, password });
                ctx.request_repaint();
            }
        }
    }

    /// Ends this window's session before the window goes, so that what it journaled is
    /// not offered as a crash at the next start.
    ///
    /// **Waited for, and not forever.** The worker may be in the middle of something long;
    /// a session it cannot end in time stays on disk and is offered at the next start,
    /// which is the safe way to be wrong.
    pub(crate) fn end_session(&self) {
        let (done, ended) = std::sync::mpsc::channel();
        if self.tx_worker.send(WorkerRequest::Shutdown { done }).is_err() {
            // The worker is gone, and its session with it or on disk; either way there is
            // nothing to wait for.
            return;
        }
        if ended.recv_timeout(std::time::Duration::from_secs(5)).is_err() {
            log::warn!(
                "the worker did not end its session in time; it is offered at the next start"
            );
        }
    }
}

/// Draws the offer, and says what the reader chose, if they chose.
fn offer_dialog(ctx: &egui::Context, offer: &mut Offer, words: &Words) -> Option<Choice> {
    let mut choice = None;
    let name = offer.found.name.clone().unwrap_or_else(|| words.untitled.clone());
    egui::Window::new("recovery")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(size::FORM_W)
        .show(ctx, |ui| {
            ui.add_space(space::GROUP);
            ui.heading(&words.title);
            ui.add_space(space::ITEM);
            ui.label(egui::RichText::new(&name).strong());
            ui.label(words.acts.replace("{}", &offer.found.entries.to_string()));
            ui.add_space(space::GROUP);
            if let Stage::Password { refused } = offer.stage {
                password_field(ui, offer, refused, words, &mut choice);
            }
            ui.horizontal(|ui| {
                if ui.button(&words.recover).clicked() {
                    choice = Some(Choice::Recover);
                }
                if ui.button(&words.later).clicked() {
                    choice = Some(Choice::Later);
                }
                if ui.button(&words.discard).clicked() {
                    choice = Some(Choice::Discard);
                }
            });
            ui.add_space(space::ITEM);
        });
    choice
}

fn password_field(
    ui: &mut egui::Ui,
    offer: &mut Offer,
    refused: bool,
    words: &Words,
    choice: &mut Option<Choice>,
) {
    let said = if refused { &words.refused } else { &words.sealed };
    ui.label(egui::RichText::new(said).color(super::theme::colors::note::WARN));
    ui.add_space(space::ITEM);
    let field = ui.add(
        egui::TextEdit::singleline(&mut offer.attempt)
            .password(true)
            .hint_text(&words.hint)
            .desired_width(f32::INFINITY),
    );
    field.request_focus();
    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        *choice = Some(Choice::Recover);
    }
    ui.add_space(space::SECTION);
}
