//! Central state container and egui UI dispatch loop for `fepdf-gui`.

pub mod icons;
mod layout;
mod modals;
mod page_ops;
mod side_panels;
mod status_bar;
pub mod theme;
mod view_panel;

use crate::interaction::{SelectionManager, TextSpan};
use crate::redaction::RedactionManager;
use crate::sidebar::{SidebarPanel, USTRegistry};
use crate::vello_egui::VelloRenderer;
use crate::view::{PDFView, PageLayout};
use crate::worker::{WorkerRequest, WorkerResponse, run_worker};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use theme::{colors, size, space, text};
use vello::Scene;

/// A document that opened encrypted, waiting for the password that unlocks it.
///
/// The bytes are kept rather than the path: a file the reader chose through a dialog may
/// not be re-openable by path, and reading it twice to answer one question is work the
/// worker already did.
pub struct LockedDocument {
    pub data: bytes::Bytes,
    pub name: Option<String>,
    /// The handler that locked it, as the engine names it — "Password Security (AES-256)".
    pub method: String,
    /// What the reader has typed, not yet tried.
    pub attempt: String,
    /// Whether a password has already been refused, which is a different thing to say.
    pub refused: bool,
}

/// How loudly the window has to say something.
///
/// **A success and a failure cannot share a type, because they did.** One
/// `error: Option<String>` carried both — a save reported through it, a tag creation
/// reported through it, and the reader saw the same red text over an emptied canvas
/// either way. The compiler now refuses to confuse them, which is the only kind of
/// enforcement this rule can have (`CODING.md` UI-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// It worked.
    Done,
    /// It worked, and something about it is worth looking at.
    Check,
    /// It did not work.
    Failed,
}

/// One line the window owes the reader, and how loudly to say it.
///
/// **A key and a detail, not a sentence.** The worker thread has no locale and cannot
/// have one usefully — it is where the document is, not where the reader is — so it used
/// to format its own English: "Failed to load PDF: …", six times over, inside a window
/// that is otherwise entirely translated. The frame is a key the window resolves; the
/// detail is the part that has no translation, which is the engine's own sentence naming
/// an ISO clause.
pub struct Notice {
    /// How loudly.
    pub level: Level,
    /// The locale key of what to say, with `{}` where the detail goes.
    pub key: &'static str,
    /// What the engine said, when it said anything.
    pub detail: Option<String>,
}

impl Notice {
    /// It worked.
    pub const fn done(key: &'static str) -> Self {
        Self { level: Level::Done, key, detail: None }
    }

    /// It worked, and there is something to look at.
    pub const fn check(key: &'static str) -> Self {
        Self { level: Level::Check, key, detail: None }
    }

    /// It did not work.
    pub const fn failed(key: &'static str) -> Self {
        Self { level: Level::Failed, key, detail: None }
    }

    /// The same notice, carrying what the engine said about it.
    #[must_use]
    pub fn about(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// What to put on the bar, in the reader's language.
    pub fn say(&self, locale: &crate::locale::LocaleManager, lang: &str) -> String {
        let frame = locale.tr(lang, self.key);
        self.detail.as_ref().map_or_else(|| frame.clone(), |d| frame.replace("{}", d))
    }

    /// The colour this level is said in.
    pub fn colour(&self) -> egui::Color32 {
        match self.level {
            Level::Done => theme::colors::note::PASS,
            Level::Check => theme::colors::note::WARN,
            Level::Failed => theme::colors::note::FAIL,
        }
    }
}

pub struct FepdfApp {
    pub tx_worker: Sender<WorkerRequest>,
    pub rx_worker: Receiver<WorkerResponse>,
    /// Set while a document waits for its password; `None` the rest of the time.
    pub locked: Option<LockedDocument>,
    pub survey: crate::sidebar::what_it_does::Survey,
    pub tools: crate::document_tools::ToolState,

    pub total_pages: usize,
    pub page_layouts: Vec<PageLayout>,

