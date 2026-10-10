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
    /// Sets the zoom, as a whole percentage. Below 30 the viewport shows tiles.
    Zoom(u32),
    /// Presses the status bar's zoom-in button, which is the path a reader takes.
    ZoomIn,
    /// Zooms the way a trackpad does: many small steps, anchored under a point.
    ///
    /// `wheel <x> <y> <steps>` in points from the viewport's top-left. A single `zoom`
    /// jumps; this is what a reader's fingers actually send, and the two are not the same
    /// question — a gesture that crosses the tile boundary keeps going afterwards.
    Wheel(u32, u32, i32),
    /// Holds the view controls open. A plan has no pointer to reach for them with.
    Pin,
    /// Selects the text of the page being shown, as a drag across it would.
    ///
    /// **Everything a drag does except the pointer.** A plan cannot press a mouse button,
    /// so this drives the same call a drag's `dragged()` frame makes — which is what
    /// shows whether a selection, once made, is drawn at all.
    SelectText,
    /// Selects a structure element by its id in the tree, which is what the element
    /// properties panel draws. Nothing else reaches that panel: the tree is a drawer, and
    /// a plan cannot click a row in it.
    Node(usize),
    /// Choose a bookmark by its position, roots first: `mark 3` is the fourth root,
    /// `mark 3 1` its second child.
    Mark(Vec<usize>),
    /// Retitle the chosen bookmark, as typing in the title field would.
    MarkTitle(String),
    /// Press the bookmark panel's write button, which sends one `UpdateOutlines`.
    ///
    /// **The last three lines of the panel are only reachable this way.** Choosing,
    /// retitling and moving are all tested without egui; what a plan alone can show is
    /// that the draft actually leaves the panel and reaches the document.
    MarkWrite,
    /// Insert a document at a page position: `insert <path> <at>`.
    ///
    /// The panel's own entry opens a file dialog, which a plan cannot answer, so this
    /// names the file instead and drives everything after the dialog.
    Insert(PathBuf, usize),
    /// Take the selected pages out: `extract` keeps them here, `extract remove` does not.
    Extract(bool),
    /// Mark every page, as a reader choosing all of them would.
    SelectAll,
    /// Open one of the document tools by name: `tool resize`.
    Tool(String),
    /// Set the resize form's sheet and fit, then press its apply:
    /// `resize <sheet|WxH> <fit|scale:FACTOR>`.
    Resize(String, String),
    /// Fill the header-and-footer form and press its apply:
    /// `decorate <top|bottom>-<left|centre|right> <text>`.
    Decorate(String, String),
    /// Put every page of the file at `path` where the selected pages are: `replace <path>`.
    Replace(PathBuf),
    /// Write the selected pages as images into a folder: `images <folder>`.
    Images(PathBuf),
    /// Choose the export wizard's protection, as its radio buttons would:
    /// `protect <none|password|certificates>`.
    Protect(String),
    /// Choose the page-view mode: `mode single|spread`. The grid is the zoom's, not a mode.
    Mode(String),
    /// Set the resize form's offset without applying: `nudge <x> <y>`.
    Nudge(i32, i32),
    /// Set the resize form's sheet and scale without applying: `setresize <sheet> <fit>`.
    SetResize(String, String),
    /// Double-click the bench at a point, which crosses the tile boundary:
    /// `dblclick <x> <y>` in points from the viewport's top-left.
    DoubleClick(u32, u32),
    /// Press the primary button at one point and let go at another, moving between them:
    /// `drag <x0> <y0> <x1> <y1>` in points from the viewport's top-left.
    ///
    /// **The one step that is the gesture itself.** Every other step drives the call a
    /// gesture would end in, and that is how the snapshot tool went a week answering
    /// nothing to a drag: the call it ends in worked, and the drag never reached it. These
    /// events go in where the platform's do, through `raw_input_hook`, one batch a frame.
    Drag([u32; 4]),
    /// Choose the annotation drawer's pen by its locale key's last part, and the words it
    /// writes: `pen <highlight|note|callout|…> [words]`. A plan cannot click a pen or type.
    Pen(String, String),
    /// Read aloud from the page on screen, writing the speech into a folder rather than
    /// out of the speakers: `readaloud <folder>`. Only `say` writes a file.
    ReadAloud(PathBuf),
    /// Compare the open document with another, as the drawer's file dialog would:
    /// `comparewith <path>`.
    CompareWith(PathBuf),
    /// Choose what a snapshot drag is for: `purpose copy|crop|remove`.
    Purpose(String),
    /// Split the selected page as the page menu would: `split <columns> <rows>`.
    Split(usize, usize),
    /// Combine the selected pages as the page menu would: `combine <columns> <rows>`.
    Combine(usize, usize),
    /// Press the print drawer's button, as it stands.
    Print,
    /// Choose what the caliper takes: `caliper distance|polygon`.
    Caliper(String),
    /// Set the page's scale as the caliper drawer's form would: `scale <N> <unit>`, 1:N.
    Scale(u32, String),
    /// Print where the current page sits against the viewport: `probe <label>`.
    Probe(String),
    /// Open a page as a double-click on its tile does: `openpage <1-based>`.
    ///
    /// **Everything the double-click does except the pointer.** A plan cannot press a
    /// mouse button twice, so this drives the two calls the tile's `double_clicked()`
    /// frame makes — which is what shows where the page actually lands.
    OpenPage(usize),
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
    /// Run one of the palette's commands by its locale key, as choosing it would:
    /// `command cmd_redaction_studio`. A plan had no way to choose one while the redaction
    /// brush's only door was the palette.
    Command(String),
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
    /// Click at a point of the window, in points from its top-left: `clickat <x> <y>`.
    /// The press and the release go in where the platform's do, as `drag`'s events do.
    ClickAt(u32, u32),
    /// Click the middle of the first widget whose name contains the words, ignoring case:
    /// `clicklabel Zoom in`. The names are the last frame's AccessKit tree (`control.rs`).
    ClickLabel(String),
    /// Press a key, with what is held: `key cmd+shift+K`, `key Escape`.
    Key(String),
    /// Type the words, as a keyboard would: `type invoice`.
    Type(String),
    /// Say which widgets the last frame drew whose names contain the words, and where:
    /// `inspect zoom`, or `inspect` for every one.
    Inspect(String),
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
    /// Pointer events a `drag` is still to send, one batch a frame.
    input: VecDeque<Vec<egui::Event>>,
    /// The files steps arrive in and answers go to, when the window is driven from
    /// outside (`--control`) rather than by a plan read once.
    control: Option<crate::control::Control>,
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
        Ok(Self {
            steps,
            shots,
            acted: std::time::Instant::now(),
            pending: None,
            input: VecDeque::new(),
            control: None,
        })
    }

    /// A plan whose steps arrive in `control`'s `in` while the window runs, and which
    /// never runs out: the window stays until it is closed.
    pub fn live(control: crate::control::Control) -> Self {
        Self {
            steps: VecDeque::new(),
            shots: control.dir().to_path_buf(),
            acted: std::time::Instant::now(),
            pending: None,
            input: VecDeque::new(),
            control: Some(control),
        }
    }

    /// Takes the steps that have arrived, and says which lines named none.
    fn take_arrivals(&mut self) {
        let Some(control) = self.control.as_mut() else { return };
        for line in control.arrived() {
            match parse(&line) {
                Some(step) => self.steps.push_back(step),
                None => control.report(&format!("unknown: {line}")),
            }
        }
    }

    /// An answer: into `out` when driven from outside, to standard output under a plan.
    fn say(&self, line: &str) {
        match &self.control {
            Some(control) => control.report(line),
            None => println!("{line}"),
        }
    }

    /// Whether anything is left to do.
    pub fn finished(&self) -> bool {
        self.control.is_none()
            && self.steps.is_empty()
            && self.pending.is_none()
            && self.input.is_empty()
    }

    /// The pointer events for this frame, if a drag is under way.
    pub fn input_for_this_frame(&mut self) -> Vec<egui::Event> {
        let events = self.input.pop_front().unwrap_or_default();
        if !events.is_empty() {
            // The settling is measured from the release, not from when the drag began.
            self.acted = std::time::Instant::now();
        }
        events
    }

    /// The next step, once the window has settled.
    fn next(&mut self, idle: bool) -> Option<Step> {
        let waited = self.acted.elapsed();
        if waited < SETTLE_FLOOR || !self.input.is_empty() {
            return None;
        }
        if !idle && waited < SETTLE_CAP {
            return None;
        }
        self.acted = std::time::Instant::now();
        let step = self.steps.pop_front();
        // Driven from outside, each step taken is said: the next one's line is how a caller
        // knows the last has been done.
        if let (Some(step), Some(control)) = (&step, &self.control) {
            control.report(&format!("step {step:?}"));
        }
        step
    }
}

