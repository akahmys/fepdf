//! Driving the window from a script, so that a person can look at what it draws.
//!
//! **Every "not visually verified" line in this repository comes back to one thing**: the
//! drawers, the dialogs and the overlays need a click to reach, and synthetic input does
//! not arrive at this window. `capture_ui.sh` was the previous answer and was deleted on
//! 2026-08-29 — it screenshotted the whole desktop and needed the application running in
//! front of someone, so nobody ever ran it.
//!
//! This takes the other side of the problem. The window is already the thing that knows
//! how to open its own drawers, so it opens them, and `ViewportCommand::Screenshot` hands
//! back what it drew. No pointer, no desktop, no person waiting.
//!
//! ```bash
//! ./target/debug/fepdf-gui --capture scripts/dev/tour.txt --shots /tmp/tour
//! ```
//!
//! A step per line, `#` for a comment. The window quits when the plan runs out, so this
//! is something a script can wait on.

use crate::sidebar::ActiveDrawer;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// One thing to do to the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Open a document, and wait for it.
    Open(PathBuf),
    /// Open one of the drawers, or close whatever is open.
    Drawer(ActiveDrawer),
    /// Select a page by its 1-based number, as a reader would count it.
    Select(usize),
    /// Delete whatever is selected.
    Delete,
    /// Turn the selection a quarter clockwise.
    Rotate,
    /// Take the last operation back, and put it back.
    Undo,
    /// See [`Self::Undo`].
    Redo,
    /// Show the command palette.
    Palette,
    /// Show the export wizard.
    Export,
    /// Show the settings window.
    Settings,
    /// Show the about window.
    About,
    /// Change the language, so a screen can be seen in both.
    Language(String),
    /// Close every window, so the next shot is of one thing.
    Clear,
    /// Write what the window is drawing to `<shots>/<name>.ppm`.
    Shot(String),
}

/// Why a plan could not be read.
///
/// **A type rather than a `String`** (Rule 11): the two failures are different questions
/// — a file that is not there, and a line that names no step — and a caller that has to
/// parse prose to tell them apart is a caller that will not.
#[derive(Debug)]
pub enum PlanError {
    /// The plan itself could not be read.
    Unreadable(PathBuf, std::io::Error),
    /// A line named no step this window knows.
    Unknown {
        /// Its 1-based number in the file.
        line: usize,
        /// What it said.
        text: String,
    },
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(path, e) => write!(f, "{}: {e}", path.display()),
            Self::Unknown { line, text } => write!(f, "line {line}: {text}"),
        }
    }
}

impl std::error::Error for PlanError {}

/// A plan, and where its shots go.
pub struct Plan {
    steps: VecDeque<Step>,
    shots: PathBuf,
    /// When the last step was taken, which is what the two waits below are measured from.
    acted: std::time::Instant,
    /// The name a screenshot in flight will be saved under.
    pending: Option<String>,
}

/// How long to wait for the worker after an action.
///
/// **A cap rather than the wait itself.** The wait is on the window being idle — nothing
/// loading, nothing queued, no rebuild running — and this is how long it may take before
/// the plan gives up and shoots anyway, so that a page which never arrives produces a
/// screenshot of the window failing to draw it rather than a script that hangs.
///
/// **Measured in time, and it used to be measured in frames.** 240 of them sounds like
/// four seconds and is not: `drive_capture` asks for a repaint every frame, so an idle
/// window runs as fast as the compositor will let it and 240 frames go by in well under a
/// second. `samples/volvo_xc90.pdf` takes about two seconds to open, so the cap fired
/// first and the shot caught the progress message — a harness quietly photographing the
/// wrong thing, which is the one failure it must not have.
const SETTLE_CAP: std::time::Duration = std::time::Duration::from_secs(20);

/// How long to wait after every step, whether or not the window says it is idle.
///
/// **A window egui has just opened is still fading in.** The first run caught the
/// palette, the settings and the about windows at a fraction of their opacity, which
/// reads as a rendering defect and is not one. Idle is about the worker; this is about
/// the animation, and nothing reports when one has finished.
const SETTLE_FLOOR: std::time::Duration = std::time::Duration::from_millis(250);

impl Plan {
    /// Reads a plan, or says which line it could not read.
    ///
    /// # Errors
    /// Fails when the file cannot be read or a line names no step.
    pub fn read(path: &Path, shots: PathBuf) -> Result<Self, PlanError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| PlanError::Unreadable(path.to_path_buf(), e))?;
        let mut steps = VecDeque::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            steps.push_back(
                parse(line)
                    .ok_or_else(|| PlanError::Unknown { line: n + 1, text: line.to_owned() })?,
            );
        }
        Ok(Self { steps, shots, acted: std::time::Instant::now(), pending: None })
    }

    /// Whether anything is left to do.
    pub fn finished(&self) -> bool {
        self.steps.is_empty() && self.pending.is_none()
    }

    /// The next step, once the window has settled.
    fn next(&mut self, idle: bool) -> Option<Step> {
        let waited = self.acted.elapsed();
        if waited < SETTLE_FLOOR {
            return None;
        }
        if !idle && waited < SETTLE_CAP {
            return None;
        }
        self.acted = std::time::Instant::now();
        self.steps.pop_front()
    }
}