    pub view: PDFView,
    /// The one line this window has to say, until the reader dismisses it.
    pub notice: Option<Notice>,
    pub pdf_name: Option<String>,

    pub vello_renderer: Option<VelloRenderer>,
    pub scenes: BTreeMap<usize, Arc<Scene>>,
    pub request_queue: BTreeSet<usize>,

    pub selection_manager: SelectionManager,
    pub page_spans: BTreeMap<usize, Vec<TextSpan>>,

    pub ust_registry: USTRegistry,
    pub sidebar_panel: SidebarPanel,

    pub redaction_manager: RedactionManager,
    pub redaction_studio_panel: crate::redaction_studio::RedactionStudioPanel,
    pub show_export_wizard: bool,
    /// Whether the saved document is encrypted, and with what (7.6.4).
    ///
    /// **Empty is not the same as absent.** A `/U` password of "" is a real encryption
    /// with an empty user password, which is what most "encrypted" PDFs on the web are;
    /// this is `None` when the reader asked for no protection at all.
    pub export_password: Option<String>,
    /// `/O`, which restricts what a reader who has only the user password may do.
    pub export_owner_password: Option<String>,
    pub export_compress: bool,
    pub export_linearize: bool,
    pub export_vacuum: bool,
    pub export_upgrade_pdf20: bool,
    pub export_apply_tags: bool,
    pub export_burn_redactions: bool,
    pub raw_texts: BTreeMap<usize, String>, // page_index -> raw extracted text

    // Digital Signature & Placement
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub signature_position: Option<(usize, egui::Rect)>, // (page_index, rect in PDF user space)
    pub is_placing_signature: bool,

    // CAD snappers & Inspector
    pub cad_snap_engine: crate::cad_canvas::CadSnapEngine,
    pub caliper_tool: crate::cad_canvas::CaliperTool,
    pub active_drawer: crate::sidebar::ActiveDrawer,

    // Selection management
    pub selected_pages: BTreeSet<usize>,
    pub last_selected_page: Option<usize>,
    pub clear_thumbnails_pending: bool,
    pub invalidated_thumbnails: BTreeSet<usize>,
    pub is_loading: bool,
    pub loading_message: String,
    pub show_reading_order: bool,
    pub show_command_palette: bool,
    pub command_palette_search: String,
    pub last_viewport_rect: Option<egui::Rect>,
    pub show_about_modal: bool,
    pub locale_mgr: crate::locale::LocaleManager,
    pub active_language: String,
    pub show_settings_modal: bool,
    pub doc_metadata: Option<fepdf::MetadataInfo>,
    pub doc_file_size: Option<usize>,
    pub doc_version: Option<String>,
    pub doc_security_method: Option<String>,
    pub doc_permissions: Option<i32>,
    pub doc_page_sizes: Vec<(f64, f64)>,
    pub doc_fonts: Vec<fepdf::FontSummary>,
    /// What to present for optional content (6.3.2.3), refreshed whenever a layer is
    /// toggled so the checkboxes show the state actually in force.
    pub layers: Vec<fepdf::LayerRow>,
    /// Reading decisions recorded by the engine while opening or repairing the document (6.3.2.3).
    pub doc_decisions: Vec<fepdf::Decision>,
    /// Whether there is an operation to take back, and one to put back.
    pub can_undo: bool,
    /// See [`Self::can_undo`].
    pub can_redo: bool,
    /// Whether the document differs from the file it was opened from.
    ///
    /// **Derived from the worker's journal rather than kept here.** A second count of
    /// what has been applied is a second thing to get wrong, and this one would be the
    /// copy that is not the document.
    pub edited: bool,
    /// Set while the window is asking whether to close with edits outstanding.
    pub confirming_close: bool,
    /// The locale key for whatever long thing is running, while it runs.
    ///
    /// **Set by the worker as well as by an undo.** It held only the history at first,
    /// which meant the one action that said it was working was the one that had needed
    /// saying least — a save and a `PDF/UA` audit both take longer and both said nothing
    /// (UI-7).
    pub busy: Option<&'static str>,
    /// Set once the reader has said to close anyway, so the guard lets the next request
    /// through.
    pub close_confirmed: bool,
    /// The plan driving this window, when one was given on the command line.
    pub capture: Option<crate::capture::Plan>,
    /// Visible pages the last frame left undrawn against the renderer's bin-data budget.
    ///
    /// **Not a `Decision`.** Every severity in that list describes the *document* — the
    /// sidebar would badge this one `規格違反 [ISO 32000-2]` — and nothing is wrong with a
    /// file that happens to have more pages on screen than one vello scene can hold. It is
    /// the engine's own limit, so it is reported as the engine's own state.
    pub pages_left_out: usize,
}

impl FepdfApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // RR-15 Limit: GUI - App state creation and initialization
        let vello_renderer =
            cc.wgpu_render_state.as_ref().and_then(|rs| VelloRenderer::new(&rs.device));
        let (tx_req, rx_req) = channel();
        let (tx_res, rx_res) = channel();