fn parse(line: &str) -> Option<Step> {
    // RR-15 Limit: Dispatcher - one arm per verb the plan vocabulary has
    //
    // It passed fifty when `resize` was added. Splitting it would put half the verbs in
    // one function and half in another, and a reader asking what a plan can say would
    // have to find both.
    let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
    let rest = rest.trim();
    Some(match verb {
        "open" => Step::Open(PathBuf::from(rest)),
        "drawer" => Step::Drawer(drawer(rest)?),
        "select" => Step::Select(rest.parse().ok()?),
        "node" => Step::Node(rest.parse().ok()?),
        "zoom" => Step::Zoom(rest.parse().ok()?),
        "zoomin" => Step::ZoomIn,
        "selecttext" => Step::SelectText,
        "pin" => Step::Pin,
        "wheel" => {
            let mut parts = rest.split_whitespace();
            Step::Wheel(
                parts.next()?.parse().ok()?,
                parts.next()?.parse().ok()?,
                parts.next()?.parse().ok()?,
            )
        }
        "mark" => Step::Mark(rest.split_whitespace().filter_map(|n| n.parse().ok()).collect()),
        "marktitle" => Step::MarkTitle(rest.to_owned()),
        "markwrite" => Step::MarkWrite,
        "insert" => {
            let (path, at) = rest.rsplit_once(' ')?;
            Step::Insert(PathBuf::from(path), at.trim().parse().ok()?)
        }
        "extract" => Step::Extract(rest == "remove"),
        "selectall" => Step::SelectAll,
        "tool" => Step::Tool(rest.to_owned()),
        "mode" => Step::Mode(rest.to_owned()),
        "nudge" => {
            let (x, y) = rest.split_once(' ')?;
            Step::Nudge(x.trim().parse().ok()?, y.trim().parse().ok()?)
        }
        "setresize" => {
            let (sheet, fit) = rest.split_once(' ')?;
            Step::SetResize(sheet.trim().to_owned(), fit.trim().to_owned())
        }
        "resize" => {
            let (sheet, fit) = rest.split_once(' ')?;
            Step::Resize(sheet.trim().to_owned(), fit.trim().to_owned())
        }
        "protect" => Step::Protect(rest.to_owned()),
        "replace" => Step::Replace(PathBuf::from(rest)),
        "images" => Step::Images(PathBuf::from(rest)),
        "decorate" => {
            let (place, text) = rest.split_once(' ')?;
            Step::Decorate(place.to_owned(), text.trim().to_owned())
        }
        "openpage" => Step::OpenPage(rest.parse().ok()?),
        "probe" => Step::Probe(rest.to_owned()),
        "drag" => {
            let mut parts = rest.split_whitespace().map(str::parse);
            let mut next = || parts.next()?.ok();
            Step::Drag([next()?, next()?, next()?, next()?])
        }
        "pen" => {
            let (pen, words) = rest.split_once(' ').unwrap_or((rest, ""));
            Step::Pen(pen.to_owned(), words.trim().to_owned())
        }
        "readaloud" => Step::ReadAloud(PathBuf::from(rest)),
        "caliper" => Step::Caliper(rest.to_owned()),
        "printit" => Step::Print,
        "purpose" => Step::Purpose(rest.to_owned()),
        "split" | "combine" => {
            let (columns, rows) = rest.split_once(' ')?;
            let (columns, rows) = (columns.trim().parse().ok()?, rows.trim().parse().ok()?);
            if verb == "split" { Step::Split(columns, rows) } else { Step::Combine(columns, rows) }
        }
        "comparewith" => Step::CompareWith(PathBuf::from(rest)),
        "scale" => {
            let (n, unit) = rest.split_once(' ')?;
            Step::Scale(n.trim().parse().ok()?, unit.trim().to_owned())
        }
        "dblclick" => {
            let (x, y) = rest.split_once(' ')?;
            Step::DoubleClick(x.trim().parse().ok()?, y.trim().parse().ok()?)
        }
        "delete" => Step::Delete,
        "rotate" => Step::Rotate,
        "undo" => Step::Undo,
        "redo" => Step::Redo,
        "palette" => Step::Palette,
        // A key no command has is an unknown step, which refuses the plan.
        "command" => crate::command_palette::Command::ALL
            .into_iter()
            .any(|c| c.keys().0 == rest)
            .then(|| Step::Command(rest.to_owned()))?,
        "export" => Step::Export,
        "settings" => Step::Settings,
        "about" => Step::About,
        "language" => Step::Language(rest.to_owned()),
        "clear" => Step::Clear,
        "shot" => Step::Shot(rest.to_owned()),
        "clickat" => {
            let (x, y) = rest.split_once(' ')?;
            Step::ClickAt(x.trim().parse().ok()?, y.trim().parse().ok()?)
        }
        "clicklabel" if !rest.is_empty() => Step::ClickLabel(rest.to_owned()),
        "key" if crate::control::key_events(rest).is_some() => Step::Key(rest.to_owned()),
        "type" if !rest.is_empty() => Step::Type(rest.to_owned()),
        "inspect" => Step::Inspect(rest.to_owned()),
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
        "bookmarks" => ActiveDrawer::Bookmarks,
        "form" => ActiveDrawer::Form,
        "snapshot" => ActiveDrawer::Snapshot,
        "annotate" => ActiveDrawer::Annotate,
        "comments" => ActiveDrawer::Comments,
        "readaloud" => ActiveDrawer::ReadAloud,
        "compare" => ActiveDrawer::Compare,
        "print" => ActiveDrawer::Print,
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
        // A reading under way is not idle either: a plan that went on would close the
        // window, and the speech with it, before the step after `readaloud` could look.
        let idle = self.request_queue.is_empty()
            && !self.is_loading
            && self.busy.is_none()
            && drawn
            && self.read_aloud.waiting_from.is_none()
            && !self.compare.waiting
            && !self.print.waiting
            && !self.read_aloud.is_reading();
        let Some(plan) = self.capture.as_mut() else { return };
        plan.take_arrivals();
        if plan.pending.is_some() {
            return;
        }
        let Some(step) = plan.next(idle) else {
            if plan.finished() {
                // **The warning is answered before the close is asked for.** A document
                // with edits cancels its own close and puts up "export, discard or stay"
                // (`guard_close`), and a plan has no pointer to press any of the three
                // with — so the first plan to apply an operation ran until it was killed.
                // A plan that has run out is asking to end, and there is nobody to ask.
                self.close_confirmed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        };

        match step {
            Step::Open(path) => self.open_file(path, ctx),
            Step::Drawer(drawer) => self.show_drawer(drawer),
            Step::Select(page) => {
                self.selected_pages.clear();
                self.selected_pages.insert(page.saturating_sub(1));
                self.view.active_page = page.saturating_sub(1);
            }
            Step::Node(id) => self.ust_registry.selected_node_id = Some(id),
            Step::Insert(path, at) => self.insert_document_bytes(&path, at),
            Step::Extract(remove) => self.extract_selected_pages(remove),
            Step::DoubleClick(x, y) => {
                let viewport = self.last_viewport_rect.unwrap_or(egui::Rect::NOTHING);
                let at = viewport.min + egui::vec2(x as f32, y as f32);
                self.view.double_click_on_the_bench(at, viewport, &self.page_layouts);
                self.compute_layouts();
            }
            Step::Drag(ends) => self.queue_drag(ends),
            Step::Pen(name, words) => {
                self.show_drawer(ActiveDrawer::Annotate);
                let key = format!("annotate_{name}");
                if let Some(pen) = crate::annotate::Pen::ALL.into_iter().find(|p| p.keys().0 == key)
                {
                    self.annotate_tool.pen = pen;
                }
                self.annotate_tool.words = words;
            }
            Step::ReadAloud(folder) => {
                self.show_drawer(ActiveDrawer::ReadAloud);
                self.read_aloud.recording_into = Some(folder);
                self.read_aloud_from_here();
            }
            Step::Caliper(mode) => {
                self.show_drawer(ActiveDrawer::Caliper);
                self.caliper_tool.clear();
                self.caliper_tool.mode = if mode == "polygon" {
                    crate::measuring::CaliperMode::Polygon
                } else {
                    crate::measuring::CaliperMode::Distance
                };
            }
            Step::Scale(n, unit) => self.drive_scale(n, &unit),
            Step::Print => {
                self.show_drawer(ActiveDrawer::Print);
                self.print_document();
            }
            Step::Purpose(name) => {
                self.show_drawer(ActiveDrawer::Snapshot);
                self.snapshot_tool.purpose = match name.as_str() {
                    "crop" => crate::snapshot::Purpose::Crop,
                    "remove" => crate::snapshot::Purpose::CropAndRemove,
                    _ => crate::snapshot::Purpose::Copy,
                };
            }
            Step::Split(columns, rows) => {
                let page = self.selected_pages.iter().next().copied().unwrap_or_default();
                self.split_page(page, columns, rows, "menu_split_quarters");
            }
            Step::Combine(columns, rows) => {
                let pages = self.selected_pages.clone();
                let onto = fepdf::PageArrangement { sheet: None, columns, rows };
                self.combine_pages(&pages, onto, "menu_combine_four");
            }
            Step::CompareWith(path) => {
                self.show_drawer(ActiveDrawer::Compare);
                self.compare_with(path);
            }
            Step::Probe(label) => self.probe_placement(&label),
            Step::OpenPage(page) => {
                self.view.open_page(page.saturating_sub(1));
                self.view.set_zoom(1.0);
                self.compute_layouts();
            }
            Step::Tool(name) => {
                self.show_drawer(ActiveDrawer::Tools);
                self.tools.open = match name.as_str() {
                    "resize" => crate::document_tools::Tool::Resize,
                    "decoration" => crate::document_tools::Tool::Decoration,
                    _ => crate::document_tools::Tool::None,
                };
            }
            Step::Resize(sheet, fit) => self.drive_resize(&sheet, &fit),
            Step::Decorate(place, text) => self.drive_decoration(&place, text),
            Step::Replace(path) => {
                let pages = self.selected_pages.clone();
                self.replace_pages_from(&pages, &path);
            }
            Step::Images(folder) => {
                let pages = self.selected_pages.clone();
                self.export_pages_into(&pages, folder);
            }
            Step::Protect(how) => {
                self.export_password = (how == "password").then(String::new);
                self.export_owner_password = None;
                self.export_recipients = (how == "certificates").then(Vec::new);
            }
            Step::Mode(name) => {
                self.view.display_mode = match name.as_str() {
                    "spread" => crate::view::DisplayMode::TwoPageSingle,
                    _ => crate::view::DisplayMode::SinglePage,
                };
                self.compute_layouts();
            }
            Step::Nudge(x, y) => self.tools.offset = (f64::from(x), f64::from(y)),
            Step::SetResize(sheet, fit) => self.fill_resize_form(&sheet, &fit),
            Step::SelectAll => {
                self.selected_pages = (0..self.total_pages).collect();
            }
            Step::Mark(path) => self.bookmarks.choose(path),
            Step::MarkTitle(title) => self.bookmarks.retitle(&title),
            Step::MarkWrite => self.write_bookmarks(),
            Step::Wheel(x, y, steps) => self.wheel_zoom(x, y, steps),
            Step::Pin => self.controls_pinned = !self.controls_pinned,
            Step::SelectText => self.select_text_of_active_page(),
            Step::ZoomIn => {
                let viewport = self.last_viewport_rect.unwrap_or(egui::Rect::NOTHING);
                let step = self.view.zoom_step_up();
                self.view.zoom_at(step, viewport.center(), viewport, &self.page_layouts);
            }
            Step::Zoom(percent) => {
                #[allow(clippy::cast_precision_loss)]
                self.view.set_zoom(percent as f32 / 100.0);
                self.compute_layouts();
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
            Step::Command(key) => {
                let named =
                    crate::command_palette::Command::ALL.into_iter().find(|c| c.keys().0 == key);
                if let Some(command) = named {
                    command.run(self, ctx);
                }
            }
            Step::Export => self.open_export_wizard(),
            Step::Settings => self.show_settings_modal = true,
            Step::About => self.show_about_modal = true,
            Step::Language(lang) => self.active_language = lang,
            Step::Clear => {
                self.show_command_palette = false;
                self.show_export_wizard = false;
                self.show_settings_modal = false;
                self.show_about_modal = false;
                self.show_drawer(ActiveDrawer::None);
            }
            Step::Shot(name) => {
                if let Some(plan) = self.capture.as_mut() {
                    plan.pending = Some(name);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            #[allow(clippy::cast_precision_loss)]
            Step::ClickAt(x, y) => {
                self.queue_input(crate::control::click_events(egui::pos2(x as f32, y as f32)));
            }
            Step::ClickLabel(name) => self.click_label(&name, ctx),
            Step::Key(chord) => {
                self.queue_input(crate::control::key_events(&chord).unwrap_or_default());
            }
            Step::Type(text) => self.queue_input([vec![egui::Event::Text(text)]].into()),
            Step::Inspect(query) => self.inspect(&query, ctx),
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

impl crate::app::FepdfApp {
    /// Selects everything on the page being shown, without a pointer.
    fn select_text_of_active_page(&mut self) {
        let page = self.view.active_page;
        let Some(layout) = self.page_layouts.get(page) else { return };
        let Some(spans) = self.page_spans.get(&page) else { return };
        // The drag is in the page's own coordinates, corner to corner, which is what a
        // pointer's positions are turned into before anything reads them.
        self.selection_manager.active_page = Some(page);
        self.selection_manager.drag_start = Some(egui::pos2(0.0, 0.0));
        self.selection_manager.drag_current =
            Some(egui::pos2(layout.rect.width(), layout.rect.height()));
        self.selection_manager.recalculate_selection(page, spans);
    }
}

impl crate::app::FepdfApp {
    /// The same call `handle_zoom_gestures` makes for a `⌘`-scroll, `steps` times.
    #[allow(clippy::cast_precision_loss)]
    fn wheel_zoom(&mut self, x: u32, y: u32, steps: i32) {
        let Some(viewport) = self.last_viewport_rect else { return };
        let at = viewport.min + egui::vec2(x as f32, y as f32);
        let notch = if steps < 0 { -40.0_f32 } else { 40.0_f32 };
        for _ in 0..steps.abs() {
            // 40 points of scroll, which is what one notch of a wheel sends.
            let factor = (notch * 0.005).exp();
            let target = self.view.zoom_before_snapping() * factor;
            self.view.zoom_at(target, at, viewport, &self.page_layouts);
            self.compute_layouts();
        }
    }
}

/// How many frames a `drag` moves over between press and release.
///
/// Enough that egui sees movement past its click distance before the release, and that a
/// tool collecting points — the ink pen — gets more than its two ends.
const DRAG_FRAMES: u16 = 12;

/// The events of a drag from `from` to `to`, one batch a frame: arrive, press, move, let go.
fn drag_events(from: egui::Pos2, to: egui::Pos2) -> VecDeque<Vec<egui::Event>> {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut frames = VecDeque::new();
    frames.push_back(vec![egui::Event::PointerMoved(from)]);
    frames.push_back(vec![button(from, true)]);
    for step in 1..=DRAG_FRAMES {
        let t = f32::from(step) / f32::from(DRAG_FRAMES);
        frames.push_back(vec![egui::Event::PointerMoved(from.lerp(to, t))]);
    }
    frames.push_back(vec![button(to, false)]);
    frames
}

impl crate::app::FepdfApp {
    /// Puts events in the plan's input, one batch a frame.
    fn queue_input(&mut self, events: VecDeque<Vec<egui::Event>>) {
        if let Some(plan) = self.capture.as_mut() {
            plan.input = events;
        }
    }

    /// The widgets the last frame drew, from its AccessKit tree.
    fn widgets(ctx: &egui::Context) -> Vec<crate::control::Widget> {
        ctx.plugin_opt::<crate::control::WidgetTree>()
            .map(|tree| tree.lock().widgets.clone())
            .unwrap_or_default()
    }

    /// Clicks the middle of the first widget named `name`, or says there is none.
    fn click_label(&mut self, name: &str, ctx: &egui::Context) {
        let widgets = Self::widgets(ctx);
        let Some(plan) = self.capture.as_mut() else { return };
        match crate::control::matching(&widgets, name).first() {
            Some(widget) => {
                plan.say(&format!("click\t{}", crate::control::describe(widget)));
                plan.input = crate::control::click_events(widget.rect.center());
            }
            None => plan.say(&format!("no widget named: {name}")),
        }
    }

    /// Says which widgets the last frame drew whose names contain `query`.
    fn inspect(&mut self, query: &str, ctx: &egui::Context) {
        let widgets = Self::widgets(ctx);
        let Some(plan) = self.capture.as_ref() else { return };
        let found = crate::control::matching(&widgets, query);
        for widget in &found {
            plan.say(&crate::control::describe(widget));
        }
        plan.say(&format!("inspected {} of {}", found.len(), widgets.len()));
    }

    /// Puts a drag in the plan's input, in screen points.
    #[allow(clippy::cast_precision_loss)]
    fn queue_drag(&mut self, [x0, y0, x1, y1]: [u32; 4]) {
        let viewport = self.last_viewport_rect.unwrap_or(egui::Rect::NOTHING);
        let at = |x: u32, y: u32| viewport.min + egui::vec2(x as f32, y as f32);
        if let Some(plan) = self.capture.as_mut() {
            plan.input = drag_events(at(x0, y0), at(x1, y1));
        }
    }

    /// Hands egui this frame's share of a plan's drag, where the platform's events go.
    ///
    /// **A key brings what is held with it into the frame.** A person pressing ⌘K holds ⌘
    /// for the whole frame, and egui reports that as the frame's modifiers, which is what
    /// the palette's shortcut reads (`i.modifiers.command`). The event alone carried the
    /// modifiers and the frame's said nothing was held, so `key cmd+K` reached the window
    /// and opened nothing.
    pub(crate) fn feed_capture_input(&mut self, raw_input: &mut egui::RawInput) {
        if let Some(plan) = self.capture.as_mut() {
            let events = plan.input_for_this_frame();
            if let Some(held) = crate::control::held(&events) {
                raw_input.modifiers = held;
            }
            raw_input.events.extend(events);
        }
    }
}

impl crate::app::FepdfApp {
    /// Sets the page's scale through the caliper drawer's form, as its button would.
    fn drive_scale(&mut self, denominator: u32, unit: &str) {
        let Some(index) = crate::measuring::UNITS.iter().position(|(u, _)| *u == unit) else {
            return;
        };
        self.caliper_tool.form =
            crate::measuring::ScaleForm { denominator: f64::from(denominator), unit: index };
        let page = self.view.active_page;
        let user_unit = self.caliper_tool.user_unit(page);
        if let Some(scale) = self.caliper_tool.form.scale(page, user_unit) {
            let done = self.tr("caliper_scale_set");
            let _ = self.tx_worker.send(crate::worker::WorkerRequest::Apply {
                operation: Box::new(fepdf::Operation::SetMeasurementScale(scale)),
                done,
            });
        }
    }
}

#[cfg(test)]
mod commands {
    use super::{Step, parse};

    /// **A plan runs a palette command by its key, and a key no command has refuses the
    /// plan.** The redaction brush's one door was the palette, so a plan could not draw a
    /// zone, and the preview a zone asks for went unseen in the window.
    #[test]
    fn a_command_is_named_by_its_key() {
        assert_eq!(
            parse("command cmd_redaction_studio"),
            Some(Step::Command("cmd_redaction_studio".to_owned()))
        );
        assert_eq!(parse("command cmd_no_such_thing"), None);
    }

    /// The verbs a person at the window has: a point, a widget by name, a key, words, and
    /// a question. A key egui has no name for, and a click with no words, are no steps.
    #[test]
    fn the_control_verbs_are_read() {
        assert_eq!(parse("clickat 10 20"), Some(Step::ClickAt(10, 20)));
        assert_eq!(parse("clicklabel Zoom in"), Some(Step::ClickLabel("Zoom in".to_owned())));
        assert_eq!(parse("key cmd+K"), Some(Step::Key("cmd+K".to_owned())));
        assert_eq!(parse("type invoice"), Some(Step::Type("invoice".to_owned())));
        assert_eq!(parse("inspect"), Some(Step::Inspect(String::new())));
        assert_eq!(parse("key cmd+NoSuchKey"), None);
        assert_eq!(parse("clicklabel"), None);
    }
}