fn parse(line: &str) -> Option<Step> {
    let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
    let rest = rest.trim();
    Some(match verb {
        "open" => Step::Open(PathBuf::from(rest)),
        "drawer" => Step::Drawer(drawer(rest)?),
        "select" => Step::Select(rest.parse().ok()?),
        "delete" => Step::Delete,
        "rotate" => Step::Rotate,
        "undo" => Step::Undo,
        "redo" => Step::Redo,
        "palette" => Step::Palette,
        "export" => Step::Export,
        "settings" => Step::Settings,
        "about" => Step::About,
        "language" => Step::Language(rest.to_owned()),
        "clear" => Step::Clear,
        "shot" => Step::Shot(rest.to_owned()),
        _ => return None,
    })
}

fn drawer(name: &str) -> Option<ActiveDrawer> {
    Some(match name {
        "none" => ActiveDrawer::None,
        "info" => ActiveDrawer::DocumentInfo,
        "survey" => ActiveDrawer::WhatItDoes,
        "accessibility" => ActiveDrawer::Accessibility,
        "redaction" => ActiveDrawer::Redaction,
        "caliper" => ActiveDrawer::Caliper,
        "tools" => ActiveDrawer::Tools,
        _ => return None,
    })
}

/// Writes `image` as a binary PPM.
///
/// **PPM because it needs nothing.** A PNG needs a deflate stream and an encoder; this is
/// fifteen bytes of header and the pixels, every tool reads it, and the file is thrown
/// away by the script that converts it. The alpha channel goes: a screenshot of a window
/// is opaque, and what is being looked at is the colour.
fn write_ppm(path: &Path, image: &egui::ColorImage) -> std::io::Result<()> {
    use std::io::Write as _;
    let [w, h] = image.size;
    let mut out = Vec::with_capacity(w * h * 3 + 32);
    write!(out, "P6\n{w} {h}\n255\n")?;
    for pixel in &image.pixels {
        out.extend_from_slice(&[pixel.r(), pixel.g(), pixel.b()]);
    }
    std::fs::write(path, out)
}

impl crate::app::FepdfApp {
    /// Performs one step of the plan, if the window has settled enough to take it.
    ///
    /// Called last in the frame, so a `shot` captures what the frame drew.
    pub(crate) fn drive_capture(&mut self, ctx: &egui::Context) {
        // RR-15 Limit: GUI - one arm per step of a capture plan
        if self.capture.is_none() {
            return;
        }
        ctx.request_repaint();
        self.take_pending_shot(ctx);

        // **A document with no page drawn yet is not idle**, whatever the queue says: the
        // queue is filled during the frame that draws, so the first frame after an open
        // has an empty one. The first shot of the first run caught the placeholder saying
        // the page was still rendering, which is exactly the sort of thing this exists to
        // catch — just not about itself.
        let drawn = self.total_pages == 0 || !self.scenes.is_empty();
        let idle =
            self.request_queue.is_empty() && !self.is_loading && self.busy.is_none() && drawn;
        let Some(plan) = self.capture.as_mut() else { return };
        if plan.pending.is_some() {
            return;
        }
        let Some(step) = plan.next(idle) else {
            if plan.finished() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        };

        match step {
            Step::Open(path) => self.open_file(path, ctx),
            Step::Drawer(drawer) => self.active_drawer = drawer,
            Step::Select(page) => {
                self.selected_pages.clear();
                self.selected_pages.insert(page.saturating_sub(1));
                self.view.active_page = page.saturating_sub(1);
            }
            Step::Delete => self.remove_selected_pages(),
            Step::Rotate => self.rotate_selected_pages(fepdf::Quarter::Q90),
            Step::Undo => {
                self.begin_rebuild("history_undoing");
                let _ = self.tx_worker.send(crate::worker::WorkerRequest::Undo);
            }
            Step::Redo => {
                self.begin_rebuild("history_redoing");
                let _ = self.tx_worker.send(crate::worker::WorkerRequest::Redo);
            }
            Step::Palette => self.show_command_palette = true,
            Step::Export => self.show_export_wizard = true,
            Step::Settings => self.show_settings_modal = true,
            Step::About => self.show_about_modal = true,
            Step::Language(lang) => self.active_language = lang,
            Step::Clear => {
                self.show_command_palette = false;
                self.show_export_wizard = false;
                self.show_settings_modal = false;
                self.show_about_modal = false;
                self.active_drawer = ActiveDrawer::None;
            }
            Step::Shot(name) => {
                if let Some(plan) = self.capture.as_mut() {
                    plan.pending = Some(name);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
    }

    /// Saves the screenshot the last frame asked for, once it arrives.
    fn take_pending_shot(&mut self, ctx: &egui::Context) {
        // `if let` rather than a `match` with a wildcard: `egui::Event` is an external
        // crate's enum with two dozen variants and Rule 5 forbids the arm that would
        // swallow the ones this does not care about.
        let image = ctx.input(|i| {
            i.raw.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(std::sync::Arc::clone(image))
                } else {
                    None
                }
            })
        });
        let (Some(image), Some(plan)) = (image, self.capture.as_mut()) else { return };
        let Some(name) = plan.pending.take() else { return };
        let path = plan.shots.join(format!("{name}.ppm"));
        match write_ppm(&path, &image) {
            Ok(()) => log::info!("capture: {}", path.display()),
            Err(e) => log::error!("capture: {}: {e}", path.display()),
        }
    }
}