        // Configure system fonts, icon font, and visual theme
        theme::configure_fonts_and_styles(&cc.egui_ctx);

        // `Cmd +`, `Cmd -` and `Cmd 0` belong to the document, not to egui's own interface
        // scaling. Left on, egui's default did **both** on every press: it stepped the
        // document zoom through `handle_zoom_shortcuts` and separately shrank the whole
        // interface by changing `pixels_per_point`, so the sidebar and the status bar
        // scaled with the page and a labelled 67% drew nothing like 67%.
        //
        // It also inflated the viewport measured in logical points, which is how one frame
        // came to report 1,960 visible pages of `intel_sdm.pdf` where the layout accounts
        // for 253 (ADR-0054). That figure was recorded as unexplained; this is the cause.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        // eframe persists the interface scale, so a window shrunk by the old behaviour
        // would stay shrunk after the shortcut was taken away from it.
        cc.egui_ctx.set_zoom_factor(1.0);

        let egui_ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            run_worker(rx_req, tx_res, egui_ctx);
        });

        Self {
            tx_worker: tx_req,
            rx_worker: rx_res,
            total_pages: 0,
            page_layouts: Vec::new(),
            view: PDFView::new(),
            notice: None,
            pdf_name: None,
            vello_renderer,
            scenes: BTreeMap::new(),
            request_queue: BTreeSet::new(),
            selection_manager: SelectionManager::new(),
            page_spans: BTreeMap::new(),
            ust_registry: USTRegistry::new(),
            sidebar_panel: SidebarPanel::new(),
            redaction_manager: RedactionManager::new(),
            redaction_studio_panel: crate::redaction_studio::RedactionStudioPanel::new(),
            show_export_wizard: false,
            export_password: None,
            export_owner_password: None,
            export_compress: true,
            export_linearize: true,
            export_vacuum: true,
            export_upgrade_pdf20: true,
            export_apply_tags: true,
            export_burn_redactions: true,
            raw_texts: BTreeMap::new(),

            // Signature Defaults
            cert_path: None,
            key_path: None,
            signature_position: None,
            is_placing_signature: false,

            // CAD & Inspector Defaults
            cad_snap_engine: crate::cad_canvas::CadSnapEngine::new(),
            caliper_tool: crate::cad_canvas::CaliperTool::new(),
            active_drawer: crate::sidebar::ActiveDrawer::None,

            // Selection Defaults
            selected_pages: BTreeSet::new(),
            last_selected_page: None,
            clear_thumbnails_pending: false,
            invalidated_thumbnails: BTreeSet::new(),
            is_loading: false,
            loading_message: String::new(),
            can_undo: false,
            can_redo: false,
            edited: false,
            confirming_close: false,
            busy: None,
            close_confirmed: false,
            capture: None,
            show_reading_order: true,
            show_command_palette: false,
            command_palette_search: String::new(),
            last_viewport_rect: None,
            show_about_modal: false,
            locale_mgr: crate::locale::LocaleManager::new(),
            active_language: "ja".to_string(),
            show_settings_modal: false,
            doc_metadata: None,
            doc_file_size: None,
            doc_version: None,
            doc_security_method: None,
            doc_permissions: None,
            doc_page_sizes: Vec::new(),
            doc_fonts: Vec::new(),
            layers: Vec::new(),
            doc_decisions: Vec::new(),
            pages_left_out: 0,
            locked: None,
            survey: crate::sidebar::what_it_does::Survey::default(),
            tools: crate::document_tools::ToolState::default(),
        }
    }

    fn process_worker_messages(&mut self, ctx: &egui::Context) {
        // RR-15 Limit: GUI - Handle asynchronous background messages
        while let Ok(msg) = self.rx_worker.try_recv() {
            match msg {
                WorkerResponse::NeedsPassword { data, name, method, retried } => {
                    // Not an error: the file is fine and the reader has not been asked yet.
                    self.is_loading = false;
                    self.locked = Some(crate::app::LockedDocument {
                        data,
                        name,
                        method,
                        attempt: String::new(),
                        refused: retried,
                    });
                    ctx.request_repaint();
                }
                WorkerResponse::LoadingProgress { message } => {
                    self.loading_message = message;
                    ctx.request_repaint();
                }
                WorkerResponse::LayersChanged { layers } => {
                    // The page must be drawn again: a layer's state decides what the
                    // interpreter paints, and every cached scene predates the toggle.
                    self.layers = layers;
                    self.scenes.clear();
                    self.raw_texts.clear();
                    self.page_spans.clear();
                }
                WorkerResponse::DocumentLoaded(loaded) => {
                    let crate::worker::LoadedDocument {
                        name,
                        num_pages,
                        page_sizes,
                        ust_root,
                        file_size,
                        version,
                        metadata,
                        security_method,
                        permissions,
                        fonts,
                        viewer_direction,
                        layers,
                        decisions,
                    } = *loaded;
                    self.layers = layers;
                    self.doc_decisions = decisions;
                    if name.is_some() {
                        self.pdf_name = name;
                    }
                    self.total_pages = num_pages;
                    self.scenes.clear();
                    self.raw_texts.clear();
                    self.page_spans.clear();
                    self.clear_thumbnails_pending = true;
                    if let Some(ref dir) = viewer_direction {
                        if dir.eq_ignore_ascii_case("R2L") {
                            self.view.binding_direction =
                                crate::view::BindingDirection::RightToLeft;
                        } else {
                            self.view.binding_direction =
                                crate::view::BindingDirection::LeftToRight;
                        }
                    } else {
                        self.view.binding_direction = crate::view::BindingDirection::LeftToRight;
                    }
                    self.doc_page_sizes = page_sizes;
                    self.compute_layouts();

                    self.doc_file_size = Some(file_size);
                    self.doc_version = Some(version);
                    self.doc_metadata = Some(metadata);
                    self.doc_security_method = Some(security_method);
                    self.doc_permissions = permissions;
                    self.doc_fonts = fonts;

                    // Load parsed accessibility tag tree
                    self.ust_registry.root = ust_root;

                    // Kick off Matterhorn compliance audit asynchronously in the background
                    let _ = self.tx_worker.send(WorkerRequest::Audit);

                    self.is_loading = false;
                    self.busy = None;
                    ctx.request_repaint();
                }
                WorkerResponse::PageRendered { index, scene, text, spans, .. } => {
                    self.scenes.insert(index, scene);
                    self.request_queue.remove(&index);
                    self.invalidated_thumbnails.insert(index);

                    if let Some(text) = text {
                        self.raw_texts.insert(index, text);
                    }

                    if let Some(spans) = spans {
                        self.page_spans.insert(index, spans);
                    } else if let Some(text) = self.raw_texts.get(&index)
                        && let Some(layout) = self.page_layouts.get(index)
                    {
                        let size = layout.rect.size();
                        let spans = SelectionManager::generate_spans_for_page(text, size.x, size.y);
                        self.page_spans.insert(index, spans);
                    }

                    ctx.request_repaint();
                }
                WorkerResponse::Busy { key } => {
                    self.busy = Some(key);
                    ctx.request_repaint();
                }
                WorkerResponse::Idle => {
                    self.busy = None;
                    ctx.request_repaint();
                }
                WorkerResponse::HistoryChanged { can_undo, can_redo, edited } => {
                    self.can_undo = can_undo;
                    self.can_redo = can_redo;
                    self.edited = edited;
                    ctx.request_repaint();
                }
                WorkerResponse::AuditFindings { findings } => {
                    self.ust_registry.audit_findings = findings;
                    ctx.request_repaint();
                }
                WorkerResponse::Surveyed { actions, coverage } => {
                    self.survey.actions = Some(actions);
                    self.survey.coverage = coverage;
                    ctx.request_repaint();
                }
                WorkerResponse::OperationApplied { message } => {
                    self.notice = Some(Notice::done("notice_attach_failed").about(message));
                    // The pages the operation moved are on screen, and every cached scene
                    // predates it.
                    self.scenes.clear();
                    self.raw_texts.clear();
                    self.page_spans.clear();
                    ctx.request_repaint();
                }
                WorkerResponse::DocumentSaved { path, notices } => {
                    let name = path.file_name().unwrap_or(path.as_os_str()).display();
                    self.notice = Some(if notices.is_empty() {
                        Notice::done("notice_exported").about(name.to_string())
                    } else {
                        Notice::check("notice_exported_with_notices")
                            .about(format!("{name} — {}", notices.join("; ")))
                    });
                    ctx.request_repaint();
                }
                WorkerResponse::Failed { key, detail } => {
                    self.is_loading = false;
                    self.busy = None;
                    let notice = Notice::failed(key);
                    self.notice = Some(detail.map_or(notice, |d| Notice::failed(key).about(d)));
                }
            }
        }
    }

    fn update_vello(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // RR-15 Limit: Dispatcher
        let ctx = ui.ctx().clone();
        self.process_worker_messages(&ctx);

        if !self.check_gpu_support(ui, frame) {
            return;
        }

        let rs = match frame.wgpu_render_state() {
            Some(state) => state,
            None => return,
        };

        if self.clear_thumbnails_pending {
            if let Some(ref mut r) = self.vello_renderer {
                r.clear_thumbnails(rs);
            }
            self.clear_thumbnails_pending = false;
        }

        if !self.invalidated_thumbnails.is_empty()
            && let Some(ref mut r) = self.vello_renderer
        {
            for page_idx in std::mem::take(&mut self.invalidated_thumbnails) {
                r.invalidate_thumbnail(rs, page_idx);
            }
        }

        self.queue_visible_pages();

        egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
            let bg_color = crate::app::theme::colors::paper::CANVAS;
            ui.painter().rect_filled(ui.max_rect(), theme::radius::FLAT, bg_color);

            // **A notice no longer replaces the document.** Every one of them — a save
            // that worked included — used to be drawn here, centred and red, over a
            // canvas emptied of the pages it was reporting on; the reader's thirteen-page
            // document vanished behind "Successfully exported". Notices belong on the
            // status bar, which is where a window says what it is doing.
            if !self.page_layouts.is_empty() {
                let viewport_rect = ui.max_rect();
                self.last_viewport_rect = Some(viewport_rect);
                self.render_document_panel(ui, rs, viewport_rect);
            } else if self.is_loading {
                ui.centered_and_justified(|ui| {
                    ui.label(&self.loading_message);
                });
            } else {
                self.render_empty_state(ui);
            }
        });
    }

    /// What the window says when it holds no document.
    ///
    /// **It used to say nothing at all.** The branch that reaches here had no `else`, so
    /// a reader opening the application met an empty canvas with a 32-point unlabelled
    /// arrow in the corner of a 3,024-pixel screen, and no statement anywhere of what to
    /// do or that dropping a file would work. An entry point nobody can find is not an
    /// entry point (principle P3), and this is the first screen there is.
    fn render_empty_state(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let tr = |key: &str| self.locale_mgr.tr(&self.active_language, key);
        let mut chosen = None;

        ui.vertical_centered(|ui| {
            let free = ui.available_height();
            ui.add_space(free / 3.0);

            ui.label(
                egui::RichText::new(icons::glyph::OPEN)
                    .size(size::ICON)
                    .family(theme::icon_family())
                    .color(colors::steel::EDGE),
            );
            ui.add_space(space::SECTION);

            ui.label(
                egui::RichText::new(tr("empty_title")).size(text::TITLE).color(colors::steel::TEXT),
            );
            ui.add_space(space::PANE);

            let choose =
                egui::Button::new(egui::RichText::new(tr("empty_choose")).size(text::BODY))
                    .min_size(egui::vec2(size::FORM_W / 2.0, size::ICON))
                    .corner_radius(theme::radius::CONTROL);
            if ui.add(choose).clicked()
                && let Some(path) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file()
            {
                chosen = Some(path);
            }
            ui.add_space(space::PANE);

            for key in ["drop_to_open_pdf", "tooltip_import_pdf", "empty_palette"] {
                ui.label(
                    egui::RichText::new(tr(key)).size(text::SMALL).color(colors::steel::MUTED),
                );
                ui.add_space(space::ITEM);
            }
        });

        if let Some(path) = chosen {
            self.open_file(path, &ctx);
        }
    }

    fn handle_file_and_edit_shortcuts(&mut self, ui: &egui::Ui) {
        let ctx = ui.ctx();
        let dropped_file = ui.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped_file
            && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            if self.total_pages > 0 {
                if let Ok(exe) = std::env::current_exe()
                    && let Err(e) = std::process::Command::new(exe).arg(&path).spawn()
                {
                    log::warn!("the external viewer would not start: {e}");
                }
            } else {
                self.open_file(path, ctx);
            }
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::O))
            && let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file()
        {
            if self.total_pages > 0 {
                if let Ok(exe) = std::env::current_exe()
                    && let Err(e) = std::process::Command::new(exe).arg(p).spawn()
                {
                    log::warn!("the external viewer would not start: {e}");
                }
            } else {
                self.open_file(p, ctx);
            }
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::E)) && self.total_pages > 0
        {
            self.show_export_wizard = true;
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::K)) {
            self.show_command_palette = !self.show_command_palette;
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::C))
            && !self.selection_manager.selected_text.is_empty()
        {
            ctx.copy_text(self.selection_manager.selected_text.clone());
        }
    }

    /// `Cmd+Z` and `Cmd+Shift+Z`, which is the platform's pair.
    ///
    /// **Both are refused rather than queued when the history has nothing at that end**,
    /// so a held key cannot outrun the worker and ask for more undo than there is. The
    /// flag is cleared here as well as by the worker's answer, because the answer takes
    /// as long as the rebuild does.
    fn handle_history_shortcuts(&mut self, ui: &egui::Ui) {
        let pressed = |shift: bool| {
            ui.input(|i| {
                i.modifiers.command && i.modifiers.shift == shift && i.key_pressed(egui::Key::Z)
            })
        };
        if pressed(false) && self.can_undo {
            self.can_undo = false;
            self.begin_rebuild("history_undoing");
            let _ = self.tx_worker.send(WorkerRequest::Undo);
        }
        if pressed(true) && self.can_redo {
            self.can_redo = false;
            self.begin_rebuild("history_redoing");
            let _ = self.tx_worker.send(WorkerRequest::Redo);
        }
    }

    /// Says that the document is being rebuilt, and drops everything drawn from the old
    /// one.
    ///
    /// **Undo re-opens the file and replays what is left**, because the engine has no
    /// inverse for an operation — see `worker::History`. On the samples that is 28ms for
    /// `constitution.pdf` and 1.7s for `intel_sdm.pdf` at 24MB, which makes this the one
    /// action in this window long enough to owe the reader a word while it runs (UI-7).
    /// A user-facing string, by its key.
    ///
    /// **The shorthand is the point.** Thirty-nine sentences were written into the source
    /// in Japanese — every tooltip on the status bar and the whole page context menu —
    /// and `self.locale_mgr.tr(&self.active_language, key)` at each of them is what made
    /// the literal look like the cheaper option (UI-5).
    pub fn tr(&self, key: &str) -> String {
        self.locale_mgr.tr(&self.active_language, key)
    }

    /// Stops a close that would take unexported edits with it.
    ///
    /// **The window had no idea it had been edited.** `dirty`, `unsaved`, `on_close` and
    /// `CloseRequested` appeared nowhere in this crate: thirty pages could be deleted and
    /// the window closed on them without a word. `History::edited` is the answer to the
    /// question and this is the only place that asks it.
    fn guard_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        if self.edited && !self.close_confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirming_close = true;
        }
    }

    pub(crate) fn begin_rebuild(&mut self, key: &'static str) {
        self.busy = Some(key);
        self.scenes.clear();
        self.raw_texts.clear();
        self.page_spans.clear();
        self.request_queue.clear();
        self.clear_thumbnails_pending = true;
    }

    fn handle_zoom_shortcuts(&mut self, ui: &egui::Ui) {
        let viewport_rect = self.last_viewport_rect.unwrap_or_else(|| ui.max_rect());
        let cursor_pos = ui.input(|i| {
            i.pointer
                .hover_pos()
                .or(i.pointer.latest_pos())
                .filter(|p| viewport_rect.contains(*p))
                .unwrap_or_else(|| viewport_rect.center())
        });

        if ui.input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::Num0))
        {
            self.view.zoom_at(1.0, cursor_pos, viewport_rect, &self.page_layouts);
        }
        if ui.input(|i| {
            (i.modifiers.command || i.modifiers.ctrl)
                && (i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals))
        }) {
            self.view.zoom_at(
                self.view.zoom_step_up(),
                cursor_pos,
                viewport_rect,
                &self.page_layouts,
            );
        }
        if ui
            .input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::Minus))
        {
            self.view.zoom_at(
                self.view.zoom_step_down(),
                cursor_pos,
                viewport_rect,
                &self.page_layouts,
            );
        }
    }

    fn handle_page_and_selection_shortcuts(&mut self, ui: &egui::Ui) {
        // Removing pages belongs where pages are arranged. A selection survives the trip
        // into the page view — checking a page before deleting it is the ordinary reason to
        // zoom in — so the key that acts on it stays behind rather than the selection being
        // thrown away. In the page view `Delete` could only ever have meant a page the
        // reader was not looking at.
        if ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
            && self.view.selects_pages()
            && !self.selected_pages.is_empty()
            && self.total_pages > 1
        {
            self.remove_selected_pages();
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.selected_pages.clear();
            self.selection_manager.clear();
        }
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A))
            && !self.view.is_page_view()
            && self.total_pages > 0
        {
            self.selected_pages.clear();
            for p in 0..self.total_pages {
                self.selected_pages.insert(p);
            }
        }
        if self.total_pages > 0 {
            let shift = ui.input(|i| i.modifiers.shift);
            let prev_key = ui.input(|i| {
                i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowUp)
            });
            let next_key = ui.input(|i| {
                i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowDown)
            });
            if prev_key && self.view.active_page > 0 {
                self.view.active_page -= 1;
                if !shift {
                    self.selected_pages.clear();
                }
                self.selected_pages.insert(self.view.active_page);
                self.last_selected_page = Some(self.view.active_page);
            } else if next_key && self.view.active_page + 1 < self.total_pages {
                self.view.active_page += 1;
                if !shift {
                    self.selected_pages.clear();
                }
                self.selected_pages.insert(self.view.active_page);
                self.last_selected_page = Some(self.view.active_page);
            }
        }
    }

    fn handle_keyboard_shortcuts(&mut self, ui: &egui::Ui) {
        self.handle_file_and_edit_shortcuts(ui);
        self.handle_history_shortcuts(ui);
        self.handle_zoom_shortcuts(ui);
        self.handle_page_and_selection_shortcuts(ui);
    }
}

impl eframe::App for FepdfApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // RR-15 Limit: Dispatcher - Main application UI shell layout routing layout panels and windows
        let ctx = ui.ctx().clone();
        theme::apply_global_styles(&ctx);

        // `apply_global_styles` above is the only place these are set. The block that
        // stood here re-set five of them on the root `Ui` every frame, and one of the
        // five discarded `rust::wash()` for a grey — so the palette held a selection
        // colour that rustc counted as used and the screen never showed.

        let entire_rect = ui.max_rect();
        ui.painter().rect_filled(entire_rect, theme::radius::FLAT, ui.visuals().window_fill);

        self.guard_close(&ctx);
        self.handle_keyboard_shortcuts(ui);

        // 1. Bottom status bar (with page navigation & zoom controls)
        self.render_status_bar(ui);

        // 2. Left vertical icon bar (file ops, drawer toggles, utilities)
        self.render_left_icon_bar(ui);

        // 3. Left utility drawer (when active)
        self.render_side_drawer(ui);

        // 4. Central PDF view canvas (full width main panel)
        self.update_vello(ui, frame);

        // 5. Floating modals & dialogs
        self.render_overlay_windows(&ctx);

        // 6. Last, so a screenshot catches what the five above drew.
        self.drive_capture(&ctx);
    }
}

#[cfg(test)]
mod notices {
    use super::{Level, Notice, theme::colors};

    /// **A success and a failure must not arrive at the same colour**, which is the
    /// visible half of what the type separation is for. The other half — that a save
    /// cannot be reported as a failure — is held by the compiler and needs no test:
    /// `Notice::done` is the only way to say a thing worked.
    #[test]
    fn each_level_says_a_different_thing() {
        let done = Notice::done("notice_exported");
        let check = Notice::check("notice_exported_with_notices");
        let failed = Notice::failed("notice_open_failed");

        assert_eq!(done.colour(), colors::note::PASS);
        assert_eq!(check.colour(), colors::note::WARN);
        assert_eq!(failed.colour(), colors::note::FAIL);
        assert_ne!(done.colour(), failed.colour());
        assert_ne!(check.colour(), failed.colour());
    }

    /// **The frame is translated and the detail is not**, which is the split the type
    /// exists for: the engine's sentence names an ISO clause and has no other wording.
    #[test]
    fn a_notice_frames_a_detail_it_does_not_translate() {
        let locale = crate::locale::LocaleManager::new();
        let plain = Notice::done("notice_exported").about("out.pdf");
        assert_eq!(plain.say(&locale, "en"), "Exported to out.pdf");
        assert_eq!(plain.say(&locale, "ja"), "out.pdf に書き出しました");

        let engine = Notice::failed("notice_open_failed")
            .about("the file has no document catalogue (ISO 7.7.2)");
        assert_eq!(engine.level, Level::Failed);
        for lang in ["en", "ja"] {
            assert!(
                engine.say(&locale, lang).contains("ISO 7.7.2"),
                "the engine's own sentence survives every language"
            );
        }
    }

    /// A notice with nothing to say about takes the frame alone.
    #[test]
    fn a_notice_without_a_detail_is_its_frame() {
        let locale = crate::locale::LocaleManager::new();
        let n = Notice::failed("notice_save_nothing");
        assert_eq!(n.say(&locale, "en"), "There is no document to write.");
        assert!(!n.say(&locale, "ja").contains("{}"));
    }
}
